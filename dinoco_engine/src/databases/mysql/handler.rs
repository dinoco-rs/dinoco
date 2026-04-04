use async_trait::async_trait;

use super::MySqlAdapter;
use crate::{
    DatabaseColumn, DatabaseEnumRaw, DatabaseForeignKey, DatabaseIndex, DatabaseParsedTable,
    DatabaseTable, DinocoAdapter, DinocoAdapterHandler, DinocoResult, DinocoValue,
};

#[async_trait]
impl DinocoAdapterHandler for MySqlAdapter {
    async fn reset_database(&self) -> DinocoResult<()> {
        self.execute("SET FOREIGN_KEY_CHECKS = 0;", &[]).await?;

        let tables = self.fetch_tables().await?;

        for table in tables {
            let query = format!("DROP TABLE IF EXISTS `{}`;", table.name);
            self.execute(&query, &[]).await?;
        }

        self.execute("SET FOREIGN_KEY_CHECKS = 1;", &[]).await?;

        Ok(())
    }

    async fn fetch_tables(&self) -> DinocoResult<Vec<DatabaseParsedTable>> {
        let query = "
            SELECT table_name AS name 
            FROM information_schema.tables 
            WHERE table_schema = DATABASE() 
              AND table_type = 'BASE TABLE';
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
                COLUMN_NAME AS name,
                COLUMN_TYPE AS db_type, -- COLUMN_TYPE traz o tipo completo, ex: varchar(255) ou enum('A','B')
                (IS_NULLABLE = 'YES') AS nullable,
                COLUMN_DEFAULT AS default_value,
                EXTRA AS extra -- Aqui vem o 'auto_increment'
            FROM information_schema.columns 
            WHERE table_schema = DATABASE() 
              AND table_name = ?;
        ";

        self.query_as::<DatabaseColumn>(query, &[DinocoValue::from(table_name)])
            .await
    }

    async fn fetch_foreign_keys(&self) -> DinocoResult<Vec<DatabaseForeignKey>> {
        let query = "
            SELECT 
                TABLE_NAME AS table_name,
                CONSTRAINT_NAME AS constraint_name,
                COLUMN_NAME AS column_name,
                REFERENCED_TABLE_NAME AS foreign_table_name,
                REFERENCED_COLUMN_NAME AS foreign_column_name
            FROM information_schema.KEY_COLUMN_USAGE 
            WHERE REFERENCED_TABLE_SCHEMA = DATABASE()
              AND REFERENCED_TABLE_NAME IS NOT NULL;
        ";

        self.query_as::<DatabaseForeignKey>(query, &[]).await
    }

    async fn fetch_enums(&self) -> DinocoResult<Vec<DatabaseEnumRaw>> {
        Ok(vec![])
    }

    async fn fetch_indexes(&self) -> DinocoResult<Vec<DatabaseIndex>> {
        let query = "
            SELECT 
                TABLE_NAME AS table_name,
                INDEX_NAME AS index_name,
                COLUMN_NAME AS column_name,
                (NON_UNIQUE = 0) AS is_unique
            FROM information_schema.STATISTICS
            WHERE TABLE_SCHEMA = DATABASE()
              AND TABLE_NAME != '_dinoco_migrations'
              AND INDEX_NAME != 'PRIMARY';
        ";

        self.query_as::<DatabaseIndex>(query, &[]).await
    }
}
