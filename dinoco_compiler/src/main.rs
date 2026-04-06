use dinoco_compiler::compile;

fn main() {
    let raw = std::fs::read_to_string("schema.dinoco").unwrap();

    match compile(&raw) {
        Ok(data) => println!("{:?}", data),
        Err(err) => println!("{:?}", err),
    }
}
