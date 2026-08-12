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

#[proc_macro_derive(DinocoEnum, attributes(dinoco))]
pub fn derive_dinoco_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match expand_dinoco_enum(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_dinoco_enum(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = input.ident;
    let Data::Enum(data) = input.data else {
        return Err(syn::Error::new_spanned(name, "DinocoEnum can only be derived for enums"));
    };

    let mut variants = Vec::new();

    for variant in data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(variant, "DinocoEnum only supports unit variants"));
        }

        let mut value = None;
        for attr in &variant.attrs {
            if !attr.path().is_ident("dinoco") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("value") {
                    value = Some(meta.value()?.parse::<LitStr>()?.value());
                    return Ok(());
                }

                Err(meta.error("unknown DinocoEnum variant attribute"))
            })?;
        }

        let ident = variant.ident;
        variants.push((ident.clone(), value.unwrap_or_else(|| ident.to_string())));
    }

    if variants.is_empty() {
        return Err(syn::Error::new_spanned(name, "DinocoEnum requires at least one variant"));
    }

    let enum_name = name.to_string();
    let value_arms = variants.iter().map(|(variant, value)| {
        quote! {
            #name::#variant => ::dinoco::DinocoValue::Enum(
                ::std::string::String::from(#enum_name),
                ::std::string::String::from(#value),
            ),
        }
    });
    let display_arms = variants.iter().map(|(variant, value)| {
        quote! { #name::#variant => formatter.write_str(#value), }
    });
    let from_str_arms = variants.iter().map(|(variant, value)| {
        quote! { #value => ::core::result::Result::Ok(Self::#variant), }
    });
    let sqlite_arms = variants.iter().map(|(variant, value)| {
        quote! { #value => ::core::result::Result::Ok(Self::#variant), }
    });
    let postgres_arms = variants.iter().map(|(variant, value)| {
        quote! { #value => ::core::result::Result::Ok(Self::#variant), }
    });
    let mysql_arms = variants.iter().map(|(variant, value)| {
        quote! { #value => ::core::result::Result::Ok(Self::#variant), }
    });

    Ok(quote! {
        impl ::core::fmt::Display for #name {
            fn fmt(
                &self,
                formatter: &mut ::core::fmt::Formatter<'_>,
            ) -> ::core::fmt::Result {
                match self {
                    #(#display_arms)*
                }
            }
        }

        impl ::core::str::FromStr for #name {
            type Err = ::std::string::String;

            fn from_str(
                value: &str,
            ) -> ::core::result::Result<
                Self,
                <Self as ::core::str::FromStr>::Err,
            > {
                match value {
                    #(#from_str_arms)*
                    _ => ::core::result::Result::Err(::std::format!(
                        "unknown value `{}` for enum `{}`",
                        value,
                        #enum_name,
                    )),
                }
            }
        }

        impl ::core::convert::TryFrom<&str> for #name {
            type Error = ::std::string::String;

            fn try_from(
                value: &str,
            ) -> ::core::result::Result<
                Self,
                <Self as ::core::convert::TryFrom<&str>>::Error,
            > {
                <Self as ::core::str::FromStr>::from_str(value)
            }
        }

        impl ::core::convert::TryFrom<::std::string::String> for #name {
            type Error = ::std::string::String;

            fn try_from(
                value: ::std::string::String,
            ) -> ::core::result::Result<
                Self,
                <Self as ::core::convert::TryFrom<::std::string::String>>::Error,
            > {
                <Self as ::core::convert::TryFrom<&str>>::try_from(value.as_str())
            }
        }

        impl ::core::convert::From<&#name> for ::dinoco::DinocoValue {
            fn from(value: &#name) -> Self {
                match value {
                    #(#value_arms)*
                }
            }
        }

        impl ::core::convert::From<#name> for ::dinoco::DinocoValue {
            fn from(value: #name) -> Self {
                ::dinoco::DinocoValue::from(&value)
            }
        }

        impl ::dinoco::IntoUpdateValue<#name> for #name {
            fn into_update_value(self) -> ::dinoco::DinocoValue {
                ::dinoco::DinocoValue::from(&self)
            }
        }

        impl ::dinoco::IntoUpdateValue<#name> for &#name {
            fn into_update_value(self) -> ::dinoco::DinocoValue {
                ::dinoco::DinocoValue::from(self)
            }
        }

        impl ::dinoco::rusqlite::types::FromSql for #name {
            fn column_result(
                value: ::dinoco::rusqlite::types::ValueRef<'_>,
            ) -> ::dinoco::rusqlite::types::FromSqlResult<Self> {
                let value =
                    <::std::string::String as ::dinoco::rusqlite::types::FromSql>::column_result(value)?;
                match value.as_str() {
                    #(#sqlite_arms)*
                    _ => ::core::result::Result::Err(
                        ::dinoco::rusqlite::types::FromSqlError::InvalidType,
                    ),
                }
            }
        }

        impl<'a> ::dinoco::tokio_postgres::types::FromSql<'a> for #name {
            fn from_sql(
                _ty: &::dinoco::tokio_postgres::types::Type,
                raw: &'a [u8],
            ) -> ::core::result::Result<
                Self,
                ::std::boxed::Box<dyn ::std::error::Error + Sync + Send>,
            > {
                let value = ::core::str::from_utf8(raw)?;
                match value {
                    #(#postgres_arms)*
                    _ => ::core::result::Result::Err(
                        ::std::format!("unknown enum value `{}`", value).into(),
                    ),
                }
            }

            fn accepts(ty: &::dinoco::tokio_postgres::types::Type) -> bool {
                matches!(ty.kind(), ::dinoco::tokio_postgres::types::Kind::Enum(_))
                    || <::std::string::String as ::dinoco::tokio_postgres::types::FromSql>::accepts(ty)
            }
        }

        impl ::dinoco::mysql_common::prelude::FromValue for #name {
            type Intermediate = Self;
        }

        impl ::core::convert::TryFrom<::dinoco::mysql_async::Value> for #name {
            type Error = ::dinoco::mysql_common::value::convert::FromValueError;

            fn try_from(
                value: ::dinoco::mysql_async::Value,
            ) -> ::core::result::Result<
                Self,
                ::dinoco::mysql_common::value::convert::FromValueError,
            > {
                let raw = value.clone();
                let value =
                    <::std::string::String as ::dinoco::mysql_common::prelude::FromValue>::from_value_opt(value)?;
                match value.as_str() {
                    #(#mysql_arms)*
                    _ => ::core::result::Result::Err(
                        ::dinoco::mysql_common::value::convert::FromValueError(raw),
                    ),
                }
            }
        }
    })
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
    let update_name = format_ident!("{}Update", name);
    let count_name = format_ident!("{}Count", name);
    let count_include_name = format_ident!("{}CountInclude", name);
    let parent_snake = to_snake_case(&name.to_string());

    let mut scalar_fields = Vec::new();
    let mut relations = Vec::new();
    let mut many_to_many_keys = Vec::new();

    for field in fields.named.iter() {
        let parsed = ParsedField::new(field, &parent_snake)?;

        match parsed.kind {
            FieldKind::Scalar => scalar_fields.push(parsed),
            FieldKind::HasMany | FieldKind::BelongsTo => relations.push(parsed),
            FieldKind::ManyToManyKey => many_to_many_keys.push(parsed),
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
        if !field.fulltext.is_empty() {
            quote! { pub #ident: ::dinoco::Field<#ty, ::dinoco::FullTextField> }
        } else {
            quote! { pub #ident: ::dinoco::Field<#ty> }
        }
    });

    let where_defaults = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;
        if field.fulltext.is_empty() {
            quote! { #ident: ::dinoco::Field::new(#name) }
        } else {
            let fields = field.fulltext.iter().map(|field| LitStr::new(field, ident.span()));
            quote! { #ident: ::dinoco::Field::new_fulltext(#name, &[#(#fields),*]) }
        }
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

    let update_fields = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        quote! { pub #ident: ::dinoco::UpdateField<#ty> }
    });

    let update_defaults = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;
        quote! { #ident: ::dinoco::UpdateField::new(#name) }
    });

    let many_to_many_update_fields = many_to_many_keys.iter().map(|field| {
        let ident = &field.ident;
        let ty = option_inner(&field.ty).unwrap_or(&field.ty);
        quote! { pub #ident: ::dinoco::ManyToManyUpdateField<#ty> }
    });

    let many_to_many_update_defaults = many_to_many_keys.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;
        let join_table = field.join_table.as_deref().expect("many-to-many join table");
        let parent_field = field.parent_field.as_deref().expect("many-to-many parent field");
        let join_parent_field = field.join_parent_field.as_deref().expect("many-to-many parent join field");
        let join_child_field = field.join_child_field.as_deref().expect("many-to-many child join field");
        quote! {
            #ident: ::dinoco::ManyToManyUpdateField::new(
                #name,
                #join_table,
                #parent_field,
                #join_parent_field,
                #join_child_field,
            )
        }
    });

    let count_relation_fields = relations.iter().filter(|field| field.kind == FieldKind::HasMany).map(|field| {
        let ident = &field.ident;
        quote! { pub #ident: ::core::option::Option<i64> }
    });

    let count_relation_defaults = relations.iter().filter(|field| field.kind == FieldKind::HasMany).map(|field| {
        let ident = &field.ident;
        quote! { #ident: ::core::default::Default::default() }
    });

    let count_methods = relations.iter().filter(|field| field.kind == FieldKind::HasMany).map(|field| {
        let ident = &field.ident;
        let relation_name = &field.name;
        let target = field.target_ty.as_ref().expect("relation target");
        let foreign_key = field.foreign_key.as_deref().unwrap_or("id");
        let references = field.references.as_deref().unwrap_or("id");
        let child_field =
            if field.foreign_key.is_some() { foreign_key.to_string() } else { format!("{}_id", parent_snake) };

        if field.many_to_many && field.join_table.is_some() {
            let join_table = field.join_table.as_deref().expect("many-to-many join table");
            let join_parent_field = field.join_parent_field.as_deref().expect("many-to-many parent join field");
            let join_child_field = field.join_child_field.as_deref().expect("many-to-many child join field");
            quote! {
                pub fn #ident(&self) -> ::dinoco::RelationCount<#name, #target> {
                    ::dinoco::RelationCount::<#name, #target>::many_to_many(
                        #relation_name,
                        #references,
                        #child_field,
                        #join_table,
                        #join_parent_field,
                        #join_child_field,
                    )
                }
            }
        } else {
            quote! {
                pub fn #ident(&self) -> ::dinoco::RelationCount<#name, #target> {
                    ::dinoco::RelationCount::<#name, #target>::new(#relation_name, #references, #child_field)
                }
            }
        }
    });

    let count_apply_arms = relations.iter().filter(|field| field.kind == FieldKind::HasMany).map(|field| {
        let relation_name = &field.name;
        let ident = &field.ident;

        quote! {
            #relation_name => {
                self.#ident = ::core::option::Option::Some(count);
            }
        }
    });

    let count_apply_impl = quote! {
        impl ::dinoco::DinocoRelationCountApply for #count_name {
            fn dinoco_apply_count(&mut self, relation: &'static str, count: i64) {
                match relation {
                    #(#count_apply_arms)*
                    _ => {}
                }
            }
        }
    };

    let row_initializers = scalar_fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let name = &field.name;
            quote! { #ident: row.get(#name).ok()? }
        })
        .collect::<Vec<_>>();

    let postgres_row_initializers = scalar_fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let name = &field.name;
            let value = postgres_row_value(&field.ty, field.is_option, quote! { #name });
            quote! { #ident: #value }
        })
        .collect::<Vec<_>>();

    let mysql_row_initializers = scalar_fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let name = &field.name;
            let value = mysql_row_value(&field.ty, field.is_option, quote! { #name });
            quote! { #ident: #value }
        })
        .collect::<Vec<_>>();

    let relation_initializers = relations
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: ::core::default::Default::default() }
        })
        .collect::<Vec<_>>();

    let many_to_many_key_initializers = many_to_many_keys
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: ::core::option::Option::None }
        })
        .collect::<Vec<_>>();

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

                if field.many_to_many && field.join_table.is_some() {
                    let join_table = field.join_table.as_deref().expect("many-to-many join table");
                    let join_parent_field = field.join_parent_field.as_deref().expect("many-to-many parent join field");
                    let join_child_field = field.join_child_field.as_deref().expect("many-to-many child join field");
                    quote! {
                        pub fn #ident(&self) -> ::dinoco::HasMany<#name, #target> {
                            ::dinoco::HasMany::many_to_many(
                                #relation_name,
                                #references,
                                #child_field,
                                #join_table,
                                #join_parent_field,
                                #join_child_field,
                            )
                        }
                    }
                } else {
                    quote! {
                        pub fn #ident(&self) -> ::dinoco::HasMany<#name, #target> {
                            ::dinoco::HasMany::new(#relation_name, #references, #child_field)
                        }
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
            FieldKind::Scalar | FieldKind::ManyToManyKey | FieldKind::Extra => quote! {},
        }
    });

    let relation_groups = group_relations_by_target(&relations);
    let relation_apply_impls = relation_groups.iter().map(|(target, fields)| {
        let many_arms = fields.iter().filter(|field| field.kind == FieldKind::HasMany).map(|field| {
            let relation_name = &field.name;
            let ident = &field.ident;
            quote! { #relation_name => self.#ident = values, }
        });
        let one_arms = fields.iter().filter(|field| field.kind == FieldKind::BelongsTo).map(|field| {
            let relation_name = &field.name;
            let ident = &field.ident;
            quote! { #relation_name => self.#ident = value, }
        });

        quote! {
            impl ::dinoco::DinocoRelationApply<#target> for #name {
                fn dinoco_apply_many(&mut self, relation: &'static str, values: ::std::vec::Vec<#target>) {
                    match relation {
                        #(#many_arms)*
                        _ => {}
                    }
                }

                fn dinoco_apply_one(&mut self, relation: &'static str, value: ::core::option::Option<#target>) {
                    match relation {
                        #(#one_arms)*
                        _ => {}
                    }
                }
            }
        }
    });

    let insert_fields = scalar_fields
        .iter()
        .filter(|field| field.auto_generate != Some(AutoGenerate::Autoincrement))
        .collect::<Vec<_>>();

    let insert_field_names = insert_fields.iter().map(|field| {
        let name = &field.name;
        quote! { #name }
    });

    let insert_values = insert_fields.iter().map(|field| {
        let ident = &field.ident;

        if field.is_option {
            quote! {
                match &self.#ident {
                    ::core::option::Option::Some(value) => ::core::convert::Into::into(value),
                    ::core::option::Option::None => ::dinoco::DinocoValue::Null,
                }
            }
        } else {
            quote! { ::core::convert::Into::into(&self.#ident) }
        }
    });

    let identity_fields = {
        let primary_fields = scalar_fields.iter().filter(|field| field.primary_key).collect::<Vec<_>>();

        if primary_fields.is_empty() { scalar_fields.iter().collect::<Vec<_>>() } else { primary_fields }
    };

    let insert_identity_conditions = identity_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;

        if field.is_option {
            quote! {
                match &self.#ident {
                    ::core::option::Option::Some(value) => ::dinoco::FindWhere::Eq(#name, ::core::convert::Into::into(value)),
                    ::core::option::Option::None => ::dinoco::FindWhere::Null(#name),
                }
            }
        } else {
            quote! { ::dinoco::FindWhere::Eq(#name, ::core::convert::Into::into(&self.#ident)) }
        }
    });

    let insert_model_scalar_initializers = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let value = insert_model_field_value(field);

        quote! { #ident: #value }
    });

    let default_scalar_initializers = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let value = default_field_value(field);
        quote! { #ident: #value }
    });

    let new_params = scalar_fields.iter().filter(|field| field.is_required_new_param()).map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        quote! { #ident: #ty }
    });

    let new_scalar_initializers = scalar_fields.iter().map(|field| {
        let ident = &field.ident;

        if field.is_required_new_param() {
            quote! { #ident }
        } else {
            let value = default_field_value(field);
            quote! { #ident: #value }
        }
    });

    let insert_nested_relations =
        relations.iter().filter(|field| should_insert_nested_relation(field, &scalar_fields)).collect::<Vec<_>>();

    let insert_nested_steps = insert_nested_relations.iter().filter_map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        let bind_relation = field.relation_name.clone().or_else(|| field.foreign_key.clone()).unwrap_or_else(|| {
            if field.kind == FieldKind::HasMany { format!("{parent_snake}_id") } else { field.name.clone() }
        });

        if let Some(inner) = vec_inner(ty) {
            Some(quote! {
                let mut nested_items = ::std::vec::Vec::new();

                for value in &self.#ident {
                    let mut item = <#inner as ::dinoco::InsertPayload<#inner>>::dinoco_insert_model(value);
                    <#inner as ::dinoco::DinocoBelongsTo<#name>>::dinoco_bind_parent_relation(
                        &mut item,
                        #bind_relation,
                        parent,
                    );
                    nested_items.push(item);
                }

                if !nested_items.is_empty() {
                    ::dinoco::execute_insert_payloads::<#inner, #inner, #inner>(&nested_items, client).await?;
                }
            })
        } else {
            let inner = option_inner(ty)?;

            Some(quote! {
                if let ::core::option::Option::Some(value) = &self.#ident {
                    let mut item = <#inner as ::dinoco::InsertPayload<#inner>>::dinoco_insert_model(value);
                    <#inner as ::dinoco::DinocoBelongsTo<#name>>::dinoco_bind_parent_relation(
                        &mut item,
                        #bind_relation,
                        parent,
                    );
                    ::dinoco::execute_insert_payloads::<#inner, #inner, #inner>(&[item], client).await?;
                }
            })
        }
    });

    let many_to_many_insert_steps = many_to_many_keys.iter().map(|field| {
        let ident = &field.ident;
        let field_name = &field.name;
        let join_table = field.join_table.as_deref().expect("many-to-many join table");
        let parent_field = field.parent_field.as_deref().expect("many-to-many parent field");
        let join_parent_field = field.join_parent_field.as_deref().expect("many-to-many parent join field");
        let join_child_field = field.join_child_field.as_deref().expect("many-to-many child join field");

        quote! {
            if let ::core::option::Option::Some(value) = &self.#ident {
                let parent_value = <#name as ::dinoco::DinocoRelationValue>::dinoco_relation_value(
                    parent,
                    #parent_field,
                )
                .ok_or_else(|| {
                    ::dinoco::anyhow::anyhow!(
                        "many-to-many key '{}' could not read parent field '{}'",
                        #field_name,
                        #parent_field,
                    )
                })?;
                let query = ::dinoco::InsertQuery {
                    table: #join_table,
                    fields: ::std::vec![#join_parent_field, #join_child_field],
                    rows: ::std::vec![::std::vec![parent_value, ::core::convert::Into::into(value)]],
                    returning: ::core::option::Option::None,
                };
                client.backend.insert(query).await?;
            }
        }
    });

    let many_to_many_transaction_insert_steps = many_to_many_keys.iter().map(|field| {
        let ident = &field.ident;
        let field_name = &field.name;
        let join_table = field.join_table.as_deref().expect("many-to-many join table");
        let parent_field = field.parent_field.as_deref().expect("many-to-many parent field");
        let join_parent_field = field.join_parent_field.as_deref().expect("many-to-many parent join field");
        let join_child_field = field.join_child_field.as_deref().expect("many-to-many child join field");
        let parent_is_autoincrement = scalar_fields
            .iter()
            .find(|scalar| scalar.name == parent_field)
            .map(|scalar| scalar.auto_generate == Some(AutoGenerate::Autoincrement))
            .unwrap_or(false);

        if parent_is_autoincrement {
            quote! {
                if self.#ident.is_some() {
                    ::dinoco::anyhow::bail!(
                        "many-to-many key '{}' cannot be connected by a transaction insert because parent field '{}' is autoincrement; insert the endpoint first and connect it after its ID is available",
                        #field_name,
                        #parent_field,
                    );
                }
            }
        } else {
            quote! {
                if let ::core::option::Option::Some(value) = &self.#ident {
                    let parent_value = <#name as ::dinoco::DinocoRelationValue>::dinoco_relation_value(
                        parent,
                        #parent_field,
                    )
                    .ok_or_else(|| {
                        ::dinoco::anyhow::anyhow!(
                            "many-to-many key '{}' could not read parent field '{}'",
                            #field_name,
                            #parent_field,
                        )
                    })?;
                    writes.push(::dinoco::ManyToManyWriteQuery {
                        parent_table: <#name as ::dinoco::DinocoEntity>::TABLE_NAME,
                        join_table: #join_table,
                        parent_field: #parent_field,
                        join_parent_field: #join_parent_field,
                        join_child_field: #join_child_field,
                        child_value: ::core::convert::Into::into(value),
                        parent_conditions: ::std::vec![::dinoco::FindWhere::Eq(#parent_field, parent_value)],
                    });
                }
            }
        }
    });

    let has_nested_insert = !insert_nested_relations.is_empty() || !many_to_many_keys.is_empty();
    let has_transaction_nested_insert = !insert_nested_relations.is_empty();

    let mut belongs_to_groups = Vec::<(&Type, Vec<(&ParsedField, &ParsedField)>)>::new();
    for field in &relations {
        if field.kind != FieldKind::BelongsTo {
            continue;
        }

        let Some(target) = field.target_ty.as_ref() else {
            continue;
        };
        let inferred_foreign_key;
        let foreign_key = if let Some(foreign_key) = field.foreign_key.as_deref() {
            foreign_key
        } else {
            let Some(inferred) = infer_belongs_to_foreign_key(&field.name, &scalar_fields) else {
                continue;
            };
            inferred_foreign_key = inferred;
            inferred_foreign_key.as_str()
        };
        let Some(foreign_key_field) = scalar_fields.iter().find(|scalar| scalar.name == foreign_key) else {
            continue;
        };

        let key = type_key(target);
        if let Some((_, bindings)) = belongs_to_groups.iter_mut().find(|(candidate, _)| type_key(candidate) == key) {
            bindings.push((field, foreign_key_field));
        } else {
            belongs_to_groups.push((target, vec![(field, foreign_key_field)]));
        }
    }

    let belongs_to_binders = belongs_to_groups.iter().map(|(target, bindings)| {
        let (default_relation, default_foreign_key) = bindings.first().expect("belongs-to binding");
        let default_foreign_key_ident = &default_foreign_key.ident;
        let default_references = default_relation.references.as_deref().unwrap_or("id");
        let default_references_ident = format_ident!("{}", default_references);
        let default_assignment = if default_foreign_key.is_option {
            quote! {
                self.#default_foreign_key_ident =
                    ::core::option::Option::Some(parent.#default_references_ident.clone());
            }
        } else {
            quote! { self.#default_foreign_key_ident = parent.#default_references_ident.clone(); }
        };

        let relation_arms = bindings.iter().map(|(field, foreign_key_field)| {
            let foreign_key = &foreign_key_field.name;
            let foreign_key_ident = &foreign_key_field.ident;
            let references = field.references.as_deref().unwrap_or("id");
            let references_ident = format_ident!("{}", references);
            let assignment = if foreign_key_field.is_option {
                quote! {
                    self.#foreign_key_ident = ::core::option::Option::Some(parent.#references_ident.clone());
                }
            } else {
                quote! { self.#foreign_key_ident = parent.#references_ident.clone(); }
            };

            if let Some(relation_name) = field.relation_name.as_deref().filter(|name| *name != foreign_key) {
                quote! { #relation_name | #foreign_key => { #assignment } }
            } else {
                quote! { #foreign_key => { #assignment } }
            }
        });

        quote! {
            impl ::dinoco::DinocoBelongsTo<#target> for #name {
                fn dinoco_bind_parent(&mut self, parent: &#target) {
                    #default_assignment
                }

                fn dinoco_bind_parent_relation(&mut self, relation: &'static str, parent: &#target) {
                    match relation {
                        #(#relation_arms)*
                        _ => self.dinoco_bind_parent(parent),
                    }
                }
            }
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

        pub struct #update_name {
            #(#update_fields,)*
            #(#many_to_many_update_fields,)*
        }

        impl ::core::default::Default for #update_name {
            fn default() -> Self {
                Self {
                    #(#update_defaults,)*
                    #(#many_to_many_update_defaults,)*
                }
            }
        }

        #[derive(Debug)]
        pub struct #count_name {
            pub total: i64,
            #(#count_relation_fields,)*
        }

        impl ::core::default::Default for #count_name {
            fn default() -> Self {
                Self {
                    total: 0,
                    #(#count_relation_defaults,)*
                }
            }
        }

        #[derive(Default)]
        pub struct #count_include_name {}

        impl #count_include_name {
            #(#count_methods)*
        }

        impl ::core::default::Default for #name {
            fn default() -> Self {
                Self {
                    #(#default_scalar_initializers,)*
                    #(#relation_initializers,)*
                    #(#many_to_many_key_initializers,)*
                }
            }
        }

        impl #name {
            pub fn new(#(#new_params),*) -> Self {
                Self {
                    #(#new_scalar_initializers,)*
                    #(#relation_initializers,)*
                    #(#many_to_many_key_initializers,)*
                }
            }
        }

        #[derive(Default)]
        pub struct #include_name {}

        impl #include_name {
            #(#include_methods)*
        }

        #[::dinoco::async_trait]
        impl ::dinoco::DinocoEntity for #name {
            const TABLE_NAME: &'static str = #table_name;
            const FIELDS: &'static [&'static str] = &[#(#field_names),*];

            type Where = #where_name;
            type OrderBy = #order_by_name;
            type Include = #include_name;
            type Update = #update_name;
            type Count = #count_name;
            type CountInclude = #count_include_name;
        }

        impl ::dinoco::DinocoProjection<#name> for #name {
            const FIELDS: &'static [&'static str] = <#name as ::dinoco::DinocoEntity>::FIELDS;
        }

        impl ::dinoco::DinocoSqlite for #name {
            fn from_sqlite_row(row: &::dinoco::SqliteRow<'_>) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#row_initializers,)*
                    #(#relation_initializers,)*
                    #(#many_to_many_key_initializers,)*
                })
            }
        }

        impl ::dinoco::DinocoPostgres for #name {
            fn from_deadpool_posgres_row(row: &::dinoco::DeadpoolPostgresRow) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#postgres_row_initializers,)*
                    #(#relation_initializers,)*
                    #(#many_to_many_key_initializers,)*
                })
            }

            fn from_deadpool_postgres_row(row: &::dinoco::DeadpoolPostgresRow) -> ::core::option::Option<Self> {
                Self::from_deadpool_posgres_row(row)
            }

            fn from_postgres_row(row: &::dinoco::PostgresRow) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#postgres_row_initializers,)*
                    #(#relation_initializers,)*
                    #(#many_to_many_key_initializers,)*
                })
            }
        }

        impl ::dinoco::DinocoMysql for #name {
            fn from_mysql_row(row: &::dinoco::MysqlRow) -> ::core::option::Option<Self> {
                let mut row = row.clone();

                ::core::option::Option::Some(Self {
                    #(#mysql_row_initializers,)*
                    #(#relation_initializers,)*
                    #(#many_to_many_key_initializers,)*
                })
            }
        }

        impl ::dinoco::DinocoRelationValue for #name {
            fn dinoco_relation_value(&self, field: &'static str) -> ::core::option::Option<::dinoco::DinocoValue> {
                match field {
                    #(#relation_value_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }

        impl ::dinoco::DinocoInsertable for #name {
            const INSERT_FIELDS: &'static [&'static str] = &[#(#insert_field_names),*];

            fn dinoco_insert_values(&self) -> ::std::vec::Vec<::dinoco::DinocoValue> {
                ::std::vec![#(#insert_values),*]
            }

            fn dinoco_insert_identity(&self) -> ::std::vec::Vec<::dinoco::FindWhere> {
                ::std::vec![#(#insert_identity_conditions),*]
            }
        }

        impl ::dinoco::InsertPayload<#name> for #name {
            const HAS_NESTED: bool = #has_nested_insert;
            const HAS_TRANSACTION_NESTED: bool = #has_transaction_nested_insert;

            fn dinoco_insert_model(&self) -> #name {
                #name {
                    #(#insert_model_scalar_initializers,)*
                    #(#relation_initializers,)*
                    #(#many_to_many_key_initializers,)*
                }
            }

            fn dinoco_insert_nested<'a>(
                &'a self,
                parent: &'a #name,
                client: &'a ::dinoco::DinocoClient,
            ) -> ::dinoco::InsertNestedFuture<'a> {
                ::std::boxed::Box::pin(async move {
                    #(#insert_nested_steps)*
                    #(#many_to_many_insert_steps)*

                    Ok(())
                })
            }

            fn dinoco_transaction_many_to_many_writes(
                &self,
                parent: &#name,
            ) -> ::dinoco::anyhow::Result<::std::vec::Vec<::dinoco::ManyToManyWriteQuery>> {
                let mut writes = ::std::vec::Vec::new();
                #(#many_to_many_transaction_insert_steps)*
                Ok(writes)
            }
        }

        impl ::dinoco::DinocoCountModel<#name> for #count_name {
            fn dinoco_set_total(&mut self, total: i64) {
                self.total = total;
            }
        }

        #(#relation_apply_impls)*
        #(#belongs_to_binders)*
        #count_apply_impl
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
            FieldKind::ManyToManyKey | FieldKind::Extra => {}
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
            let _ = |include: &<#model as ::dinoco::DinocoEntity>::Include| {
                let _ = include.#ident();
            };
        }
    });

    let field_names = scalar_fields.iter().map(|field| {
        let name = &field.name;
        quote! { #name }
    });

    let mut scalar_index = 0usize;
    let row_initializers = scalar_fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let index = scalar_index;
            scalar_index += 1;

            quote! { #ident: row.get(#index).ok()? }
        })
        .collect::<Vec<_>>();

    let mut scalar_index = 0usize;
    let postgres_row_initializers = scalar_fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let index = scalar_index;
            scalar_index += 1;

            quote! { #ident: row.try_get(#index).ok()? }
        })
        .collect::<Vec<_>>();

    let mut scalar_index = 0usize;
    let mysql_row_initializers = scalar_fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let index = scalar_index;
            scalar_index += 1;
            let value = mysql_row_value(&field.ty, field.is_option, quote! { #index });
            quote! { #ident: #value }
        })
        .collect::<Vec<_>>();

    let relation_initializers = relations
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: ::core::default::Default::default() }
        })
        .collect::<Vec<_>>();

    let relation_value_arms = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.name;

        if field.is_option {
            quote! { #name => self.#ident.as_ref().map(::core::convert::Into::into), }
        } else {
            quote! { #name => ::core::option::Option::Some((&self.#ident).into()), }
        }
    });

    let mut relation_groups = Vec::<(&Type, Vec<&ParsedExtendField>)>::new();
    for field in &relations {
        let target = field.target_ty.as_ref().expect("relation target");
        let key = type_key(target);
        if let Some((_, fields)) = relation_groups.iter_mut().find(|(candidate, _)| type_key(candidate) == key) {
            fields.push(field);
        } else {
            relation_groups.push((target, vec![field]));
        }
    }

    let relation_apply_impls = relation_groups.iter().map(|(target, fields)| {
        let many_arms = fields.iter().filter(|field| field.kind == FieldKind::HasMany).map(|field| {
            let relation_name = &field.name;
            let ident = &field.ident;
            quote! { #relation_name => self.#ident = values, }
        });
        let one_arms = fields.iter().filter(|field| field.kind == FieldKind::BelongsTo).map(|field| {
            let relation_name = &field.name;
            let ident = &field.ident;
            quote! { #relation_name => self.#ident = value, }
        });

        quote! {
            impl ::dinoco::DinocoRelationApply<#target> for #name {
                fn dinoco_apply_many(&mut self, relation: &'static str, values: ::std::vec::Vec<#target>) {
                    match relation {
                        #(#many_arms)*
                        _ => {}
                    }
                }

                fn dinoco_apply_one(&mut self, relation: &'static str, value: ::core::option::Option<#target>) {
                    match relation {
                        #(#one_arms)*
                        _ => {}
                    }
                }
            }
        }
    });

    let insert_model_assignments = scalar_fields.iter().map(|field| {
        let ident = &field.ident;
        quote! { model.#ident = self.#ident.clone(); }
    });

    let insert_nested_steps = relations.iter().filter(|field| field.kind == FieldKind::HasMany).filter_map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;

        if let Some(inner) = vec_inner(ty) {
            Some(quote! {
                let mut nested_items = ::std::vec::Vec::new();

                for value in &self.#ident {
                    let mut item = <#inner as ::dinoco::InsertPayload<#inner>>::dinoco_insert_model(value);
                    <#inner as ::dinoco::DinocoBelongsTo<#model>>::dinoco_bind_parent(&mut item, parent);
                    nested_items.push(item);
                }

                if !nested_items.is_empty() {
                    ::dinoco::execute_insert_payloads::<#inner, #inner, #inner>(&nested_items, client).await?;
                }
            })
        } else {
            let inner = option_inner(ty)?;

            Some(quote! {
                if let ::core::option::Option::Some(value) = &self.#ident {
                    let mut item = <#inner as ::dinoco::InsertPayload<#inner>>::dinoco_insert_model(value);
                    <#inner as ::dinoco::DinocoBelongsTo<#model>>::dinoco_bind_parent(&mut item, parent);
                    ::dinoco::execute_insert_payloads::<#inner, #inner, #inner>(&[item], client).await?;
                }
            })
        }
    });

    let has_nested_insert = relations.iter().any(|field| field.kind == FieldKind::HasMany);

    let insert_payload_impl = quote! {
        impl ::dinoco::InsertPayload<#model> for #name {
            const HAS_NESTED: bool = #has_nested_insert;

            fn dinoco_insert_model(&self) -> #model {
                let mut model = <#model as ::core::default::Default>::default();
                #(#insert_model_assignments)*
                model
            }

            fn dinoco_insert_nested<'a>(
                &'a self,
                parent: &'a #model,
                client: &'a ::dinoco::DinocoClient,
            ) -> ::dinoco::InsertNestedFuture<'a> {
                ::std::boxed::Box::pin(async move {
                    #(#insert_nested_steps)*

                    Ok(())
                })
            }
        }
    };

    Ok(quote! {
        #[doc(hidden)]
        const _: () = {
            #(#scalar_validations)*
            #(#relation_validations)*
        };

        impl ::dinoco::DinocoProjection<#model> for #name {
            const FIELDS: &'static [&'static str] = &[#(#field_names),*];
        }

        impl ::dinoco::DinocoSqlite for #name {
            fn from_sqlite_row(row: &::dinoco::SqliteRow<'_>) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#row_initializers,)*
                    #(#relation_initializers,)*
                })
            }
        }

        impl ::dinoco::DinocoPostgres for #name {
            fn from_deadpool_posgres_row(row: &::dinoco::DeadpoolPostgresRow) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#postgres_row_initializers,)*
                    #(#relation_initializers,)*
                })
            }

            fn from_deadpool_postgres_row(row: &::dinoco::DeadpoolPostgresRow) -> ::core::option::Option<Self> {
                Self::from_deadpool_posgres_row(row)
            }

            fn from_postgres_row(row: &::dinoco::PostgresRow) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#postgres_row_initializers,)*
                    #(#relation_initializers,)*
                })
            }
        }

        impl ::dinoco::DinocoMysql for #name {
            fn from_mysql_row(row: &::dinoco::MysqlRow) -> ::core::option::Option<Self> {
                let mut row = row.clone();

                ::core::option::Option::Some(Self {
                    #(#mysql_row_initializers,)*
                    #(#relation_initializers,)*
                })
            }
        }

        impl ::dinoco::DinocoRelationValue for #name {
            fn dinoco_relation_value(&self, field: &'static str) -> ::core::option::Option<::dinoco::DinocoValue> {
                match field {
                    #(#relation_value_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }

        #(#relation_apply_impls)*
        #insert_payload_impl
    })
}

