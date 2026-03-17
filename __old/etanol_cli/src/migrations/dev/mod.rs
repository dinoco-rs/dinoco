use etanol_databases::Migration;
use etanol_utils::{readTables, readTokens};

use crate::executeInDatabase;
use crate::generation::{createConnection, createModel, createMods, execute};

mod migration;
use migration::{createMigration, createMigrationFolder};

pub fn migrate_dev(name: String) {
    readTokens();

    createMigrationFolder(name.clone());

    let mut migration = Migration::new();

    let mut allMods = vec![];

    for (name, columns) in readTables() {
        allMods.push(name.clone().to_lowercase());

        createModel(&mut migration, name, columns);
    }

    createMods(allMods);
    createConnection();

    execute();

    let content = migration.make().join("");

    executeInDatabase(
        createMigration(name.clone(), content.clone()),
        content.clone(),
    );
}
