use etanol_databases::{Database, Migration};
use etanol_utils::readConfig;

use uuid::Uuid;

use etanol_utils::hash;

pub fn executeInDatabase(name: String, content: String) {
    let url = readConfig().take("database_url".to_string()).unwrap();

    Database::createConnection(url.value()).unwrap();

    let id = Uuid::new_v4().to_string();
    let checksum = hash(content.clone());

    Database::execute(migrationTable(), &vec![]).unwrap();
    Database::execute(Database::formatQuery(content), &vec![]).unwrap();

    let sql = "INSERT INTO _etanol_migrations (id, migration_name, checksum) VALUES(?1, ?2, ?3);";

    Database::execute(sql.to_string(), &vec![id, name, checksum]).unwrap();
}

pub fn migrationTable() -> String {
    let mut migration = Migration::new();

    let columns = vec![
        ("id".to_string(), "String".to_string(), true, false, None),
        (
            "checksum".to_string(),
            "String".to_string(),
            false,
            false,
            None,
        ),
        (
            "migration_name".to_string(),
            "String".to_string(),
            false,
            false,
            None,
        ),
    ];

    let table = migration.createTable("_etanol_migrations".to_string());

    for (name, columnType, isPrimary, isOptional, default) in columns {
        table
            .addColumn(name, columnType)
            .primaryKey(isPrimary)
            .nullable(isOptional)
            .default(default);
    }

    migration.make().join("")
}
