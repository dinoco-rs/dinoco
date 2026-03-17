use etanol_compiler::{Parser, Tokenizer};

use std::fs::read_to_string;

fn main() {
    let tokenizer = Tokenizer::new(
        "data.etanol".to_string(),
        read_to_string("data.etanol").unwrap(),
    );

    let mut parser = Parser::new(tokenizer);

    parser.run();
}
