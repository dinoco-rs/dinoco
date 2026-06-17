use proc_macro::TokenStream;

use syn::LitStr;
use syn::parse_macro_input;

use crate::parse_struct_fields;

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);

    let struct_name = input.ident.clone();

    let fields = match parse_struct_fields(input.data) {
        Ok(items) => items,
        Err(message) => {
            return syn::Error::new_spanned(struct_name, message.to_string()).to_compile_error().into();
        }
    };

    let mut model_name = struct_name.to_string();
    let mut flags = Vec::new();

    // Struct attributes:
    //
    // #[dinoco(model_name = "player")]
    for attr in &input.attrs {
        if !attr.path().is_ident("dinoco") {
            continue;
        }

        let result = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("model_name") {
                let value = meta.value()?;
                let value = value.parse::<LitStr>()?;

                model_name = value.value();

                return Ok(());
            }

            Err(meta.error("unknown dinoco model attribute"))
        });

        if let Err(err) = result {
            return err.to_compile_error().into();
        }
    }

    // Field attributes:
    //
    // #[dinoco(primary_key)]
    // #[dinoco(extra)]
    for field in fields.iter() {
        let field_name = match &field.ident {
            Some(ident) => ident.to_string(),
            None => continue,
        };

        for attr in &field.attrs {
            if !attr.path().is_ident("dinoco") {
                continue;
            }

            let result = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("primary_key") {
                    flags.push((field_name.clone(), "primary_key".to_string()));

                    return Ok(());
                }

                if meta.path.is_ident("extra") {
                    flags.push((field_name.clone(), "extra".to_string()));

                    return Ok(());
                }

                Err(meta.error("unknown dinoco field attribute"))
            });

            if let Err(err) = result {
                return err.to_compile_error().into();
            }
        }
    }

    println!("model_name: {:?}", model_name);
    println!("flags: {:?}", flags);

    TokenStream::new()
}
