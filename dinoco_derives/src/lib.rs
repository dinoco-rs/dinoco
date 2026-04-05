extern crate proc_macro;

mod extend;
mod rowable;
mod shared;

use proc_macro::TokenStream;

#[proc_macro_derive(Rowable)]
pub fn rowable_derive(input: TokenStream) -> TokenStream {
    rowable::derive(input)
}

#[proc_macro_derive(Extend, attributes(extend))]
pub fn extend_derive(input: TokenStream) -> TokenStream {
    extend::derive(input)
}
