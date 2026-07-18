use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Field, Fields, GenericArgument, LitStr, PathArguments, Type, parse_macro_input};

#[proc_macro_derive(Entity, attributes(dinoco))]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match expand_entity(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_derive(EntityExtend, attributes(extend))]
pub fn derive_entity_extend(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match expand_entity_extend(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_derive(Extend, attributes(extend))]
pub fn derive_extend(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match expand_entity_extend(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_entity(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = input.ident;
    let table_name = parse_table_name(&input.attrs)?.ok_or_else(|| {
        syn::Error::new_spanned(&name, "Entity requires #[dinoco(table_name = \"...\")] on the struct")
    })?;

    let Data::Struct(data) = input.data else {
        return Err(syn::Error::new_spanned(name, "Entity can only be derived for structs"));
    };

    let Fields::Named(fields) = data.fields else {
        return Err(syn::Error::new_spanned(name, "Entity only supports structs with named fields"));
    };

    let where_name = format_ident!("{}Where", name);
    let order_by_name = format_ident!("{}OrderBy", name);
    let include_name = format_ident!("{}Include", name);
    let parent_snake = to_snake_case(&name.to_string());

    let mut scalar_fields = Vec::new();
    let mut relations = Vec::new();

    for field in fields.named.iter() {
        let parsed = ParsedField::new(field, &parent_snake)?;

        match parsed.kind {
            FieldKind::Scalar => scalar_fields.push(parsed),
            FieldKind::HasMany | FieldKind::BelongsTo => relations.push(parsed),
            FieldKind::Extra => {}
        }
    }

    let field_names = scalar_fields.iter().map(|field| {
        let name = &field.name;
        quote! { #name }
    });

    let where_fields = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        quote! { pub #ident: ::dinoco::Field<#ty> }
    });

    let where_defaults = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;
        quote! { #ident: ::dinoco::Field::new(#name) }
    });

    let order_by_fields = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        quote! { pub #ident: ::dinoco::OrderBy }
    });

    let order_by_defaults = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;
        quote! { #ident: ::dinoco::OrderBy::new(#name) }
    });

    let row_initializers = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;
        quote! { #ident: row.get(#name).ok()? }
    });

    let relation_initializers = relations.iter().map(|field| {
        let ident = &field.ident;
        quote! { #ident: ::core::default::Default::default() }
    });

    let relation_value_arms = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;

        if field.is_option {
            quote! { #name => self.#ident.as_ref().map(::core::convert::Into::into), }
        } else {
            quote! { #name => ::core::option::Option::Some((&self.#ident).into()), }
        }
    });

    let include_methods = relations.iter().map(|field| {
        let ident = &field.ident;
        let relation_name = &field.name;
        let target = field.target_ty.as_ref().expect("relation target");
        let foreign_key = field.foreign_key.as_deref().unwrap_or("id");
        let references = field.references.as_deref().unwrap_or("id");

        match field.kind {
            FieldKind::HasMany => {
                let child_field =
                    if field.foreign_key.is_some() { foreign_key.to_string() } else { format!("{}_id", parent_snake) };

                quote! {
                    pub fn #ident(&self) -> ::dinoco::HasMany<#name, #target> {
                        ::dinoco::HasMany::new(#relation_name, #references, #child_field)
                    }
                }
            }
            FieldKind::BelongsTo => {
                let parent_field = if field.foreign_key.is_some() {
                    foreign_key.to_string()
                } else if let Some(inferred_foreign_key) = infer_belongs_to_foreign_key(relation_name, &scalar_fields) {
                    inferred_foreign_key
                } else {
                    format!("{}_id", relation_name)
                };

                quote! {
                    pub fn #ident(&self) -> ::dinoco::BelongsTo<#name, #target> {
                        ::dinoco::BelongsTo::new(#relation_name, #parent_field, #references)
                    }
                }
            }
            FieldKind::Scalar | FieldKind::Extra => quote! {},
        }
    });

    let relation_apply_impls = relations.iter().map(|field| {
        let relation_name = &field.name;
        let ident = &field.ident;
        let target = field.target_ty.as_ref().expect("relation target");

        match field.kind {
            FieldKind::HasMany => quote! {
                impl ::dinoco::DinocoRelationApply<#target> for #name {
                    fn dinoco_apply_many(&mut self, relation: &'static str, values: ::std::vec::Vec<#target>) {
                        if relation == #relation_name {
                            self.#ident = values;
                        }
                    }

                    fn dinoco_apply_one(&mut self, _relation: &'static str, _value: ::core::option::Option<#target>) {}
                }
            },
            FieldKind::BelongsTo => quote! {
                impl ::dinoco::DinocoRelationApply<#target> for #name {
                    fn dinoco_apply_many(&mut self, _relation: &'static str, _values: ::std::vec::Vec<#target>) {}

                    fn dinoco_apply_one(&mut self, relation: &'static str, value: ::core::option::Option<#target>) {
                        if relation == #relation_name {
                            self.#ident = value;
                        }
                    }
                }
            },
            FieldKind::Scalar | FieldKind::Extra => quote! {},
        }
    });

    Ok(quote! {
        pub struct #where_name {
            #(#where_fields,)*
        }

        impl ::core::default::Default for #where_name {
            fn default() -> Self {
                Self {
                    #(#where_defaults,)*
                }
            }
        }

        pub struct #order_by_name {
            #(#order_by_fields,)*
        }

        impl ::core::default::Default for #order_by_name {
            fn default() -> Self {
                Self {
                    #(#order_by_defaults,)*
                }
            }
        }

        #[derive(Default)]
        pub struct #include_name {}

        impl #include_name {
            #(#include_methods)*
        }

        #[::dinoco::async_trait(?Send)]
        impl ::dinoco_engine::DinocoEntity for #name {
            const TABLE_NAME: &'static str = #table_name;
            const FIELDS: &'static [&'static str] = &[#(#field_names),*];

            type Where = #where_name;
            type OrderBy = #order_by_name;
            type Include = #include_name;
        }

        impl ::dinoco_engine::DinocoProjection<#name> for #name {
            const FIELDS: &'static [&'static str] = <#name as ::dinoco_engine::DinocoEntity>::FIELDS;
        }

        impl ::dinoco_engine::DinocoSqlite for #name {
            fn from_sqlite_row(row: &::dinoco_engine::SqliteRow<'_>) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#row_initializers,)*
                    #(#relation_initializers,)*
                })
            }
        }

        impl ::dinoco::DinocoRelationValue for #name {
            fn dinoco_relation_value(&self, field: &'static str) -> ::core::option::Option<::dinoco_engine::DinocoValue> {
                match field {
                    #(#relation_value_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }

        #(#relation_apply_impls)*
    })
}

