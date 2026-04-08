use async_trait::async_trait;
use dinoco_derives::Rowable;

use super::SqliteAdapter;
use crate::{
    DatabaseColumn, DatabaseEnumRaw, DatabaseForeignKey, DatabaseIndex, DatabaseParsedTable, DatabaseTable,
    DinocoAdapter, DinocoAdapterHandler, DinocoResult, DinocoValue,
};

#[derive(Rowable, Debug)]
struct SqliteTableSql {
    sql: Option<String>,
}

#[async_trait]
impl DinocoAdapterHandler for SqliteAdapter {
    async fn reset_database(&self) -> DinocoResult<()> {
        self.execute("PRAGMA foreign_keys = OFF;", &[]).await?;

        let tables = self.fetch_tables().await?;

        for table in tables {
            let query = format!("DROP TABLE IF EXISTS \"{}\";", table.name);

            self.execute(&query, &[]).await?;
        }

        self.execute("PRAGMA foreign_keys = ON;", &[]).await?;

        Ok(())
    }

    async fn fetch_tables(&self) -> DinocoResult<Vec<DatabaseParsedTable>> {
        let query = "
            SELECT name 
            FROM sqlite_master 
            WHERE type = 'table' 
              AND name NOT LIKE 'sqlite_%';
        ";

        let mut tables = vec![];

        for table in self.query_as::<DatabaseTable>(query, &[]).await? {
            let columns = self.fetch_columns(table.name.clone()).await?;

            tables.push(DatabaseParsedTable { name: table.name, columns })
        }

        Ok(tables)
    }

