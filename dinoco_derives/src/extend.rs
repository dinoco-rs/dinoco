use proc_macro::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::parse_macro_input;

use crate::shared::{named_fields, runtime_crate};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let name = input.ident.clone();

    let fields = match named_fields(&input) {
        Ok(fields) => fields,
        Err(error) => return TokenStream::from(error.to_compile_error()),
    };

    let model = match extend_model(&input.attrs) {
        Ok(model) => model,
        Err(error) => return TokenStream::from(error.to_compile_error()),
    };
    // A generated model extends itself. `dinoco` already provides
    // `ProjectionModel` for every `Model + Projection<Model>`, so emitting a
    // second implementation here conflicts with that blanket implementation.
    // Custom projections still need the explicit mapping to their source model.
    let is_model_self_projection = model.is_ident(&name);
    let insertable = has_insertable_attr(&input.attrs);
    let crate_path = runtime_crate();

    let scalar_fields = fields
        .iter()
        .filter(|field| is_count_container_field(field) || !is_relation_field(&field.ty))
        .collect::<Vec<_>>();
    let count_fields = scalar_fields.iter().copied().filter(|field| is_count_field(field)).collect::<Vec<_>>();
    let count_container_field = scalar_fields.iter().copied().find(|field| is_count_container_field(field));
    let base_scalar_fields = scalar_fields
        .iter()
        .copied()
        .filter(|field| !is_count_field(field) && !is_count_container_field(field) && !is_virtual_field(field))
        .collect::<Vec<_>>();
    let relation_fields = fields
        .iter()
        .filter(|field| !is_count_container_field(field) && is_relation_field(&field.ty))
        .collect::<Vec<_>>();

    let scalar_field_validations = base_scalar_fields.iter().map(|field| {
        let ident = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let span = ident.span();

        quote_spanned! {span=>
            let _ = |model: &#model| {
                let _: &#ty = &model.#ident;
            };
        }
    });

    let relation_field_validations = if insertable {
        Vec::new()
    } else {
        relation_fields
            .iter()
            .map(|field| {
                let ident = field.ident.as_ref().unwrap();
                let span = ident.span();

                quote_spanned! {span=>
                    let _ = |include: &<#model as #crate_path::Model>::Include| {
                        let _ = include.#ident();
                    };
                }
            })
            .collect::<Vec<_>>()
    };

    let count_field_validations = count_fields.iter().map(|field| {
        let ident = field.ident.as_ref().unwrap();
        let relation_ident = format_ident!("{}", count_field_relation_name(ident));
        let span = ident.span();

        quote_spanned! {span=>
            let _ = |include: &<#model as #crate_path::Model>::Include| {
                let _ = include.#relation_ident();
            };
        }
    });

    let field_names =
        base_scalar_fields.iter().filter_map(|field| field.ident.as_ref()).map(|ident| quote! { stringify!(#ident) });
    let nested_name = format_ident!("__DinocoInsertNestedFor{}", name);
    let base_model_initializers = base_scalar_fields.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;

        Some(quote! { #ident: self.#ident })
    });
    let nested_field_defs = relation_fields.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;
        let ty = &field.ty;

        Some(quote! { #ident: #ty })
    });
    let nested_field_initializers = relation_fields.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;

        Some(quote! { #ident: self.#ident })
    });
    let nested_execute_steps = relation_fields.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;

        if is_connection_field(&field.ty) {
            return match relation_field_kind(&field.ty)? {
                RelationFieldKind::Many => Some(quote! {
                    #crate_path::execute_insert_connected_payloads(parent, self.#ident, client).await?;
                }),
                RelationFieldKind::Optional => Some(quote! {
                    if let ::core::option::Option::Some(connected) = self.#ident {
                        #crate_path::execute_insert_connected_payload(parent, connected, client).await?;
                    }
                }),
            };
        }

        match relation_field_kind(&field.ty)? {
            RelationFieldKind::Many => Some(quote! {
                #crate_path::execute_insert_related_payloads(parent, self.#ident, client).await?;
            }),
            RelationFieldKind::Optional => Some(quote! {
                if let ::core::option::Option::Some(related) = self.#ident {
                    #crate_path::execute_insert_related_payload(parent, related, client).await?;
                }
            }),
        }
    });

    let mut scalar_index = 0usize;
    let row_initializers = fields.iter().map(|field| {
        let ident = field.ident.as_ref().unwrap();

        if is_count_field(field) || is_count_container_field(field) || is_virtual_field(field) {
            quote! { #ident: ::core::default::Default::default() }
        } else if is_relation_field(&field.ty) {
            quote! { #ident: ::core::default::Default::default() }
        } else if let Some(inner_ty) = extract_option_inner(&field.ty) {
            let index = scalar_index;
            scalar_index += 1;

            quote! { #ident: row.get_optional::<#inner_ty>(#index)? }
        } else {
            let index = scalar_index;
            scalar_index += 1;

            quote! { #ident: row.get(#index)? }
        }
    });

    let relation_match_arms = if insertable {
        Vec::new()
    } else {
        relation_fields
            .iter()
            .filter_map(|field| {
                let ident = field.ident.as_ref()?;
                let loader = format_ident!("__dinoco_load_{}", ident);
                let loader_by_primary_key = format_ident!("__dinoco_load_{}_by_primary_key", ident);
                let inner_ty = relation_inner_type(&field.ty)?;
                let relation_kind = relation_field_kind(&field.ty)?;
                let (selected_loader, key_getter) =
                    if let Some(foreign_key_field) = find_relation_foreign_key_field(fields, &ident.to_string()) {
                        let foreign_key_ident = foreign_key_field.ident.as_ref()?;
                        let key_getter = if extract_option_inner(&foreign_key_field.ty).is_some() {
                            quote! { |item: &Self| item.#foreign_key_ident.clone() }
                        } else {
                            quote! { |item: &Self| ::core::option::Option::Some(item.#foreign_key_ident.clone()) }
                        };

                        (loader.clone(), key_getter)
                    } else {
                        let selected_loader = match relation_kind {
                            RelationFieldKind::Many => loader.clone(),
                            RelationFieldKind::Optional => loader_by_primary_key,
                        };
                        let key_getter = quote! { |item: &Self| ::core::option::Option::Some(item.id.clone()) };

                        (selected_loader, key_getter)
                    };

                match relation_kind {
                    RelationFieldKind::Many => Some(quote! {
                        stringify!(#ident) => {
                            let item_keys = items.iter().map(#key_getter).collect::<::std::vec::Vec<_>>();

                            tasks.push(#model::#selected_loader::<Self, #inner_ty, A>(
                                item_keys,
                                include,
                                client,
                                read_mode,
                                |item: &mut Self| &mut item.#ident,
                            )
                            );
                        }
                    }),
                    RelationFieldKind::Optional => Some(quote! {
                        stringify!(#ident) => {
                            let item_keys = items.iter().map(#key_getter).collect::<::std::vec::Vec<_>>();

                            tasks.push(#model::#selected_loader::<Self, #inner_ty, A>(
                                item_keys,
                                include,
                                client,
                                read_mode,
                                |item: &mut Self| &mut item.#ident,
                            )
                            );
                        }
                    }),
                }
            })
            .collect::<Vec<_>>()
    };

    let count_match_arms = count_fields.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;
        let relation_name = count_field_relation_name(ident);
        let loader = format_ident!("__dinoco_count_{}", relation_name);
        let relation_field_ident = format_ident!("{}", relation_name);
        let relation_name_literal = relation_name.clone();
        let key_getter = if let Some(foreign_key_field) =
            find_relation_foreign_key_field(fields, &relation_field_ident.to_string())
        {
            let foreign_key_ident = foreign_key_field.ident.as_ref()?;

            if extract_option_inner(&foreign_key_field.ty).is_some() {
                quote! { |item: &Self| item.#foreign_key_ident.clone() }
            } else {
                quote! { |item: &Self| ::core::option::Option::Some(item.#foreign_key_ident.clone()) }
            }
        } else {
            quote! { |item: &Self| ::core::option::Option::Some(item.id.clone()) }
        };

        Some(quote! {
            #relation_name_literal => {
                let item_keys = items.iter().map(#key_getter).collect::<::std::vec::Vec<_>>();

                tasks.push(#model::#loader::<Self, A>(
                    item_keys,
                    count,
                    client,
                    read_mode,
                    |item: &mut Self| &mut item.#ident,
                ));
            }
        })
    });
    let count_container_match_arms = count_container_field
        .into_iter()
        .flat_map(|field| {
            let count_ident = field.ident.as_ref().unwrap();
            let model_for_counts = model.clone();

            relation_fields
                .iter()
                .filter_map(move |relation_field| {
                    let model = model_for_counts.clone();
                    let relation_ident = relation_field.ident.as_ref()?;
                    let relation_name = relation_ident.to_string();
                    let loader = format_ident!("__dinoco_count_{}", relation_ident);
                    let key_getter =
                        if let Some(foreign_key_field) = find_relation_foreign_key_field(fields, &relation_name) {
                            let foreign_key_ident = foreign_key_field.ident.as_ref()?;

                            if extract_option_inner(&foreign_key_field.ty).is_some() {
                                quote! { |item: &Self| item.#foreign_key_ident.clone() }
                            } else {
                                quote! { |item: &Self| ::core::option::Option::Some(item.#foreign_key_ident.clone()) }
                            }
                        } else {
                            quote! { |item: &Self| ::core::option::Option::Some(item.id.clone()) }
                        };

                    Some(quote! {
                        #relation_name => {
                            let item_keys = items.iter().map(#key_getter).collect::<::std::vec::Vec<_>>();

                            tasks.push(#model::#loader::<Self, A>(
                                item_keys,
                                count,
                                client,
                                read_mode,
                                |item: &mut Self| &mut item.#count_ident.get_or_insert_with(::core::default::Default::default).#relation_ident,
                            ));
                        }
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let insert_payload_impl = if insertable {
        quote! {
            #[doc(hidden)]
            struct #nested_name {
                #(#nested_field_defs),*
            }

            impl #crate_path::InsertNested<#model> for #nested_name {
                fn execute<'a, A>(
                    self,
                    parent: &'a #model,
                    client: &'a #crate_path::DinocoClient<A>,
                ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = #crate_path::DinocoResult<()>> + ::std::marker::Send + 'a>>
                where
                    A: #crate_path::DinocoAdapter,
                {
                    Box::pin(async move {
                        #(#nested_execute_steps)*

                        Ok(())
                    })
                }
            }

            impl #crate_path::InsertPayload<#model> for #name {
                type Nested = #nested_name;

                fn split_insert_payload(self) -> (#model, Self::Nested) {
                    (
                        #model {
                            #(#base_model_initializers),*
                        },
                        #nested_name {
                            #(#nested_field_initializers),*
                        },
                    )
                }
            }
        }
    } else {
        quote! {}
    };
    let projection_model_impl = if is_model_self_projection {
        quote! {}
    } else {
        quote! {
            impl #crate_path::ProjectionModel for #name {
                type Model = #model;
            }
        }
    };

    TokenStream::from(quote! {
        #[doc(hidden)]
        #[allow(unused_imports)]
        const _: () = {
            use #crate_path::{
                DinocoAdapter as _,
                DinocoClient as _,
                DinocoGenericRow as _,
                DinocoResult as _,
                DinocoRow as _,
                IncludeLoaderFuture as _,
                IncludeNode as _,
                Projection as _,
                ReadMode as _,
            };

            #(#scalar_field_validations)*
            #(#relation_field_validations)*
            #(#count_field_validations)*
        };

        #projection_model_impl

        impl #crate_path::Projection<#model> for #name {
            fn columns() -> &'static [&'static str] {
                &[#(#field_names),*]
            }

            fn load_includes<'a, A>(
                items: &'a mut [Self],
                includes: &'a [#crate_path::IncludeNode],
                client: &'a #crate_path::DinocoClient<A>,
                read_mode: #crate_path::ReadMode,
            ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = #crate_path::DinocoResult<()>> + ::std::marker::Send + 'a>>
            where
                A: #crate_path::DinocoAdapter,
            {
                Box::pin(async move {
                    let mut tasks: ::std::vec::Vec<#crate_path::IncludeLoaderFuture<'a, Self>> = ::std::vec::Vec::new();

                    for include in includes {
                        match include.name {
                            #(#relation_match_arms)*
                            _ => {}
                        }
                    }

                    let appliers = #crate_path::futures::future::try_join_all(tasks).await?;

                    for apply in appliers {
                        apply(items);
                    }

                    Ok(())
                })
            }

            fn load_counts<'a, A>(
                items: &'a mut [Self],
                counts: &'a [#crate_path::CountNode],
                client: &'a #crate_path::DinocoClient<A>,
                read_mode: #crate_path::ReadMode,
            ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = #crate_path::DinocoResult<()>> + ::std::marker::Send + 'a>>
            where
                A: #crate_path::DinocoAdapter,
            {
                Box::pin(async move {
                    let mut tasks: ::std::vec::Vec<#crate_path::IncludeLoaderFuture<'a, Self>> = ::std::vec::Vec::new();

                    for count in counts {
                        match count.name {
                            #(#count_match_arms)*
                            #(#count_container_match_arms)*
                            _ => {}
                        }
                    }

                    let appliers = #crate_path::futures::future::try_join_all(tasks).await?;

                    for apply in appliers {
                        apply(items);
                    }

                    Ok(())
                })
            }
        }
        #insert_payload_impl

        impl #crate_path::DinocoRow for #name {
            fn from_row<R: #crate_path::DinocoGenericRow>(row: &R) -> #crate_path::DinocoResult<Self> {
                Ok(Self {
                    #(#row_initializers),*
                })
            }
        }
    })
}

fn is_count_field(field: &syn::Field) -> bool {
    let Some(ident) = &field.ident else {
        return false;
    };

    ident.to_string().ends_with("_count") && is_usize_type(&field.ty)
}

fn is_count_container_field(field: &syn::Field) -> bool {
    let Some(ident) = &field.ident else {
        return false;
    };

    ident == "_count" && extract_option_inner(&field.ty).is_some_and(is_custom_type)
}

fn is_virtual_field(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| attr.path().is_ident("dinoco_virtual"))
}

fn is_usize_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    let Some(segment) = type_path.path.segments.last() else {
        return false;
    };

    segment.ident == "usize"
}

fn count_field_relation_name(ident: &syn::Ident) -> String {
    ident.to_string().trim_end_matches("_count").to_string()
}

fn is_relation_field(ty: &syn::Type) -> bool {
    relation_field_kind(ty).is_some()
}

fn is_connection_field(ty: &syn::Type) -> bool {
    relation_inner_type(ty).is_some_and(is_connection_type)
}

fn relation_field_kind(ty: &syn::Type) -> Option<RelationFieldKind> {
    if extract_vec_inner(ty).is_some() {
        return Some(RelationFieldKind::Many);
    }

    if let Some(inner) = extract_option_inner(ty) {
        if is_custom_type(inner) {
            return Some(RelationFieldKind::Optional);
        }
    }

    None
}

fn relation_inner_type(ty: &syn::Type) -> Option<&syn::Type> {
    extract_vec_inner(ty).or_else(|| extract_option_inner(ty).filter(|inner| is_custom_type(inner)))
}

fn extract_option_inner(ty: &syn::Type) -> Option<&syn::Type> {
    extract_generic_inner(ty, "Option")
}

fn extract_vec_inner(ty: &syn::Type) -> Option<&syn::Type> {
    extract_generic_inner(ty, "Vec")
}

fn extract_generic_inner<'a>(ty: &'a syn::Type, wrapper: &str) -> Option<&'a syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;

    if segment.ident != wrapper {
        return None;
    }

    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };

    match arguments.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

