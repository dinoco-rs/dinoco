use std::fs;
use std::path::{Path, PathBuf};

use bincode::{deserialize, serialize};

use dinoco_compiler::ParsedSchema;
use dinoco_engine::{DinocoAdapter, DinocoError, DinocoResult};

pub fn local_migration_names() -> DinocoResult<Vec<String>> {
    let migrations_dir = Path::new("dinoco/migrations");

    if !migrations_dir.exists() {
        return Ok(Vec::new());
    }

    let mut migration_names = fs::read_dir(migrations_dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| path.file_name().and_then(|value| value.to_str()).map(|value| value.to_string()))
        .collect::<Vec<_>>();

    migration_names.sort();

    Ok(migration_names)
}

pub fn latest_local_migration_name() -> DinocoResult<Option<String>> {
    Ok(local_migration_names()?.into_iter().last())
}

pub fn migration_dir(migration_name: &str) -> PathBuf {
    Path::new("dinoco/migrations").join(migration_name)
}

pub fn migration_sql_path(migration_name: &str) -> PathBuf {
    migration_dir(migration_name).join("migration.sql")
}

pub fn migration_schema_path(migration_name: &str) -> PathBuf {
    migration_dir(migration_name).join("schema.bin")
}

pub fn write_migration_schema(migration_name: &str, schema: &ParsedSchema) -> DinocoResult<()> {
    let schema_bytes = serialize(schema).map_err(|err| DinocoError::ParseError(err.to_string()))?;

    fs::write(migration_schema_path(migration_name), schema_bytes)
        .map_err(|err| DinocoError::ParseError(err.to_string()))
}

pub fn read_migration_schema(migration_name: &str) -> DinocoResult<ParsedSchema> {
    let schema_bytes = fs::read(migration_schema_path(migration_name))?;

    deserialize(&schema_bytes).map_err(|err| DinocoError::ParseError(err.to_string()))
}

pub fn read_latest_local_schema() -> DinocoResult<Option<ParsedSchema>> {
    let Some(migration_name) = latest_local_migration_name()? else {
        return Ok(None);
    };

    Ok(Some(read_migration_schema(&migration_name)?))
}

pub async fn execute_migration_file<T>(adapter: &T, migration_name: &str) -> DinocoResult<()>
where
    T: DinocoAdapter,
{
    let sql_path = migration_sql_path(migration_name);
    let sql_content = fs::read_to_string(&sql_path)?;

    execute_sql_script(adapter, &sql_content).await
}

pub async fn execute_sql_script<T>(adapter: &T, sql_content: &str) -> DinocoResult<()>
where
    T: DinocoAdapter,
{
    for statement in sql_content.split(';') {
        let clean_statement = statement.trim();

        if clean_statement.is_empty() {
            continue;
        }

        adapter.execute(clean_statement, &[]).await?;
    }

    Ok(())
}
