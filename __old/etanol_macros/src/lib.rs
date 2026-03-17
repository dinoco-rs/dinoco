use proc_macro::TokenStream;

#[proc_macro]
pub fn impl_value_trait(item: TokenStream) -> TokenStream {
    let data = format!(
        r"
        impl Value for {} {{
            fn toValue(&self, _default: Option<String>) -> String {{
                self.to_string().clone()
            }}             
        }}
    ",
        item.to_string()
    );

    data.parse().unwrap()
}

#[proc_macro]
pub fn impl_value_trait_option(item: TokenStream) -> TokenStream {
    let data = format!(
        r"
        impl Value for Option<{}> {{
            fn toValue(&self, _default: Option<String>) -> String {{ 
                match self {{ 
                    Some(value) => value.to_string(),
                    None => {{ 
                        if let Some(value) = _default {{ 
                            value.to_string()
                        }} else {{
                            {}.to_string()
                        }}
                    }}
                }}
            }}
        }}
    ",
        item.to_string(),
        "\"None\""
    );

    data.parse().unwrap()
}