    async fn fetch_columns(&self, table_name: String) -> DinocoResult<Vec<DatabaseColumn>> {
        let query = "
            SELECT 
                name,
                type AS db_type,
                -- No SQLite, notnull é 1 (se for NOT NULL) e 0 (se permitir NULL).
                (\"notnull\" = 0) AS nullable,
                (pk = 1) AS is_primary_key,
                dflt_value AS default_value,
                NULL AS enum_values
            FROM pragma_table_info(?);
        ";

        let mut columns = self.query_as::<DatabaseColumn>(query, &[DinocoValue::from(table_name.clone())]).await?;
        let create_table_sql = self.fetch_create_table_sql(&table_name).await?;

        if let Some(sql) = create_table_sql {
            let enum_values_by_column = parse_sqlite_inline_enums(&sql);

            for column in &mut columns {
                column.enum_values = enum_values_by_column.get(&column.name).cloned();
            }
        }

        Ok(columns)
    }

    async fn fetch_foreign_keys(&self) -> DinocoResult<Vec<DatabaseForeignKey>> {
        let query = "
            SELECT 
                m.name AS table_name,
                -- SQLite não nomeia constraints de FK. Usamos o ID interno gerado pelo PRAGMA.
                CAST(fk.id AS TEXT) AS constraint_name, 
                fk.\"from\" AS column_name,
                fk.\"table\" AS foreign_table_name,
                fk.\"to\" AS foreign_column_name
            FROM sqlite_master m
            JOIN pragma_foreign_key_list(m.name) fk
            WHERE m.type = 'table' 
              AND m.name != '_dinoco_migrations';
        ";

        self.query_as::<DatabaseForeignKey>(query, &[]).await
    }

    async fn fetch_enums(&self) -> DinocoResult<Vec<DatabaseEnumRaw>> {
        Ok(vec![])
    }

    async fn fetch_indexes(&self) -> DinocoResult<Vec<DatabaseIndex>> {
        let query = "
            SELECT 
                m.name AS table_name,
                il.name AS index_name,
                ii.name AS column_name,
                -- No SQLite, unique vem como 1 ou 0
                (il.\"unique\" = 1) AS is_unique
            FROM sqlite_master m
            JOIN pragma_index_list(m.name) il
            JOIN pragma_index_info(il.name) ii
            WHERE m.type = 'table' 
              AND m.name != '_dinoco_migrations'
              -- Ignora índices gerados automaticamente para as Primary Keys
              AND il.origin != 'pk';
        ";

        self.query_as::<DatabaseIndex>(query, &[]).await
    }
}

impl SqliteAdapter {
    async fn fetch_create_table_sql(&self, table_name: &str) -> DinocoResult<Option<String>> {
        let query = "
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table'
              AND name = ?;
        ";

        let rows = self.query_as::<SqliteTableSql>(query, &[DinocoValue::from(table_name.to_string())]).await?;

        Ok(rows.into_iter().next().and_then(|row| row.sql))
    }
}

fn parse_sqlite_inline_enums(create_table_sql: &str) -> std::collections::HashMap<String, String> {
    let mut enum_values_by_column = std::collections::HashMap::new();
    let Some(columns_sql) = between_outer_parentheses(create_table_sql) else {
        return enum_values_by_column;
    };

    for column_definition in split_top_level_definitions(columns_sql) {
        let trimmed = column_definition.trim();

        if trimmed.is_empty() || is_table_constraint(trimmed) {
            continue;
        }

        let Some(column_name) = extract_column_name(trimmed) else {
            continue;
        };

        let Some(enum_values) = extract_inline_enum_values(trimmed, &column_name) else {
            continue;
        };

        enum_values_by_column.insert(column_name, enum_values.join("|"));
    }

    enum_values_by_column
}

fn between_outer_parentheses(sql: &str) -> Option<&str> {
    let start = sql.find('(')?;
    let end = sql.rfind(')')?;

    (start < end).then_some(&sql[start + 1..end])
}

fn split_top_level_definitions(sql: &str) -> Vec<&str> {
    let mut definitions = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut in_string = false;
    let chars = sql.char_indices().collect::<Vec<_>>();

    for (index, ch) in &chars {
        match ch {
            '\'' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string && depth > 0 => depth -= 1,
            ',' if !in_string && depth == 0 => {
                definitions.push(sql[start..*index].trim());
                start = *index + 1;
            }
            _ => {}
        }
    }

    if start < sql.len() {
        definitions.push(sql[start..].trim());
    }

    definitions
}

fn is_table_constraint(definition: &str) -> bool {
    let normalized = definition.trim_start().to_ascii_lowercase();

    normalized.starts_with("constraint ")
        || normalized.starts_with("primary key")
        || normalized.starts_with("foreign key")
        || normalized.starts_with("unique")
        || normalized.starts_with("check ")
}

fn extract_column_name(definition: &str) -> Option<String> {
    let trimmed = definition.trim_start();

    if let Some(rest) = trimmed.strip_prefix('"') {
        let end = rest.find('"')?;

        return Some(rest[..end].to_string());
    }

    let end = trimmed.find(char::is_whitespace)?;

    Some(trimmed[..end].to_string())
}

fn extract_inline_enum_values(definition: &str, column_name: &str) -> Option<Vec<String>> {
    let normalized = definition.to_ascii_lowercase();
    let check_index = normalized.find("check")?;
    let in_index = normalized[check_index..].find(" in ")? + check_index;
    let open_index = definition[in_index..].find('(')? + in_index;
    let close_index = find_matching_parenthesis(definition, open_index)?;
    let values_sql = &definition[open_index + 1..close_index];
    let normalized_column_name = column_name.to_ascii_lowercase();

    if !normalized.contains(&normalized_column_name) {
        return None;
    }

    let values = split_enum_values(values_sql);

    (!values.is_empty()).then_some(values)
}

fn find_matching_parenthesis(input: &str, open_index: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;

    for (offset, ch) in input[open_index..].char_indices() {
        match ch {
            '\'' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth -= 1;

                if depth == 0 {
                    return Some(open_index + offset);
                }
            }
            _ => {}
        }
    }

    None
}

fn split_enum_values(values_sql: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = values_sql.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                if in_string && matches!(chars.peek(), Some('\'')) {
                    current.push('\'');
                    chars.next();
                } else {
                    in_string = !in_string;
                }
            }
            ',' if !in_string => {
                let value = current.trim();

                if !value.is_empty() {
                    values.push(value.to_string());
                }

                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let value = current.trim();

    if !value.is_empty() {
        values.push(value.to_string());
    }

    values
}