fn expand_entity_extend(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = input.ident;
    let model = parse_extend_model(&input.attrs)?;

    let Data::Struct(data) = input.data else {
        return Err(syn::Error::new_spanned(name, "EntityExtend can only be derived for structs"));
    };

    let Fields::Named(fields) = data.fields else {
        return Err(syn::Error::new_spanned(name, "EntityExtend only supports structs with named fields"));
    };

    let mut scalar_fields = Vec::new();
    let mut relations = Vec::new();

    for field in fields.named.iter() {
        let parsed = ParsedExtendField::new(field)?;

        match parsed.kind {
            FieldKind::Scalar => scalar_fields.push(parsed),
            FieldKind::HasMany | FieldKind::BelongsTo => relations.push(parsed),
            FieldKind::Extra => {}
        }
    }

    let scalar_validations = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;

        quote! {
            let _ = |model: &#model| {
                let _: &#ty = &model.#ident;
            };
        }
    });

    let relation_validations = relations.iter().map(|field| {
        let ident = &field.ident;

        quote! {
            let _ = |include: &<#model as ::dinoco_engine::DinocoEntity>::Include| {
                let _ = include.#ident();
            };
        }
    });

    let field_names = scalar_fields.iter().map(|field| {
        let name = &field.name;
        quote! { #name }
    });

    let mut scalar_index = 0usize;
    let row_initializers = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let index = scalar_index;
        scalar_index += 1;

        quote! { #ident: row.get(#index).ok()? }
    });

    let relation_initializers = relations.iter().map(|field| {
        let ident = &field.ident;
        quote! { #ident: ::core::default::Default::default() }
    });

    let relation_value_arms = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;

        if field.is_option {
            quote! { #name => self.#ident.as_ref().map(::core::convert::Into::into), }
        } else {
            quote! { #name => ::core::option::Option::Some((&self.#ident).into()), }
        }
    });

    let relation_apply_impls = relations.iter().map(|field| {
        let relation_name = &field.name;
        let ident = &field.ident;
        let target = field.target_ty.as_ref().expect("relation target");

        match field.kind {
            FieldKind::HasMany => quote! {
                impl ::dinoco::DinocoRelationApply<#target> for #name {
                    fn dinoco_apply_many(&mut self, relation: &'static str, values: ::std::vec::Vec<#target>) {
                        if relation == #relation_name {
                            self.#ident = values;
                        }
                    }

                    fn dinoco_apply_one(&mut self, _relation: &'static str, _value: ::core::option::Option<#target>) {}
                }
            },
            FieldKind::BelongsTo => quote! {
                impl ::dinoco::DinocoRelationApply<#target> for #name {
                    fn dinoco_apply_many(&mut self, _relation: &'static str, _values: ::std::vec::Vec<#target>) {}

                    fn dinoco_apply_one(&mut self, relation: &'static str, value: ::core::option::Option<#target>) {
                        if relation == #relation_name {
                            self.#ident = value;
                        }
                    }
                }
            },
            FieldKind::Scalar | FieldKind::Extra => quote! {},
        }
    });

    Ok(quote! {
        #[doc(hidden)]
        const _: () = {
            #(#scalar_validations)*
            #(#relation_validations)*
        };

        impl ::dinoco_engine::DinocoProjection<#model> for #name {
            const FIELDS: &'static [&'static str] = &[#(#field_names),*];
        }

        impl ::dinoco_engine::DinocoSqlite for #name {
            fn from_sqlite_row(row: &::dinoco_engine::SqliteRow<'_>) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#row_initializers,)*
                    #(#relation_initializers,)*
                })
            }
        }

        impl ::dinoco::DinocoRelationValue for #name {
            fn dinoco_relation_value(&self, field: &'static str) -> ::core::option::Option<::dinoco_engine::DinocoValue> {
                match field {
                    #(#relation_value_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }

        #(#relation_apply_impls)*
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Scalar,
    HasMany,
    BelongsTo,
    Extra,
}

struct ParsedField {
    ident: syn::Ident,
    name: String,
    ty: Type,
    kind: FieldKind,
    target_ty: Option<Type>,
    foreign_key: Option<String>,
    references: Option<String>,
    is_option: bool,
}

impl ParsedField {
    fn new(field: &Field, parent_snake: &str) -> syn::Result<Self> {
        let ident = field.ident.clone().ok_or_else(|| syn::Error::new_spanned(field, "field must have a name"))?;
        let name = ident.to_string();
        let mut extra = false;
        let mut relation_kind = None;
        let mut foreign_key = None;
        let mut references = None;

        for attr in &field.attrs {
            if !attr.path().is_ident("dinoco") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("extra") {
                    extra = true;
                    return Ok(());
                }

                if meta.path.is_ident("one_to_many") || meta.path.is_ident("many_to_many") {
                    relation_kind = Some(FieldKind::HasMany);
                    return Ok(());
                }

                if meta.path.is_ident("many_to_one") || meta.path.is_ident("one_to_one") {
                    relation_kind = Some(FieldKind::BelongsTo);
                    return Ok(());
                }

                if meta.path.is_ident("foreign_key") {
                    let value = meta.value()?.parse::<LitStr>()?;
                    foreign_key = Some(value.value());
                    return Ok(());
                }

                if meta.path.is_ident("references") {
                    let value = meta.value()?.parse::<LitStr>()?;
                    references = Some(value.value());
                    return Ok(());
                }

                if meta.path.is_ident("primary_key") {
                    return Ok(());
                }

                Err(meta.error("unknown dinoco field attribute"))
            })?;
        }

        if extra {
            return Ok(Self {
                ident,
                name,
                ty: field.ty.clone(),
                kind: FieldKind::Extra,
                target_ty: None,
                foreign_key,
                references,
                is_option: false,
            });
        }

        if let Some(inner) = vec_inner(&field.ty) {
            if !is_u8(inner) {
                if relation_kind.is_none() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "relation fields require an explicit #[dinoco(...)] relation attribute",
                    ));
                }

                return Ok(Self {
                    ident,
                    name,
                    ty: field.ty.clone(),
                    kind: FieldKind::HasMany,
                    target_ty: Some(inner.clone()),
                    foreign_key,
                    references,
                    is_option: false,
                });
            }
        }

        if let Some(inner) = option_inner(&field.ty) {
            if is_custom_type(inner) {
                if relation_kind.is_none() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "relation fields require an explicit #[dinoco(...)] relation attribute",
                    ));
                }

                return Ok(Self {
                    ident,
                    name,
                    ty: field.ty.clone(),
                    kind: FieldKind::BelongsTo,
                    target_ty: Some(inner.clone()),
                    foreign_key,
                    references,
                    is_option: true,
                });
            }
        }

        Ok(Self {
            ident,
            name: if name == "id" && references.is_none() && foreign_key.is_none() { name } else { name },
            ty: field.ty.clone(),
            kind: FieldKind::Scalar,
            target_ty: None,
            foreign_key: if foreign_key.is_none() && relation_kind == Some(FieldKind::HasMany) {
                Some(format!("{parent_snake}_id"))
            } else {
                foreign_key
            },
            references,
            is_option: option_inner(&field.ty).is_some(),
        })
    }
}

