use etanol_utils::readConfig;

use super::add;

pub fn createMods(all: Vec<String>) {
    let mut models = String::new();
    let database = "pub mod connection;\nmod models;\n\npub use connection::*;\npub use models::*;";

    for file in all {
        models.push_str(&format!("pub mod {};\n", file.replace(".rs", "")));
    }

    add("src/database/models/mod.rs".to_string(), models);
    add("src/database/mod.rs".to_string(), database.to_string())
}

pub fn createConnection() {
    let url = readConfig().take("database_url".to_string()).unwrap();

    let content = format!(
        "use etanol::{{Database{}}};\n\npub fn create_connection() {{\n Database::createConnection({}).unwrap();\n }}",
        if url.isEnv { ", Env"} else { "" },
        if url.isEnv { format!("Env::take(\"{}\".to_string())", url.value) } else { format!("\"{}\".to_string()", url.value) }
    );

    add("src/database/connection.rs".to_string(), content);
}
