use proc_macro_crate::FoundCrate;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, PathArguments, Type};

pub fn named_fields(input: &DeriveInput) -> syn::Result<&syn::punctuated::Punctuated<syn::Field, syn::token::Comma>> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => Ok(&fields.named),
            _ => Err(syn::Error::new_spanned(input, "this derive only supports structs with named fields")),
        },
        _ => Err(syn::Error::new_spanned(input, "this derive can only be used with structs")),
    }
}

pub fn runtime_crate() -> TokenStream {
    match proc_macro_crate::crate_name("dinoco") {
        Ok(FoundCrate::Itself) => quote!(::dinoco),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());

            quote!(::#ident)
        }
        Err(_) => match proc_macro_crate::crate_name("dinoco_engine") {
            Ok(FoundCrate::Itself) => quote!(::dinoco_engine),
            Ok(FoundCrate::Name(name)) => {
                let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());

                quote!(::#ident)
            }
            Err(_) => quote!(crate),
        },
    }
}

pub fn expand_rowable_impl(
    crate_path: &TokenStream,
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
) -> TokenStream {
    let getters = fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        let ty = &field.ty;

        if let Some(inner_ty) = extract_option_inner(ty) {
            quote! {
                #ident: row.get_optional::<#inner_ty>(#index)?
            }
        } else {
            quote! {
                #ident: row.get(#index)?
            }
        }
    });

    quote! {
        impl #crate_path::DinocoRow for #name {
            fn from_row<R: #crate_path::DinocoGenericRow>(row: &R) -> #crate_path::DinocoResult<Self> {
                Ok(Self {
                    #(#getters),*
                })
            }
        }
    }
}

fn extract_option_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;

        if segment.ident == "Option" {
            if let PathArguments::AngleBracketed(arguments) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner_ty)) = arguments.args.first() {
                    return Some(inner_ty);
                }
            }
        }
    }

    None
}
