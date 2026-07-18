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
    let update_name = format_ident!("{}Update", name);
    let count_name = format_ident!("{}Count", name);
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

    let count_relation_fields = relations.iter().filter(|field| field.kind == FieldKind::HasMany).map(|field| {
        let ident = &field.ident;
        let target = field.target_ty.as_ref().expect("relation target");
        quote! { pub #ident: ::core::option::Option<<#target as ::dinoco_engine::DinocoEntity>::Count> }
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

        quote! {
            pub fn #ident(&self) -> ::dinoco::RelationCount<#name, #target> {
                ::dinoco::RelationCount::new(#relation_name, #references, #child_field)
            }
        }
    });

    let count_apply_impls = relations.iter().filter(|field| field.kind == FieldKind::HasMany).map(|field| {
        let relation_name = &field.name;
        let ident = &field.ident;
        let target = field.target_ty.as_ref().expect("relation target");

        quote! {
            impl ::dinoco::DinocoRelationCountApply<<#target as ::dinoco_engine::DinocoEntity>::Count> for #count_name {
                fn dinoco_apply_count(
                    &mut self,
                    relation: &'static str,
                    count: <#target as ::dinoco_engine::DinocoEntity>::Count,
                ) {
                    if relation == #relation_name {
                        self.#ident = ::core::option::Option::Some(count);
                    }
                }
            }
        }
    });

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
            quote! { #ident: row.try_get(#name).ok()? }
        })
        .collect::<Vec<_>>();

    let mysql_row_initializers = scalar_fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let name = &field.name;
            quote! { #ident: row.take(#name)? }
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
                    ::core::option::Option::None => ::dinoco_engine::DinocoValue::Null,
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
                    ::core::option::Option::Some(value) => ::dinoco_engine::FindWhere::Eq(#name, ::core::convert::Into::into(value)),
                    ::core::option::Option::None => ::dinoco_engine::FindWhere::Null(#name),
                }
            }
        } else {
            quote! { ::dinoco_engine::FindWhere::Eq(#name, ::core::convert::Into::into(&self.#ident)) }
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

        if let Some(inner) = vec_inner(ty) {
            Some(quote! {
                let mut nested_items = ::std::vec::Vec::new();

                for value in &self.#ident {
                    let mut item = <#inner as ::dinoco::InsertPayload<#inner>>::dinoco_insert_model(value);
                    <#inner as ::dinoco::DinocoBelongsTo<#name>>::dinoco_bind_parent(&mut item, parent);
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
                    <#inner as ::dinoco::DinocoBelongsTo<#name>>::dinoco_bind_parent(&mut item, parent);
                    ::dinoco::execute_insert_payloads::<#inner, #inner, #inner>(&[item], client).await?;
                }
            })
        }
    });

    let has_nested_insert = !insert_nested_relations.is_empty();

    let belongs_to_binders = relations.iter().filter_map(|field| {
        if field.kind != FieldKind::BelongsTo {
            return None;
        }

        let target = field.target_ty.as_ref()?;
        let inferred_foreign_key;
        let foreign_key = if let Some(foreign_key) = field.foreign_key.as_deref() {
            foreign_key
        } else {
            inferred_foreign_key = infer_belongs_to_foreign_key(&field.name, &scalar_fields)?;
            inferred_foreign_key.as_str()
        };
        let foreign_key_field = scalar_fields.iter().find(|scalar| scalar.name == foreign_key)?;
        let foreign_key_ident = &foreign_key_field.ident;
        let references = field.references.as_deref().unwrap_or("id");
        let references_ident = format_ident!("{}", references);

        if foreign_key_field.is_option {
            Some(quote! {
                impl ::dinoco::DinocoBelongsTo<#target> for #name {
                    fn dinoco_bind_parent(&mut self, parent: &#target) {
                        self.#foreign_key_ident = ::core::option::Option::Some(parent.#references_ident.clone());
                    }
                }
            })
        } else {
            Some(quote! {
                impl ::dinoco::DinocoBelongsTo<#target> for #name {
                    fn dinoco_bind_parent(&mut self, parent: &#target) {
                        self.#foreign_key_ident = parent.#references_ident.clone();
                    }
                }
            })
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
        }

        impl ::core::default::Default for #update_name {
            fn default() -> Self {
                Self {
                    #(#update_defaults,)*
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

        impl #count_name {
            #(#count_methods)*
        }

        impl ::core::default::Default for #name {
            fn default() -> Self {
                Self {
                    #(#default_scalar_initializers,)*
                    #(#relation_initializers,)*
                }
            }
        }

        impl #name {
            pub fn new(#(#new_params),*) -> Self {
                Self {
                    #(#new_scalar_initializers,)*
                    #(#relation_initializers,)*
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
            type Update = #update_name;
            type Count = #count_name;
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

        impl ::dinoco_engine::DinocoPostgres for #name {
            fn from_deadpool_posgres_row(row: &::dinoco_engine::DeadpoolPostgresRow) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#postgres_row_initializers,)*
                    #(#relation_initializers,)*
                })
            }

            fn from_deadpool_postgres_row(row: &::dinoco_engine::DeadpoolPostgresRow) -> ::core::option::Option<Self> {
                Self::from_deadpool_posgres_row(row)
            }

            fn from_postgres_row(row: &::dinoco_engine::PostgresRow) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#postgres_row_initializers,)*
                    #(#relation_initializers,)*
                })
            }
        }

        impl ::dinoco_engine::DinocoMysql for #name {
            fn from_mysql_row(row: &::dinoco_engine::MysqlRow) -> ::core::option::Option<Self> {
                let mut row = row.clone();

                ::core::option::Option::Some(Self {
                    #(#mysql_row_initializers,)*
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

        impl ::dinoco::DinocoInsertable for #name {
            const INSERT_FIELDS: &'static [&'static str] = &[#(#insert_field_names),*];

            fn dinoco_insert_values(&self) -> ::std::vec::Vec<::dinoco_engine::DinocoValue> {
                ::std::vec![#(#insert_values),*]
            }

            fn dinoco_insert_identity(&self) -> ::std::vec::Vec<::dinoco_engine::FindWhere> {
                ::std::vec![#(#insert_identity_conditions),*]
            }
        }

        impl ::dinoco::InsertPayload<#name> for #name {
            const HAS_NESTED: bool = #has_nested_insert;

            fn dinoco_insert_model(&self) -> #name {
                #name {
                    #(#insert_model_scalar_initializers,)*
                    #(#relation_initializers,)*
                }
            }

            fn dinoco_insert_nested<'a>(
                &'a self,
                parent: &'a #name,
                client: &'a ::dinoco_engine::DinocoClient,
            ) -> ::dinoco::InsertNestedFuture<'a> {
                ::std::boxed::Box::pin(async move {
                    #(#insert_nested_steps)*

                    Ok(())
                })
            }
        }

        impl ::dinoco::DinocoCountModel<#name> for #count_name {
            fn dinoco_set_total(&mut self, total: i64) {
                self.total = total;
            }
        }

        #(#relation_apply_impls)*
        #(#belongs_to_binders)*
        #(#count_apply_impls)*
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

            quote! { #ident: row.take(#index)? }
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
                client: &'a ::dinoco_engine::DinocoClient,
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

        impl ::dinoco_engine::DinocoPostgres for #name {
            fn from_deadpool_posgres_row(row: &::dinoco_engine::DeadpoolPostgresRow) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#postgres_row_initializers,)*
                    #(#relation_initializers,)*
                })
            }

            fn from_deadpool_postgres_row(row: &::dinoco_engine::DeadpoolPostgresRow) -> ::core::option::Option<Self> {
                Self::from_deadpool_posgres_row(row)
            }

            fn from_postgres_row(row: &::dinoco_engine::PostgresRow) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(Self {
                    #(#postgres_row_initializers,)*
                    #(#relation_initializers,)*
                })
            }
        }

        impl ::dinoco_engine::DinocoMysql for #name {
            fn from_mysql_row(row: &::dinoco_engine::MysqlRow) -> ::core::option::Option<Self> {
                let mut row = row.clone();

                ::core::option::Option::Some(Self {
                    #(#mysql_row_initializers,)*
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
        #insert_payload_impl
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
    primary_key: bool,
    default_value: Option<DefaultValue>,
    auto_generate: Option<AutoGenerate>,
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
        let mut primary_key = false;
        let mut default_value = None;
        let mut auto_generate = None;

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
                    primary_key = true;
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
                is_option: false,
                primary_key,
                default_value,
                auto_generate,
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
                    primary_key,
                    default_value,
                    auto_generate,
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
                    primary_key,
                    default_value,
                    auto_generate,
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
            primary_key,
            default_value,
            auto_generate,
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
    match field.kind {
        FieldKind::HasMany => true,
        FieldKind::BelongsTo => {
            let Some(foreign_key) = field.foreign_key.as_deref() else {
                return true;
            };

            !scalar_fields.iter().any(|scalar| scalar.name == foreign_key)
        }
        FieldKind::Scalar | FieldKind::Extra => false,
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
            quote! {
                if self.#ident == ::core::default::Default::default() {
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

    type_path.path.segments.last().is_some_and(|segment| segment.ident == "String")
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
