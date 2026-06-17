use proc_macro::TokenStream;

use proc_macro_crate::{FoundCrate, crate_name};
use quote::{ToTokens, quote};
use syn::{
    Fields, GenericArgument, ItemStruct, LitStr, PathArguments, Result, Type, parse::Parse, parse::ParseStream,
    parse_macro_input,
};

pub fn parse(input: TokenStream) -> TokenStream {
    // let input = parse_macro_input!(input);

    // let expanded = quote! {
    //     #(#input)*

    //     // pub fn __dinoco_models() -> &'static [#dinoco_crate::DinocoModelMeta] {
    //     //     &[
    //     //         #(#models_tokens),*
    //     //     ]
    //     // }

    //     // pub fn __dinoco_relations() -> &'static [#dinoco_crate::DinocoRelationMeta] {
    //     //     &[
    //     //         #(#relations_tokens),*
    //     //     ]
    //     // }
    // };

    // expanded.into()

    TokenStream::new()
}
