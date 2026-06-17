use proc_macro::TokenStream;

use proc_macro_crate::{FoundCrate, crate_name};
use quote::{ToTokens, quote};
use syn::{
    Fields, GenericArgument, ItemStruct, LitStr, PathArguments, Result, Type, parse::Parse, parse::ParseStream,
    parse_macro_input,
};

impl Parse for DinocoModelsInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut models = Vec::new();

        while !input.is_empty() {
            let item: ItemStruct = input.parse()?;
            models.push(item);
        }

        Ok(Self { models })
    }
}

pub fn parse(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DinocoModelsInput);

    let dinoco_crate = get_dinoco_crate_path();

    let mut metas = Vec::new();

    for item in &input.models {
        match parse_model_meta(item) {
            Ok(meta) => metas.push(meta),
            Err(err) => return err.to_compile_error().into(),
        }
    }

    if let Err(err) = validate_relations(&metas) {
        return err.to_compile_error().into();
    }

    let structs = input.models.iter().cloned().map(strip_dinoco_attrs_from_struct);

    let models_tokens = metas.iter().map(|model| {
        let rust_name = &model.rust_name;
        let model_name = &model.model_name;

        let fields = model.fields.iter().map(|field| {
            let name = &field.name;
            let ty = &field.ty;
            let primary_key = field.primary_key;
            let extra = field.extra;
            let relation = field.relation;

            quote! {
                #dinoco_crate::DinocoFieldMeta {
                    name: #name,
                    ty: #ty,
                    primary_key: #primary_key,
                    extra: #extra,
                    relation: #relation,
                }
            }
        });

        quote! {
            #dinoco_crate::DinocoModelMeta {
                rust_name: #rust_name,
                model_name: #model_name,
                fields: &[#(#fields),*],
            }
        }
    });

    let relations_tokens = metas.iter().flat_map(|model| {
        model.relations.iter().map(|relation| {
            let source_model = &relation.source_model;
            let field_name = &relation.field_name;
            let target_model = &relation.target_model;
            let relation_type = relation.kind.as_str();

            let foreign_key = match &relation.foreign_key {
                Some(value) => quote! { Some(#value) },
                None => quote! { None },
            };

            let references = match &relation.references {
                Some(value) => quote! { Some(#value) },
                None => quote! { None },
            };

            quote! {
                #dinoco_crate::DinocoRelationMeta {
                    source_model: #source_model,
                    field_name: #field_name,
                    target_model: #target_model,
                    relation_type: #relation_type,
                    foreign_key: #foreign_key,
                    references: #references,
                }
            }
        })
    });

    let expanded = quote! {
        #(#structs)*

        pub fn __dinoco_models() -> &'static [#dinoco_crate::DinocoModelMeta] {
            &[
                #(#models_tokens),*
            ]
        }

        pub fn __dinoco_relations() -> &'static [#dinoco_crate::DinocoRelationMeta] {
            &[
                #(#relations_tokens),*
            ]
        }
    };

    expanded.into()
}

fn strip_dinoco_attrs_from_struct(mut item: syn::ItemStruct) -> syn::ItemStruct {
    item.attrs.retain(|attr| !attr.path().is_ident("dinoco"));

    if let syn::Fields::Named(fields_named) = &mut item.fields {
        for field in &mut fields_named.named {
            field.attrs.retain(|attr| !attr.path().is_ident("dinoco"));
        }
    }

    item
}

fn get_dinoco_crate_path() -> proc_macro2::TokenStream {
    match crate_name("dinoco") {
        Ok(FoundCrate::Itself) => {
            quote! { crate }
        }
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote! { ::#ident }
        }
        Err(_) => {
            quote! { ::dinoco }
        }
    }
}

fn parse_model_meta(item: &ItemStruct) -> Result<ModelMeta> {
    let rust_name = item.ident.to_string();
    let mut model_name = rust_name.clone();

    for attr in &item.attrs {
        if !attr.path().is_ident("dinoco") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("model_name") {
                let value = meta.value()?;
                let value = value.parse::<LitStr>()?;

                model_name = value.value();

                return Ok(());
            }

            Err(meta.error("unknown dinoco model attribute"))
        })?;
    }

    let Fields::Named(fields_named) = &item.fields else {
        return Err(syn::Error::new_spanned(item, "Dinoco models only support structs with named fields"));
    };

    let mut fields = Vec::new();
    let mut relations = Vec::new();

    for field in &fields_named.named {
        let field_meta = parse_field_meta(&rust_name, field)?;

        if let Some(relation) = parse_relation_meta(&rust_name, field)? {
            relations.push(relation);
        }

        fields.push(field_meta);
    }

    Ok(ModelMeta { rust_name, model_name, fields, relations })
}