fn group_relations_by_target(relations: &[ParsedField]) -> Vec<(&Type, Vec<&ParsedField>)> {
    let mut groups = Vec::<(&Type, Vec<&ParsedField>)>::new();

    for field in relations {
        let target = field.target_ty.as_ref().expect("relation target");
        let key = type_key(target);
        if let Some((_, fields)) = groups.iter_mut().find(|(candidate, _)| type_key(candidate) == key) {
            fields.push(field);
        } else {
            groups.push((target, vec![field]));
        }
    }

    groups
}

fn type_key(ty: &Type) -> String {
    quote! { #ty }.to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Scalar,
    HasMany,
    BelongsTo,
    ManyToManyKey,
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
    relation_name: Option<String>,
    is_option: bool,
    primary_key: bool,
    fulltext: Vec<String>,
    default_value: Option<DefaultValue>,
    auto_generate: Option<AutoGenerate>,
    many_to_many: bool,
    join_table: Option<String>,
    parent_field: Option<String>,
    join_parent_field: Option<String>,
    join_child_field: Option<String>,
}

enum DefaultValue {
    Expr(syn::Expr),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AutoGenerate {
    Uuid,
    Snowflake,
    Autoincrement,
}

impl ParsedField {
    fn is_required_new_param(&self) -> bool {
        self.kind == FieldKind::Scalar
            && !self.is_option
            && self.default_value.is_none()
            && self.auto_generate.is_none()
    }

    fn new(field: &Field, parent_snake: &str) -> syn::Result<Self> {
        let ident = field.ident.clone().ok_or_else(|| syn::Error::new_spanned(field, "field must have a name"))?;
        let name = ident.to_string();
        let mut extra = false;
        let mut relation_kind = None;
        let mut foreign_key = None;
        let mut references = None;
        let mut relation_name = None;
        let mut primary_key = false;
        let mut fulltext = Vec::new();
        let mut default_value = None;
        let mut auto_generate = None;
        let mut many_to_many = false;
        let mut join_table = None;
        let mut parent_field = None;
        let mut join_parent_field = None;
        let mut join_child_field = None;

        for attr in &field.attrs {
            if !attr.path().is_ident("dinoco") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("extra") {
                    extra = true;
                    return Ok(());
                }

                if meta.path.is_ident("one_to_many") {
                    relation_kind = Some(FieldKind::HasMany);
                    return Ok(());
                }

                if meta.path.is_ident("many_to_many") {
                    relation_kind = Some(FieldKind::HasMany);
                    many_to_many = true;
                    return Ok(());
                }

                if meta.path.is_ident("many_to_many_key") {
                    relation_kind = Some(FieldKind::ManyToManyKey);
                    many_to_many = true;
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

                if meta.path.is_ident("relation_name") {
                    relation_name = Some(meta.value()?.parse::<LitStr>()?.value());
                    return Ok(());
                }

                if meta.path.is_ident("join_table") {
                    join_table = Some(meta.value()?.parse::<LitStr>()?.value());
                    return Ok(());
                }

                if meta.path.is_ident("parent_field") {
                    parent_field = Some(meta.value()?.parse::<LitStr>()?.value());
                    return Ok(());
                }

                if meta.path.is_ident("join_parent_field") {
                    join_parent_field = Some(meta.value()?.parse::<LitStr>()?.value());
                    return Ok(());
                }

                if meta.path.is_ident("join_child_field") {
                    join_child_field = Some(meta.value()?.parse::<LitStr>()?.value());
                    return Ok(());
                }

                if meta.path.is_ident("primary_key") {
                    primary_key = true;
                    return Ok(());
                }

                if meta.path.is_ident("fulltext") {
                    if meta.input.peek(syn::Token![=]) {
                        let value = meta.value()?.parse::<LitStr>()?;
                        fulltext = value
                            .value()
                            .split(',')
                            .map(str::trim)
                            .filter(|field| !field.is_empty())
                            .map(ToString::to_string)
                            .collect();
                        if fulltext.is_empty() {
                            return Err(meta.error("fulltext requires at least one field"));
                        }
                    } else {
                        fulltext = vec![name.clone()];
                    }
                    return Ok(());
                }

                if meta.path.is_ident("default") {
                    let value = meta.value()?.parse::<syn::Expr>()?;
                    default_value = Some(DefaultValue::Expr(value));
                    return Ok(());
                }

                if meta.path.is_ident("auto_generate") {
                    let value = meta.value()?.parse::<syn::Path>()?;
                    let Some(segment) = value.segments.last() else {
                        return Err(meta.error("auto_generate requires uuid, snowflake, or autoincrement"));
                    };

                    auto_generate = Some(match segment.ident.to_string().as_str() {
                        "uuid" => AutoGenerate::Uuid,
                        "snowflake" => AutoGenerate::Snowflake,
                        "autoincrement" => AutoGenerate::Autoincrement,
                        _ => return Err(meta.error("auto_generate supports uuid, snowflake, or autoincrement")),
                    });
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
                relation_name,
                is_option: false,
                primary_key,
                fulltext,
                default_value,
                auto_generate,
                many_to_many,
                join_table,
                parent_field,
                join_parent_field,
                join_child_field,
            });
        }

        if relation_kind == Some(FieldKind::ManyToManyKey) {
            if option_inner(&field.ty).is_none() {
                return Err(syn::Error::new_spanned(field, "many-to-many virtual keys must be optional"));
            }
            if join_table.is_none()
                || parent_field.is_none()
                || join_parent_field.is_none()
                || join_child_field.is_none()
            {
                return Err(syn::Error::new_spanned(
                    field,
                    "many-to-many virtual keys require join_table, parent_field, join_parent_field, and join_child_field",
                ));
            }

            return Ok(Self {
                ident,
                name,
                ty: field.ty.clone(),
                kind: FieldKind::ManyToManyKey,
                target_ty: None,
                foreign_key,
                references,
                relation_name,
                is_option: true,
                primary_key,
                fulltext,
                default_value,
                auto_generate,
                many_to_many,
                join_table,
                parent_field,
                join_parent_field,
                join_child_field,
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
                    relation_name,
                    is_option: false,
                    primary_key,
                    fulltext,
                    default_value,
                    auto_generate,
                    many_to_many,
                    join_table,
                    parent_field,
                    join_parent_field,
                    join_child_field,
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
                    relation_name,
                    is_option: true,
                    primary_key,
                    fulltext,
                    default_value,
                    auto_generate,
                    many_to_many,
                    join_table,
                    parent_field,
                    join_parent_field,
                    join_child_field,
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
            relation_name,
            is_option: option_inner(&field.ty).is_some(),
            primary_key,
            fulltext,
            default_value,
            auto_generate,
            many_to_many,
            join_table,
            parent_field,
            join_parent_field,
            join_child_field,
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

fn should_insert_nested_relation(field: &ParsedField, scalar_fields: &[ParsedField]) -> bool {
    if field.many_to_many {
        return false;
    }

    match field.kind {
        FieldKind::HasMany => true,
        FieldKind::BelongsTo => {
            let Some(foreign_key) = field.foreign_key.as_deref() else {
                return true;
            };

            !scalar_fields.iter().any(|scalar| scalar.name == foreign_key)
        }
        FieldKind::Scalar | FieldKind::ManyToManyKey | FieldKind::Extra => false,
    }
}

fn default_field_value(field: &ParsedField) -> proc_macro2::TokenStream {
    if let Some(DefaultValue::Expr(value)) = &field.default_value {
        return quote! { #value };
    }

    match field.auto_generate {
        Some(AutoGenerate::Uuid) => quote! { ::dinoco::new_uuid() },
        Some(AutoGenerate::Snowflake) => quote! { ::dinoco::new_snowflake_id() },
        Some(AutoGenerate::Autoincrement) | None if field.is_option => quote! { ::core::option::Option::None },
        Some(AutoGenerate::Autoincrement) | None => quote! { ::core::default::Default::default() },
    }
}

fn insert_model_field_value(field: &ParsedField) -> proc_macro2::TokenStream {
    let ident = &field.ident;

    match field.auto_generate {
        Some(AutoGenerate::Uuid) if is_string(&field.ty) => {
            quote! {
                if self.#ident.is_empty() {
                    ::dinoco::new_uuid()
                } else {
                    self.#ident.clone()
                }
            }
        }
        Some(AutoGenerate::Snowflake) => {
            let ty = &field.ty;
            quote! {
                if self.#ident == <#ty as ::core::default::Default>::default() {
                    ::dinoco::new_snowflake_id()
                } else {
                    self.#ident.clone()
                }
            }
        }
        Some(AutoGenerate::Uuid) => default_field_value(field),
        Some(AutoGenerate::Autoincrement) | None => quote! { self.#ident.clone() },
    }
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

fn mysql_row_value(ty: &Type, is_option: bool, index: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let ty = option_inner(ty).unwrap_or(ty);
    let is_datetime =
        matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "DateTime"));

    if !is_datetime {
        return quote! { row.take(#index)? };
    }

    if is_option {
        quote! {
            row.take::<::core::option::Option<::dinoco::chrono::NaiveDateTime>, _>(#index)?
                .map(|value| value.and_utc())
        }
    } else {
        quote! {
            row.take::<::dinoco::chrono::NaiveDateTime, _>(#index)?.and_utc()
        }
    }
}

fn postgres_row_value(ty: &Type, is_option: bool, index: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let ty = option_inner(ty).unwrap_or(ty);
    let is_datetime =
        matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "DateTime"));

    if !is_datetime {
        return quote! { row.try_get(#index).ok()? };
    }

    if is_option {
        quote! { ::dinoco::postgres_optional_datetime_from_row(row, #index)? }
    } else {
        quote! { ::dinoco::postgres_datetime_from_row(row, #index)? }
    }
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

fn is_string(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };

    type_path.path.segments.last().is_some_and(|segment| segment.ident == "String" || segment.ident == "Uuid")
}

fn to_snake_case(value: &str) -> String {
    let mut out = String::new();
    let chars = value.chars().collect::<Vec<_>>();

    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_uppercase() {
            let previous_is_lowercase_or_digit =
                index > 0 && (chars[index - 1].is_lowercase() || chars[index - 1].is_ascii_digit());
            let starts_word_after_acronym = index > 0
                && chars[index - 1].is_uppercase()
                && chars.get(index + 1).is_some_and(|next| next.is_lowercase());
            if previous_is_lowercase_or_digit || starts_word_after_acronym {
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
