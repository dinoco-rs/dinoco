extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Path, parse_macro_input};

#[proc_macro_derive(DinocoModel)]
pub fn dinoco_row_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let fields = if let syn::Data::Struct(data) = input.data {
        if let syn::Fields::Named(fields) = data.fields {
            fields.named
        } else {
            panic!("DinocoModel only supports structs with named fields");
        }
    } else {
        panic!("DinocoModel can only be derived for structs");
    };

    let getters = fields.iter().enumerate().map(|(i, f)| {
        let ident = &f.ident;
        quote! {
            #ident: row.get(#i)?
        }
    });

    let expanded = quote! {



        impl DinocoRow for #name {
            fn from_row<R: DinocoDatabaseRow>(row: &R) -> DinocoResult<Self> {
                Ok(Self {
                    #(#getters),*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(Selectable)]
pub fn selectable_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let fields = if let syn::Data::Struct(data) = input.data {
        if let syn::Fields::Named(fields) = data.fields {
            fields.named
        } else {
            panic!("Selectable only supports structs with named fields");
        }
    } else {
        panic!("Selectable can only be derived for structs");
    };

    let field_names: Vec<String> = fields.iter().map(|f| f.ident.as_ref().unwrap().to_string()).collect();

    let expanded = quote! {
        // impl DinocoModel for #name {
        //     fn select_fields() -> Vec<&'static str> {
        //         vec![#(#field_names),*]
        //     }

        //     fn select_fields() -> Vec<&'static str> {
        //         vec![#(#field_names),*]
        //     }
        // }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(DinocoExtend, attributes(extend))]
pub fn extend_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    let mut base_model: Option<Path> = None;

    for attr in &input.attrs {
        if attr.path().is_ident("extend") {
            let _ = attr.parse_nested_meta(|meta| {
                base_model = Some(meta.path.clone());
                Ok(())
            });
        }
    }

    let base_model = base_model.expect("Expected #[extend(ModelName)]");

    let fields = if let syn::Data::Struct(data) = input.data {
        if let syn::Fields::Named(fields) = data.fields {
            fields.named
        } else {
            panic!("DinocoExtend only supports structs with named fields");
        }
    } else {
        panic!("DinocoExtend can only be derived for structs");
    };

    let field_names: Vec<String> = fields.iter().map(|f| f.ident.as_ref().unwrap().to_string()).collect();

    let getters = fields.iter().enumerate().map(|(i, f)| {
        let ident = &f.ident;
        quote! {
            #ident: row.get(#i)?
        }
    });

    let expanded = quote! {
        impl DinocoModel for #struct_name {
            fn select_fields() -> Vec<&'static str> {
                vec![#(#field_names),*]
            }
        }

        impl DinocoRow for #struct_name {
            fn from_row<R: DinocoDatabaseRow>(row: &R) -> DinocoResult<Self> {
                Ok(Self {
                    #(#getters),*
                })
            }
        }
    };

    TokenStream::from(expanded)
}
