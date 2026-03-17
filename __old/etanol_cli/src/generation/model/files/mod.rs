use std::fs::{create_dir_all, write};
use std::sync::{Arc, Mutex};

use std::process::Command;

mod create;
mod model;

pub use create::*;
pub use model::*;

struct File {
    name: String,
    content: String,
}

lazy_static::lazy_static! {
    static ref FILES: Arc<Mutex<Vec<File>>> = Arc::new(Mutex::new(vec![]));
}

fn add(name: String, content: String) {
    FILES.lock().unwrap().push(File { name, content });
}

pub fn execute() {
    create_dir_all("src/database/models").unwrap();

    for file in FILES.lock().unwrap().iter() {
        println!("Writing file '{}'", file.name);
        write(file.name.clone(), file.content.clone()).unwrap();

        println!("Formatting file '{}'\n", file.name);
        formatFile(file.name.clone());
    }
}

fn formatFile(path: String) {
    if cfg!(target_os = "windows") {
        Command::new("cmd")
            .arg("/C")
            .arg(format!("rustfmt {}", path))
            .output()
            .expect("failed to execute process")
    } else {
        Command::new("sh")
            .arg("-c")
            .arg(format!("rustfmt {}", path))
            .output()
            .expect("failed to execute process")
    };
}