fn is_custom_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    let Some(segment) = type_path.path.segments.last() else {
        return false;
    };
    let ident = segment.ident.to_string();

    if is_known_scalar_type_ident(&ident) {
        return false;
    }

    // Treat fully-qualified scalar types (e.g. `dinoco::Uuid`) as non-relation fields.
    if let Some(first_segment) = type_path.path.segments.first()
        && matches!(first_segment.ident.to_string().as_str(), "dinoco" | "chrono" | "serde_json")
    {
        return false;
    }

    // Heuristic for common scalar-like custom field names.
    if ident.ends_with("Id")
        || ident.ends_with("UUID")
        || ident.ends_with("Uuid")
        || ident.ends_with("Snowflake")
        || ident.ends_with("Date")
        || ident.ends_with("DateTime")
        || ident.ends_with("Json")
    {
        return false;
    }

    if ident.chars().all(|ch| ch.is_uppercase() || ch == '_' || ch.is_ascii_digit()) {
        return false;
    }

    !matches!(
        ident.as_str(),
        "String"
            | "bool"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
    )
}

fn is_known_scalar_type_ident(ident: &str) -> bool {
    matches!(
        ident,
        "String"
            | "bool"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "Uuid"
            | "RawUuid"
            | "Snowflake"
            | "DateTimeUtc"
            | "NaiveDate"
            | "JsonValue"
            | "Value"
    )
}