fn parse_field_meta(source_model: &str, field: &syn::Field) -> Result<FieldMeta> {
    let field_ident = field.ident.as_ref().ok_or_else(|| syn::Error::new_spanned(field, "field must have a name"))?;

    let name = field_ident.to_string();
    let ty = type_to_string(&field.ty);

    let mut primary_key = false;
    let mut extra = false;
    let mut relation = false;

    for attr in &field.attrs {
        if !attr.path().is_ident("dinoco") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("primary_key") {
                primary_key = true;
                return Ok(());
            }

            if meta.path.is_ident("extra") {
                extra = true;
                return Ok(());
            }

            if meta.path.is_ident("one_to_one")
                || meta.path.is_ident("one_to_many")
                || meta.path.is_ident("many_to_one")
                || meta.path.is_ident("many_to_many")
            {
                relation = true;
                return Ok(());
            }

            if meta.path.is_ident("foreign_key") || meta.path.is_ident("references") {
                let _ = meta.value()?.parse::<LitStr>()?;
                return Ok(());
            }

            Err(meta.error(format!("unknown dinoco field attribute on `{}`", source_model)))
        })?;
    }

    Ok(FieldMeta { name, ty, primary_key, extra, relation })
}

fn parse_relation_meta(source_model: &str, field: &syn::Field) -> Result<Option<RelationMeta>> {
    let field_ident = field.ident.as_ref().ok_or_else(|| syn::Error::new_spanned(field, "field must have a name"))?;

    let field_name = field_ident.to_string();

    let mut kind = None;
    let mut foreign_key = None;
    let mut references = None;

    for attr in &field.attrs {
        if !attr.path().is_ident("dinoco") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("one_to_one") {
                kind = Some(RelationKind::OneToOne);
                return Ok(());
            }

            if meta.path.is_ident("one_to_many") {
                kind = Some(RelationKind::OneToMany);
                return Ok(());
            }

            if meta.path.is_ident("many_to_one") {
                kind = Some(RelationKind::ManyToOne);
                return Ok(());
            }

            if meta.path.is_ident("many_to_many") {
                kind = Some(RelationKind::ManyToMany);
                return Ok(());
            }

            if meta.path.is_ident("foreign_key") {
                let value = meta.value()?;
                let value = value.parse::<LitStr>()?;

                foreign_key = Some(value.value());

                return Ok(());
            }

            if meta.path.is_ident("references") {
                let value = meta.value()?;
                let value = value.parse::<LitStr>()?;

                references = Some(value.value());

                return Ok(());
            }

            Ok(())
        })?;
    }

    let Some(kind) = kind else {
        return Ok(None);
    };

    let target_model = match kind {
        RelationKind::OneToMany | RelationKind::ManyToMany => extract_vec_inner_type(&field.ty).ok_or_else(|| {
            syn::Error::new_spanned(&field.ty, "#[dinoco(one_to_many)] and #[dinoco(many_to_many)] require Vec<T>")
        })?,

        RelationKind::OneToOne | RelationKind::ManyToOne => type_to_string(&field.ty),
    };

    Ok(Some(RelationMeta {
        source_model: source_model.to_string(),
        field_name,
        target_model,
        kind,
        foreign_key,
        references,
    }))
}

fn validate_relations(models: &[ModelMeta]) -> Result<()> {
    for model in models {
        for relation in &model.relations {
            let target_model = models.iter().find(|item| item.rust_name == relation.target_model).ok_or_else(|| {
                syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "relation `{}` on model `{}` points to `{}`, but target model was not found",
                        relation.field_name, relation.source_model, relation.target_model
                    ),
                )
            })?;

            if let Some(foreign_key) = &relation.foreign_key {
                let exists = target_model.fields.iter().any(|field| field.name == *foreign_key);

                if !exists {
                    return Err(syn::Error::new(
                        proc_macro2::Span::call_site(),
                        format!(
                            "relation `{}` on model `{}` uses foreign_key `{}`, but model `{}` does not have this field",
                            relation.field_name, relation.source_model, foreign_key, relation.target_model
                        ),
                    ));
                }
            }

            if let Some(references) = &relation.references {
                let source_model = models.iter().find(|item| item.rust_name == relation.source_model).unwrap();

                let exists = source_model.fields.iter().any(|field| field.name == *references);

                if !exists {
                    return Err(syn::Error::new(
                        proc_macro2::Span::call_site(),
                        format!(
                            "relation `{}` on model `{}` references `{}`, but model `{}` does not have this field",
                            relation.field_name, relation.source_model, references, relation.source_model
                        ),
                    ));
                }
            }
        }
    }

    Ok(())
}

fn type_to_string(ty: &Type) -> String {
    ty.to_token_stream().to_string().replace(' ', "")
}

fn extract_vec_inner_type(ty: &Type) -> Option<String> {
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

    let first_arg = args.args.first()?;

    let GenericArgument::Type(inner_ty) = first_arg else {
        return None;
    };

    Some(type_to_string(inner_ty))
}

impl RelationKind {
    fn as_str(self) -> &'static str {
        match self {
            RelationKind::OneToOne => "one_to_one",
            RelationKind::OneToMany => "one_to_many",
            RelationKind::ManyToOne => "many_to_one",
            RelationKind::ManyToMany => "many_to_many",
        }
    }
}
