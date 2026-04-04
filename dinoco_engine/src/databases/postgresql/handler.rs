use async_trait::async_trait;

use super::PostgresAdapter;
use crate::{
    DatabaseColumn, DatabaseEnumRaw, DatabaseForeignKey, DatabaseIndex, DatabaseParsedTable,
    DatabaseTable, DinocoAdapter, DinocoAdapterHandler, DinocoResult, DinocoValue,
};

#[async_trait]
impl DinocoAdapterHandler for PostgresAdapter {
    async fn reset_database(&self) -> DinocoResult<()> {
        let tables = self.fetch_tables().await?;
        let enums = self.fetch_enums().await?;

        for table in tables {
            let query = format!("DROP TABLE IF EXISTS \"{}\" CASCADE;", table.name);

            self.execute(&query, &[]).await?;
        }

        for _enum in enums {
            let query = format!("DROP TYPE IF EXISTS \"{}\" CASCADE;", _enum.name);

            self.execute(&query, &[]).await?;
        }

        Ok(())
    }

    async fn fetch_tables(&self) -> DinocoResult<Vec<DatabaseParsedTable>> {
        let query = "
            SELECT table_name AS name 
            FROM information_schema.tables 
            WHERE table_schema = 'public' 
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
                column_name AS name,
                data_type AS db_type,
                -- Postgres retorna 'YES' ou 'NO'. Convertendo direto no SQL para Boolean:
                (is_nullable = 'YES') AS nullable,
                column_default AS default_value,
                NULL AS extra -- Postgres usa default_value (nextval) para auto_increment
            FROM information_schema.columns 
            WHERE table_schema = 'public' 
              AND table_name = $1;
        ";

        self.query_as::<DatabaseColumn>(query, &[DinocoValue::from(table_name)])
            .await
    }

    async fn fetch_foreign_keys(&self) -> DinocoResult<Vec<DatabaseForeignKey>> {
        let query = "
            SELECT
                tc.table_name AS table_name,
                tc.constraint_name AS constraint_name,
                kcu.column_name AS column_name,
                ccu.table_name AS foreign_table_name,
                ccu.column_name AS foreign_column_name
            FROM information_schema.table_constraints AS tc
            JOIN information_schema.key_column_usage AS kcu
              ON tc.constraint_name = kcu.constraint_name 
             AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage AS ccu
              ON ccu.constraint_name = tc.constraint_name 
             AND ccu.table_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY' 
              AND tc.table_schema = 'public';
        ";

        self.query_as::<DatabaseForeignKey>(query, &[]).await
    }

    async fn fetch_enums(&self) -> DinocoResult<Vec<DatabaseEnumRaw>> {
        let query = "
            SELECT 
                t.typname AS name, 
                e.enumlabel AS value
            FROM pg_type t 
            JOIN pg_enum e ON t.oid = e.enumtypid  
            JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname = 'public';
        ";

        self.query_as::<DatabaseEnumRaw>(query, &[]).await
    }

    async fn fetch_indexes(&self) -> DinocoResult<Vec<DatabaseIndex>> {
        let query = "
            SELECT 
                t.relname AS table_name,
                i.relname AS index_name,
                a.attname AS column_name,
                ix.indisunique AS is_unique
            FROM pg_class t
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE t.relkind = 'r' 
              AND n.nspname = 'public'
              AND t.relname != '_dinoco_migrations';
        ";

        self.query_as::<DatabaseIndex>(query, &[]).await
    }
}
