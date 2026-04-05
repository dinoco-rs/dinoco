use async_trait::async_trait;

use super::PostgresAdapter;
use crate::{
    DatabaseColumn, DatabaseEnumRaw, DatabaseForeignKey, DatabaseIndex, DatabaseParsedTable,
    DatabaseTable, DinocoAdapter, DinocoAdapterHandler, DinocoResult, DinocoValue,
};

#[async_trait]
impl DinocoAdapterHandler for PostgresAdapter {
    async fn reset_database(&self) -> DinocoResult<()> {
        self.execute("DROP SCHEMA IF EXISTS public CASCADE;", &[])
            .await?;
        self.execute("CREATE SCHEMA public;", &[]).await?;

        Ok(())
    }

    async fn fetch_tables(&self) -> DinocoResult<Vec<DatabaseParsedTable>> {
        let query = "
            SELECT
                table_name::text AS name
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_type = 'BASE TABLE'
            ORDER BY table_name;
        ";

        let mut tables = vec![];

        for table in self.query_as::<DatabaseTable>(query, &[]).await? {
            let columns = self.fetch_columns(table.name.clone()).await?;

            tables.push(DatabaseParsedTable {
                name: table.name,
                columns,
            });
        }

        Ok(tables)
    }

    async fn fetch_columns(&self, table_name: String) -> DinocoResult<Vec<DatabaseColumn>> {
        let query = "
            SELECT
                column_name::text AS name,
                data_type::text AS db_type,
                (is_nullable = 'YES') AS nullable,
                column_default::text AS default_value
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = $1
            ORDER BY ordinal_position;
        ";

        self.query_as::<DatabaseColumn>(query, &[DinocoValue::from(table_name)])
            .await
    }

    async fn fetch_foreign_keys(&self) -> DinocoResult<Vec<DatabaseForeignKey>> {
        let query = "
            SELECT
                tc.table_name::text AS table_name,
                tc.constraint_name::text AS constraint_name,
                kcu.column_name::text AS column_name,
                ccu.table_name::text AS foreign_table_name,
                ccu.column_name::text AS foreign_column_name
            FROM information_schema.table_constraints AS tc
            JOIN information_schema.key_column_usage AS kcu
              ON tc.constraint_name = kcu.constraint_name
             AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage AS ccu
              ON ccu.constraint_name = tc.constraint_name
             AND ccu.table_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
              AND tc.table_schema = 'public'
            ORDER BY tc.table_name, tc.constraint_name, kcu.ordinal_position;
        ";

        self.query_as::<DatabaseForeignKey>(query, &[]).await
    }

    async fn fetch_enums(&self) -> DinocoResult<Vec<DatabaseEnumRaw>> {
        let query = "
            SELECT
                t.typname::text AS name,
                e.enumlabel::text AS value
            FROM pg_type t
            JOIN pg_enum e ON t.oid = e.enumtypid
            JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname = 'public'
            ORDER BY t.typname, e.enumsortorder;
        ";

        self.query_as::<DatabaseEnumRaw>(query, &[]).await
    }

    async fn fetch_indexes(&self) -> DinocoResult<Vec<DatabaseIndex>> {
        let query = "
            SELECT
                t.relname::text AS table_name,
                i.relname::text AS index_name,
                a.attname::text AS column_name,
                ix.indisunique AS is_unique
            FROM pg_class t
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE t.relkind = 'r'
              AND n.nspname = 'public'
              AND t.relname != '_dinoco_migrations'
              AND NOT ix.indisprimary
            ORDER BY t.relname, i.relname, a.attnum;
        ";

        self.query_as::<DatabaseIndex>(query, &[]).await
    }
}
