use crate::{Pointer, Token};

pub fn read_string(pointer: &mut Pointer) -> String {
    let mut value = String::new();

    pointer.equal_with_error("Punctuation", "\"", "Expected a '\"'");

    while !pointer.equal("Punctuation", "\"") {
        match pointer.token.clone() {
            None | Some(Token::EOF) => pointer.error("Expected a '\"'".to_string()),
            Some(token) => value.push_str(&token.value()),
        }

        pointer.next();
    }

    value
}
