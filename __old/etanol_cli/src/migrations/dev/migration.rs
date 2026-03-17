use std::fs::{create_dir, read_dir, write};
use std::path::PathBuf;

use chrono::Local;
use etanol_utils::EtanolError;

pub fn createMigration(name: String, content: String) -> String {
    let timestamp = Local::now().timestamp_millis();
    let name = format!("{}_{}", timestamp, name);

    let migrationDir = format!("etanol/migrations/{}", name);
    let migration = PathBuf::from(format!("{}/migration.sql", migrationDir));

    create_dir(PathBuf::from(migrationDir.clone())).unwrap();
    write(migration, content).unwrap();

    name
}

pub fn createMigrationFolder(name: String) {
    let migrations = "etanol/migrations";
    let migrationsPath = PathBuf::from(migrations);

    if migrationsPath.exists() {
        for file in read_dir(migrationsPath).unwrap() {
            let filename = file.unwrap().file_name();
            let filename = filename.to_str().unwrap();

            let mut splited = filename
                .split("_")
                .map(|x| x.to_string())
                .collect::<Vec<String>>();

            splited.remove(0);

            let filename = splited.join("_");

            if filename.trim() == name.trim() {
                EtanolError::new(
                    format!("Migration '{}' already exists", name),
                    "MigrationNameCollision".to_string(),
                );
            }
        }
    } else {
        create_dir(migrationsPath).unwrap();
    }
}