fn is_connection_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    let Some(segment) = type_path.path.segments.last() else {
        return false;
    };

    segment.ident.to_string().ends_with("Connection")
}

enum RelationFieldKind {
    Many,
    Optional,
}

fn find_relation_foreign_key_field<'a>(
    fields: &'a syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    relation_name: &str,
) -> Option<&'a syn::Field> {
    let expected = normalized_field_name(&format!("{relation_name}id"));

    fields
        .iter()
        .find(|field| field.ident.as_ref().is_some_and(|ident| normalized_field_name(&ident.to_string()) == expected))
}

fn normalized_field_name(value: &str) -> String {
    value.chars().filter(|ch| *ch != '_').flat_map(char::to_lowercase).collect()
}

fn extend_model(attrs: &[syn::Attribute]) -> syn::Result<syn::Path> {
    for attr in attrs {
        if attr.path().is_ident("extend") {
            return attr.parse_args::<syn::Path>();
        }
    }

    Err(syn::Error::new(proc_macro2::Span::call_site(), "missing #[extend(ModelName)] attribute"))
}

fn has_insertable_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("insertable"))
}

#[cfg(test)]
mod tests {
    use super::{RelationFieldKind, find_relation_foreign_key_field, relation_field_kind};
    use syn::{Data, DeriveInput, Fields};

