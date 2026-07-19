use proc_macro::TokenStream;
use syn::parse_macro_input;

use crate::shared::{expand_rowable_impl, named_fields, runtime_crate};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let name = input.ident.clone();

    let fields = match named_fields(&input) {
        Ok(fields) => fields,
        Err(error) => return TokenStream::from(error.to_compile_error()),
    };

    TokenStream::from(expand_rowable_impl(&runtime_crate(), &name, fields))
}
