extern crate proc_macro;

use syn::{DeriveInput, PathArguments, Type, parse_macro_input};

use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(Rowable)]
pub fn rowable_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let fields = if let syn::Data::Struct(data) = input.data {
        if let syn::Fields::Named(fields) = data.fields {
            fields.named
        } else {
            panic!("Rowable only supports structs with named fields");
        }
    } else {
        panic!("Rowable can only be derived for structs");
    };

    let getters = fields.iter().enumerate().map(|(i, f)| {
        let ident = &f.ident;
        let ty = &f.ty;

        if let Some(inner_ty) = extract_option_inner(ty) {
            quote! {
                #ident: row.get_optional::<#inner_ty>(#i)?
            }
        } else {
            quote! {
                #ident: row.get(#i)?
            }
        }
    });

    let expanded = quote! {
        impl DinocoRow for #name {
            fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
                Ok(Self {
                    #(#getters),*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

fn extract_option_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;

        if segment.ident == "Option" {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                    return Some(inner_ty);
                }
            }
        }
    }

    None
}
