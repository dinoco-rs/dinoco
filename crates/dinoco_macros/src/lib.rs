use proc_macro::TokenStream;

mod models;

#[proc_macro]
pub fn dinoco_models(input: TokenStream) -> TokenStream {
    models::parse(input)
}
