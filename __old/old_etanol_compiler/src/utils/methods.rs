use crate::utils::{Pointer, Token};

pub fn readString(pointer: &mut Pointer) -> String {
    let startString = pointer.toEqual("Punctuation", "\"");
    if !startString {
        pointer.error("Expected '\"'");
    }

    let mut value = String::new();

    loop {
        if pointer.toEqual("Punctuation", "\"") {
            break;
        }

        match pointer.token.clone() {
            None | Some(Token::EOF) => {
                pointer.error("Expected a '\"'");
            }
            Some(data) => {
                value.push_str(&data.tokenValue());

                pointer.take(&data.tokenType());
            }
        }
    }

    value
}
