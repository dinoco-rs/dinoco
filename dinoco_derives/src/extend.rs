use proc_macro::TokenStream;
use quote::{format_ident, quote};
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
    let crate_path = runtime_crate();

    let scalar_fields = fields.iter().filter(|field| !is_relation_field(&field.ty)).collect::<Vec<_>>();
    let relation_fields = fields.iter().filter(|field| is_relation_field(&field.ty)).collect::<Vec<_>>();

    let field_names =
        scalar_fields.iter().filter_map(|field| field.ident.as_ref()).map(|ident| quote! { stringify!(#ident) });

    let mut scalar_index = 0usize;
    let row_initializers = fields.iter().map(|field| {
        let ident = field.ident.as_ref().unwrap();

        if is_relation_field(&field.ty) {
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

    let relation_match_arms = relation_fields.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;
        let loader = format_ident!("__dinoco_load_{}", ident);
        let foreign_key_ident = format_ident!("{}Id", ident);
        let inner_ty = relation_inner_type(&field.ty)?;
        let uses_foreign_key =
            fields.iter().any(|candidate| candidate.ident.as_ref().is_some_and(|item| item == &foreign_key_ident));
        let key_getter = if uses_foreign_key {
            quote! { |item: &Self| ::core::option::Option::Some(item.#foreign_key_ident.clone()) }
        } else {
            quote! { |item: &Self| ::core::option::Option::Some(item.id.clone()) }
        };

        match relation_field_kind(&field.ty)? {
            RelationFieldKind::Many => Some(quote! {
                stringify!(#ident) => {
                    let item_keys = items.iter().map(#key_getter).collect::<::std::vec::Vec<_>>();

                    tasks.push(#model::#loader::<Self, #inner_ty, A>(
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

                    tasks.push(#model::#loader::<Self, #inner_ty, A>(
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
    });

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
        };

        impl #crate_path::Projection<#model> for #name {
            fn columns() -> &'static [&'static str] {
                &[#(#field_names),*]
            }

            fn load_includes<'a, A>(
                items: &'a mut [Self],
                includes: &'a [#crate_path::IncludeNode],
                client: &'a #crate_path::DinocoClient<A>,
                read_mode: #crate_path::ReadMode,
            ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = #crate_path::DinocoResult<()>> + 'a>>
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
        }

        impl #crate_path::DinocoRow for #name {
            fn from_row<R: #crate_path::DinocoGenericRow>(row: &R) -> #crate_path::DinocoResult<Self> {
                Ok(Self {
                    #(#row_initializers),*
                })
            }
        }
    })
}

fn is_relation_field(ty: &syn::Type) -> bool {
    relation_field_kind(ty).is_some()
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

enum RelationFieldKind {
    Many,
    Optional,
}

fn extend_model(attrs: &[syn::Attribute]) -> syn::Result<syn::Path> {
    for attr in attrs {
        if attr.path().is_ident("extend") {
            return attr.parse_args::<syn::Path>();
        }
    }

    Err(syn::Error::new(proc_macro2::Span::call_site(), "missing #[extend(ModelName)] attribute"))
}
