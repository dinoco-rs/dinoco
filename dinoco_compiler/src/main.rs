use dinoco_compiler::{compile, render_error};

fn main() {
    let raw_input = std::fs::read_to_string("schema.dinoco").unwrap();

    match compile(&raw_input) {
        Ok((schema, parsed)) => {
            // println!("SUCCESS: {:?}", parsed);

            println!("OK: True")
        }
        Err(errs) => {
            // println!("ERR: {:#?}", errs);

            for err in &errs {
                println!("{}\n", render_error(err, &raw_input));
            }
        }
    }
}