    fn parse_fields(raw: &str) -> syn::punctuated::Punctuated<syn::Field, syn::token::Comma> {
        let input = format!("struct Projection {{ {raw} }}");
        let derive_input: DeriveInput = syn::parse_str(&input).expect("valid struct");
        let Data::Struct(data) = derive_input.data else {
            panic!("expected struct");
        };
        let Fields::Named(fields) = data.fields else {
            panic!("expected named fields");
        };

        fields.named
    }

    #[test]
    fn find_relation_foreign_key_field_accepts_camel_case() {
        let fields = parse_fields("playerId: Option<String>, player: Option<Player>, id: String");
        let field = find_relation_foreign_key_field(&fields, "player").expect("field should exist");

        assert_eq!(field.ident.as_ref().expect("named field").to_string(), "playerId");
    }

    #[test]
    fn find_relation_foreign_key_field_accepts_snake_case() {
        let fields = parse_fields("player_id: Option<String>, player: Option<Player>, id: String");
        let field = find_relation_foreign_key_field(&fields, "player").expect("field should exist");

        assert_eq!(field.ident.as_ref().expect("named field").to_string(), "player_id");
    }

    #[test]
    fn find_relation_foreign_key_field_accepts_flat_lowercase() {
        let fields = parse_fields("playerid: Option<String>, player: Option<Player>, id: String");
        let field = find_relation_foreign_key_field(&fields, "player").expect("field should exist");

        assert_eq!(field.ident.as_ref().expect("named field").to_string(), "playerid");
    }

    #[test]
    fn find_relation_foreign_key_field_rejects_unrelated_field() {
        let fields = parse_fields("teamid: Option<String>, player: Option<Player>, id: String");

        assert!(find_relation_foreign_key_field(&fields, "player").is_none());
    }

    #[test]
    fn relation_field_kind_does_not_treat_optional_uuid_as_relation() {
        let ty: syn::Type = syn::parse_str("Option<dinoco::Uuid>").expect("type should parse");

        assert!(relation_field_kind(&ty).is_none());
    }

    #[test]
    fn relation_field_kind_treats_optional_model_as_relation() {
        let ty: syn::Type = syn::parse_str("Option<Player>").expect("type should parse");

        assert!(matches!(relation_field_kind(&ty), Some(RelationFieldKind::Optional)));
    }
}
