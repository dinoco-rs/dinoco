use async_trait::async_trait;

use super::SqliteAdapter;
use crate::{
    DatabaseColumn, DatabaseEnumRaw, DatabaseForeignKey, DatabaseIndex, DatabaseParsedTable,
    DatabaseTable, DinocoAdapter, DinocoAdapterHandler, DinocoResult, DinocoValue,
};

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

            tables.push(DatabaseParsedTable {
                name: table.name,
                columns,
            })
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
                dflt_value AS default_value,
                NULL AS extra
            FROM pragma_table_info(?);
        ";

        self.query_as::<DatabaseColumn>(query, &[DinocoValue::from(table_name)])
            .await
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