struct ParsedExtendField {
    ident: syn::Ident,
    name: String,
    ty: Type,
    kind: FieldKind,
    target_ty: Option<Type>,
    is_option: bool,
}

impl ParsedExtendField {
    fn new(field: &Field) -> syn::Result<Self> {
        let ident = field.ident.clone().ok_or_else(|| syn::Error::new_spanned(field, "field must have a name"))?;
        let name = ident.to_string();

        if let Some(inner) = vec_inner(&field.ty) {
            if !is_u8(inner) {
                return Ok(Self {
                    ident,
                    name,
                    ty: field.ty.clone(),
                    kind: FieldKind::HasMany,
                    target_ty: Some(inner.clone()),
                    is_option: false,
                });
            }
        }

        if let Some(inner) = option_inner(&field.ty) {
            if is_custom_type(inner) {
                return Ok(Self {
                    ident,
                    name,
                    ty: field.ty.clone(),
                    kind: FieldKind::BelongsTo,
                    target_ty: Some(inner.clone()),
                    is_option: true,
                });
            }
        }

        Ok(Self {
            ident,
            name,
            ty: field.ty.clone(),
            kind: FieldKind::Scalar,
            target_ty: None,
            is_option: option_inner(&field.ty).is_some(),
        })
    }
}

fn parse_table_name(attrs: &[syn::Attribute]) -> syn::Result<Option<String>> {
    let mut table_name = None;

    for attr in attrs {
        if !attr.path().is_ident("dinoco") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("table_name") {
                let value = meta.value()?.parse::<LitStr>()?;
                table_name = Some(value.value());
                return Ok(());
            }

            Err(meta.error("unknown dinoco entity attribute"))
        })?;
    }

    Ok(table_name)
}

fn parse_extend_model(attrs: &[syn::Attribute]) -> syn::Result<syn::Path> {
    for attr in attrs {
        if attr.path().is_ident("extend") {
            return attr.parse_args::<syn::Path>();
        }
    }

    Err(syn::Error::new(proc_macro2::Span::call_site(), "missing #[extend(ModelName)] attribute"))
}

fn infer_belongs_to_foreign_key(relation_name: &str, scalar_fields: &[ParsedField]) -> Option<String> {
    let pascal = relation_name
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };

            first.to_uppercase().chain(chars).collect::<String>()
        })
        .collect::<String>();

    let candidates = [
        format!("{relation_name}Id"),
        format!("{relation_name}_id"),
        format!("{relation_name}ID"),
        format!("{relation_name}{pascal}Id"),
    ];

    candidates.into_iter().find(|candidate| scalar_fields.iter().any(|field| field.name == *candidate))
}

fn vec_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };

    let segment = type_path.path.segments.last()?;

    if segment.ident != "Vec" {
        return None;
    }

    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };

    let GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };

    Some(inner)
}

fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };

    let segment = type_path.path.segments.last()?;

    if segment.ident != "Option" {
        return None;
    }

    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };

    let GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };

    Some(inner)
}

fn is_custom_type(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };

    let Some(segment) = type_path.path.segments.last() else {
        return false;
    };

    let ident = segment.ident.to_string();

    if is_known_scalar_type_ident(&ident) {
        return false;
    }

    if let Some(first_segment) = type_path.path.segments.first()
        && matches!(first_segment.ident.to_string().as_str(), "dinoco" | "chrono" | "serde_json" | "uuid")
    {
        return false;
    }

    if ident.ends_with("Id")
        || ident.ends_with("ID")
        || ident.ends_with("Uuid")
        || ident.ends_with("UUID")
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

    true
}

fn is_known_scalar_type_ident(ident: &str) -> bool {
    matches!(
        ident,
        "String"
            | "str"
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
            | "Vec"
            | "Option"
            | "Uuid"
            | "RawUuid"
            | "Snowflake"
            | "DateTimeUtc"
            | "NaiveDate"
            | "JsonValue"
            | "Value"
    )
}

fn is_u8(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };

    type_path.path.segments.last().is_some_and(|segment| segment.ident == "u8")
}

fn to_snake_case(value: &str) -> String {
    let mut out = String::new();

    for (index, ch) in value.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                out.push('_');
            }

            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }

    out
}
