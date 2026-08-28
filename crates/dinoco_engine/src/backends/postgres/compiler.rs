use crate::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CountQuery,
    CreateEnumMigration, CreateIndexMigration, CreateTableMigration, DeleteQuery, DinocoSqlCompiler, DinocoValue,
    DropColumnMigration, DropEnumMigration, DropForeignKeyMigration, DropIndexMigration, DropTableMigration,
    FindOrderBy, FindQuery, FindWhere, InsertQuery, ManyToManyRelationCountQuery, ManyToManyRelationQuery,
    MigrationColumn, MigrationColumnType, MigrationDefault, MigrationForeignKey, MigrationIndexKind, ReferentialAction,
    RelationBatchQuery, RelationCountQuery, RelationJoinQuery, RenameColumnMigration, RenameTableMigration,
    UpdateQuery,
};

use super::{PgBouncerAdapter, PostgresAdapter};

impl DinocoSqlCompiler for PostgresAdapter {
    fn compile_find_query(&self, query: FindQuery) -> (String, Vec<DinocoValue>) {
        compile_find_query(query)
    }

    fn compile_insert_query(&self, query: InsertQuery) -> (String, Vec<DinocoValue>) {
        compile_insert_query(query)
    }

    fn compile_update_query(&self, query: UpdateQuery) -> (String, Vec<DinocoValue>) {
        compile_update_query(query)
    }

    fn compile_delete_query(&self, query: DeleteQuery) -> (String, Vec<DinocoValue>) {
        compile_delete_query(query)
    }

    fn compile_count_query(&self, query: CountQuery) -> (String, Vec<DinocoValue>) {
        compile_count_query(query)
    }

    fn compile_relation_count_query(&self, query: RelationCountQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_count_query(query)
    }

    fn compile_relation_batch_query(&self, query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_batch_query(query)
    }

    fn compile_relation_join_query(&self, query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_join_query(query)
    }

    fn compile_many_to_many_relation_query(&self, query: ManyToManyRelationQuery) -> (String, Vec<DinocoValue>) {
        compile_many_to_many_relation_query(query)
    }

    fn compile_many_to_many_relation_count_query(
        &self,
        query: ManyToManyRelationCountQuery,
    ) -> (String, Vec<DinocoValue>) {
        compile_many_to_many_relation_count_query(query)
    }

    fn compile_create_migrations_table(&self) -> String {
        compile_create_migrations_table()
    }

    fn compile_insert_migration_record(&self, name: &str) -> String {
        compile_insert_migration_record(name)
    }

    fn compile_create_table_migration(&self, migration: CreateTableMigration) -> String {
        compile_create_table_migration(migration)
    }

    fn compile_drop_table_migration(&self, migration: DropTableMigration) -> String {
        compile_drop_table_migration(migration)
    }

    fn compile_rename_table_migration(&self, migration: RenameTableMigration) -> Vec<String> {
        compile_rename_table_migration(migration)
    }

    fn compile_add_column_migration(&self, migration: AddColumnMigration) -> String {
        compile_add_column_migration(migration)
    }

    fn compile_drop_column_migration(&self, migration: DropColumnMigration) -> String {
        compile_drop_column_migration(migration)
    }

    fn compile_alter_column_migration(&self, migration: AlterColumnMigration) -> Vec<String> {
        compile_alter_column_migration(migration)
    }

    fn compile_rename_column_migration(&self, migration: RenameColumnMigration) -> Vec<String> {
        compile_rename_column_migration(migration)
    }

    fn compile_add_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
        compile_add_foreign_key_migration(migration)
    }

    fn compile_add_unvalidated_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
        compile_add_unvalidated_foreign_key_migration(migration)
    }

    fn compile_drop_foreign_key_migration(&self, migration: DropForeignKeyMigration) -> Vec<String> {
        compile_drop_foreign_key_migration(migration)
    }

    fn compile_create_index_migration(&self, migration: CreateIndexMigration) -> String {
        compile_create_index_migration(migration)
    }

    fn compile_drop_index_migration(&self, migration: DropIndexMigration) -> String {
        compile_drop_index_migration(migration)
    }

    fn compile_create_enum_migration(&self, migration: CreateEnumMigration) -> Vec<String> {
        compile_create_enum_migration(migration)
    }

    fn compile_drop_enum_migration(&self, migration: DropEnumMigration) -> Vec<String> {
        compile_drop_enum_migration(migration)
    }

    fn compile_alter_enum_migration(&self, migration: AlterEnumMigration) -> Vec<String> {
        compile_alter_enum_migration(migration)
    }
}

impl DinocoSqlCompiler for PgBouncerAdapter {
    fn compile_find_query(&self, query: FindQuery) -> (String, Vec<DinocoValue>) {
        compile_find_query(query)
    }

    fn compile_insert_query(&self, query: InsertQuery) -> (String, Vec<DinocoValue>) {
        compile_insert_query(query)
    }

    fn compile_update_query(&self, query: UpdateQuery) -> (String, Vec<DinocoValue>) {
        compile_update_query(query)
    }

    fn compile_delete_query(&self, query: DeleteQuery) -> (String, Vec<DinocoValue>) {
        compile_delete_query(query)
    }

    fn compile_count_query(&self, query: CountQuery) -> (String, Vec<DinocoValue>) {
        compile_count_query(query)
    }

    fn compile_relation_count_query(&self, query: RelationCountQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_count_query(query)
    }

    fn compile_relation_batch_query(&self, query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_batch_query(query)
    }

    fn compile_relation_join_query(&self, query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_join_query(query)
    }

    fn compile_many_to_many_relation_query(&self, query: ManyToManyRelationQuery) -> (String, Vec<DinocoValue>) {
        compile_many_to_many_relation_query(query)
    }

    fn compile_many_to_many_relation_count_query(
        &self,
        query: ManyToManyRelationCountQuery,
    ) -> (String, Vec<DinocoValue>) {
        compile_many_to_many_relation_count_query(query)
    }

    fn compile_create_migrations_table(&self) -> String {
        compile_create_migrations_table()
    }

    fn compile_insert_migration_record(&self, name: &str) -> String {
        compile_insert_migration_record(name)
    }

    fn compile_create_table_migration(&self, migration: CreateTableMigration) -> String {
        compile_create_table_migration(migration)
    }

    fn compile_drop_table_migration(&self, migration: DropTableMigration) -> String {
        compile_drop_table_migration(migration)
    }

    fn compile_rename_table_migration(&self, migration: RenameTableMigration) -> Vec<String> {
        compile_rename_table_migration(migration)
    }

    fn compile_add_column_migration(&self, migration: AddColumnMigration) -> String {
        compile_add_column_migration(migration)
    }

    fn compile_drop_column_migration(&self, migration: DropColumnMigration) -> String {
        compile_drop_column_migration(migration)
    }

    fn compile_alter_column_migration(&self, migration: AlterColumnMigration) -> Vec<String> {
        compile_alter_column_migration(migration)
    }

    fn compile_rename_column_migration(&self, migration: RenameColumnMigration) -> Vec<String> {
        compile_rename_column_migration(migration)
    }

    fn compile_add_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
        compile_add_foreign_key_migration(migration)
    }

    fn compile_add_unvalidated_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
        compile_add_unvalidated_foreign_key_migration(migration)
    }

    fn compile_drop_foreign_key_migration(&self, migration: DropForeignKeyMigration) -> Vec<String> {
        compile_drop_foreign_key_migration(migration)
    }

    fn compile_create_index_migration(&self, migration: CreateIndexMigration) -> String {
        compile_create_index_migration(migration)
    }

    fn compile_drop_index_migration(&self, migration: DropIndexMigration) -> String {
        compile_drop_index_migration(migration)
    }

    fn compile_create_enum_migration(&self, migration: CreateEnumMigration) -> Vec<String> {
        compile_create_enum_migration(migration)
    }

    fn compile_drop_enum_migration(&self, migration: DropEnumMigration) -> Vec<String> {
        compile_drop_enum_migration(migration)
    }

    fn compile_alter_enum_migration(&self, migration: AlterEnumMigration) -> Vec<String> {
        compile_alter_enum_migration(migration)
    }
}

fn compile_create_migrations_table() -> String {
    "CREATE TABLE IF NOT EXISTS dinoco_migrations (name VARCHAR(255) PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)"
        .to_string()
}

fn compile_insert_migration_record(name: &str) -> String {
    format!("INSERT INTO dinoco_migrations (name) VALUES ('{}') ON CONFLICT (name) DO NOTHING", escape_sql(name))
}

fn compile_create_table_migration(migration: CreateTableMigration) -> String {
    let if_not_exists = if migration.if_not_exists { " IF NOT EXISTS" } else { "" };
    let primary_key_columns = migration
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| sql_identifier(&column.name))
        .collect::<Vec<_>>();
    let inline_primary_key = primary_key_columns.len() <= 1;
    let mut definitions =
        migration.columns.iter().map(|column| compile_migration_column(column, inline_primary_key)).collect::<Vec<_>>();
    if !inline_primary_key {
        definitions.push(format!("PRIMARY KEY ({})", primary_key_columns.join(", ")));
    }
    definitions.extend(migration.foreign_keys.iter().map(compile_foreign_key));

    format!(
        "CREATE TABLE{if_not_exists} {} (\n    {}\n);",
        sql_identifier(&migration.table),
        definitions.join(",\n    ")
    )
}

fn compile_drop_table_migration(migration: DropTableMigration) -> String {
    let if_exists = if migration.if_exists { " IF EXISTS" } else { "" };
    format!("DROP TABLE{if_exists} {};", sql_identifier(&migration.table))
}

fn compile_rename_table_migration(migration: RenameTableMigration) -> Vec<String> {
    vec![format!("ALTER TABLE {} RENAME TO {};", quoted_identifier(&migration.from), quoted_identifier(&migration.to))]
}

fn compile_add_column_migration(migration: AddColumnMigration) -> String {
    format!(
        "ALTER TABLE {} ADD COLUMN {};",
        sql_identifier(&migration.table),
        compile_migration_column(&migration.column, true)
    )
}

fn compile_drop_column_migration(migration: DropColumnMigration) -> String {
    format!("ALTER TABLE {} DROP COLUMN {};", sql_identifier(&migration.table), sql_identifier(&migration.column))
}

fn compile_alter_column_migration(migration: AlterColumnMigration) -> Vec<String> {
    let mut statements = Vec::new();
    let table = sql_identifier(&migration.table);
    let current = migration.current;
    let desired = migration.desired;

    if current.ty != desired.ty {
        statements.push(format!(
            "ALTER TABLE {table} ALTER COLUMN {} TYPE {};",
            sql_identifier(&desired.name),
            migration_type(&desired.ty)
        ));
    }

    if current.nullable != desired.nullable {
        let action = if desired.nullable { "DROP NOT NULL" } else { "SET NOT NULL" };
        statements.push(format!("ALTER TABLE {table} ALTER COLUMN {} {action};", sql_identifier(&desired.name)));
    }

    if current.default != desired.default {
        match &desired.default {
            Some(default) => {
                if let Some(default) = migration_default(default) {
                    statements.push(format!(
                        "ALTER TABLE {table} ALTER COLUMN {} SET {default};",
                        sql_identifier(&desired.name)
                    ));
                }
            }
            None => statements
                .push(format!("ALTER TABLE {table} ALTER COLUMN {} DROP DEFAULT;", sql_identifier(&desired.name))),
        }
    }

    statements
}

fn compile_rename_column_migration(migration: RenameColumnMigration) -> Vec<String> {
    vec![format!(
        "ALTER TABLE {} RENAME COLUMN {} TO {};",
        sql_identifier(&migration.table),
        quoted_identifier(&migration.from),
        quoted_identifier(&migration.to)
    )]
}

fn compile_add_foreign_key_migration(migration: AddForeignKeyMigration) -> Vec<String> {
    vec![format!(
        "ALTER TABLE {} ADD {};",
        sql_identifier(&migration.table),
        compile_foreign_key(&migration.foreign_key)
    )]
}

fn compile_add_unvalidated_foreign_key_migration(migration: AddForeignKeyMigration) -> Vec<String> {
    vec![format!(
        "ALTER TABLE {} ADD {} NOT VALID;",
        sql_identifier(&migration.table),
        compile_foreign_key(&migration.foreign_key)
    )]
}

fn compile_drop_foreign_key_migration(migration: DropForeignKeyMigration) -> Vec<String> {
    vec![format!(
        "ALTER TABLE {} DROP CONSTRAINT IF EXISTS {};",
        sql_identifier(&migration.table),
        quoted_identifier(&migration.name)
    )]
}

fn compile_create_index_migration(migration: CreateIndexMigration) -> String {
    if migration.index.kind == MigrationIndexKind::FullText {
        let document = migration
            .index
            .columns
            .iter()
            .map(|column| format!("COALESCE({}, '')", sql_identifier(column)))
            .collect::<Vec<_>>()
            .join(" || ' ' || ");
        return format!(
            "CREATE INDEX {} ON {} USING GIN (to_tsvector('simple', {document}));",
            sql_identifier(&migration.index.name),
            sql_identifier(&migration.table),
        );
    }
    let unique = if migration.index.kind == MigrationIndexKind::Unique { "UNIQUE " } else { "" };
    format!(
        "CREATE {unique}INDEX {} ON {} ({});",
        sql_identifier(&migration.index.name),
        sql_identifier(&migration.table),
        migration.index.columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", ")
    )
}

fn compile_drop_index_migration(migration: DropIndexMigration) -> String {
    format!("DROP INDEX {};", sql_identifier(&migration.index.name))
}

fn compile_foreign_key(foreign_key: &MigrationForeignKey) -> String {
    format!(
        "CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) ON UPDATE {} ON DELETE {}",
        quoted_identifier(&foreign_key.name),
        foreign_key.columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", "),
        sql_identifier(&foreign_key.references_table),
        foreign_key.references_columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", "),
        referential_action(foreign_key.on_update),
        referential_action(foreign_key.on_delete),
    )
}

fn referential_action(action: ReferentialAction) -> &'static str {
    match action {
        ReferentialAction::Cascade => "CASCADE",
        ReferentialAction::Restrict => "RESTRICT",
        ReferentialAction::NoAction => "NO ACTION",
        ReferentialAction::SetNull => "SET NULL",
        ReferentialAction::SetDefault => "SET DEFAULT",
    }
}

fn compile_create_enum_migration(migration: CreateEnumMigration) -> Vec<String> {
    let values = migration.values.iter().map(|value| format!("'{}'", escape_sql(value))).collect::<Vec<_>>().join(", ");
    vec![format!("CREATE TYPE {} AS ENUM ({values});", quoted_identifier(&migration.name))]
}

fn compile_drop_enum_migration(migration: DropEnumMigration) -> Vec<String> {
    vec![format!("DROP TYPE IF EXISTS {};", quoted_identifier(&migration.name))]
}

fn compile_alter_enum_migration(migration: AlterEnumMigration) -> Vec<String> {
    migration
        .desired_values
        .iter()
        .filter(|value| !migration.current_values.contains(value))
        .map(|value| {
            format!(
                "ALTER TYPE {} ADD VALUE IF NOT EXISTS '{}';",
                quoted_identifier(&migration.name),
                escape_sql(value)
            )
        })
        .collect()
}

fn compile_migration_column(column: &MigrationColumn, inline_primary_key: bool) -> String {
    let mut parts = vec![sql_identifier(&column.name), migration_type(&column.ty).to_string()];

    if column.primary_key && inline_primary_key {
        parts.push("PRIMARY KEY".to_string());
    }

    if column.unique && !column.primary_key {
        parts.push("UNIQUE".to_string());
    }

    if !column.nullable {
        parts.push("NOT NULL".to_string());
    }

    if let Some(default) = &column.default
        && let Some(default) = migration_default(default)
    {
        parts.push(default);
    }

    parts.join(" ")
}

fn migration_type(ty: &MigrationColumnType) -> String {
    match ty {
        MigrationColumnType::String | MigrationColumnType::Text => "TEXT".to_string(),
        MigrationColumnType::Boolean => "BOOLEAN".to_string(),
        MigrationColumnType::Integer => "BIGINT".to_string(),
        MigrationColumnType::Float => "DOUBLE PRECISION".to_string(),
        MigrationColumnType::DateTime => "TIMESTAMP".to_string(),
        MigrationColumnType::Date => "DATE".to_string(),
        MigrationColumnType::Json => "JSONB".to_string(),
        MigrationColumnType::Enum { name, .. } => quoted_identifier(name),
    }
}

fn migration_default(default: &MigrationDefault) -> Option<String> {
    match default {
        MigrationDefault::String(value) => Some(format!("DEFAULT '{}'", escape_sql(value))),
        MigrationDefault::Boolean(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::Integer(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::Float(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::CurrentTimestamp => Some("DEFAULT CURRENT_TIMESTAMP".to_string()),
        MigrationDefault::AutoIncrement => Some("GENERATED BY DEFAULT AS IDENTITY".to_string()),
    }
}

fn escape_sql(value: &str) -> String {
    value.replace('\'', "''")
}

fn quoted_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn compile_find_query(query: FindQuery) -> (String, Vec<DinocoValue>) {
    let fields = query.fields.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", ");
    let mut sql = format!("SELECT {fields} FROM {}", sql_identifier(query.from));
    let mut placeholders = Placeholder::default();
    let params =
        append_find_tail(&mut sql, query.conditions, query.order_by, query.limit, query.skip, None, &mut placeholders);

    (sql, params)
}

fn compile_insert_query(query: InsertQuery) -> (String, Vec<DinocoValue>) {
    let mut placeholders = Placeholder::default();
    let placeholders_sql = (0..query.rows.len())
        .map(|_| format!("({})", (0..query.fields.len()).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ")))
        .collect::<Vec<_>>()
        .join(", ");
    let params = query.rows.into_iter().flatten().collect::<Vec<_>>();
    let fields = query.fields.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", ");
    let mut sql = format!("INSERT INTO {} ({fields}) VALUES {placeholders_sql}", sql_identifier(query.table));

    if let Some(returning) = query.returning {
        sql.push_str(" RETURNING ");
        sql.push_str(&returning.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", "));
    }

    (sql, params)
}

fn compile_update_query(query: UpdateQuery) -> (String, Vec<DinocoValue>) {
    let mut placeholders = Placeholder::default();
    let sets = query.sets.iter().filter(|set| set.operation.is_scalar()).collect::<Vec<_>>();
    let mut params = sets.iter().map(|set| set.value.clone()).collect::<Vec<_>>();
    let set_sql = sets
        .iter()
        .filter_map(|set| set.operation.assignment_sql(&sql_identifier(set.field), &placeholders.next()))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!("UPDATE {} SET {set_sql}", sql_identifier(query.table));

    params.extend(append_conditions(&mut sql, query.conditions, None, &mut placeholders));

    if let Some(returning) = query.returning {
        sql.push_str(" RETURNING ");
        sql.push_str(&returning.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", "));
    }

    (sql, params)
}

fn compile_delete_query(query: DeleteQuery) -> (String, Vec<DinocoValue>) {
    let mut sql = format!("DELETE FROM {}", sql_identifier(query.table));
    let mut placeholders = Placeholder::default();
    let params = append_conditions(&mut sql, query.conditions, None, &mut placeholders);

    if let Some(returning) = query.returning {
        sql.push_str(" RETURNING ");
        sql.push_str(&returning.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", "));
    }

    (sql, params)
}

fn compile_count_query(query: CountQuery) -> (String, Vec<DinocoValue>) {
    let mut sql = format!("SELECT COUNT(*) FROM {}", sql_identifier(query.table));
    let mut placeholders = Placeholder::default();
    let params = append_conditions(&mut sql, query.conditions, None, &mut placeholders);

    (sql, params)
}

fn compile_relation_count_query(query: RelationCountQuery) -> (String, Vec<DinocoValue>) {
    let mut parent_sql = format!("SELECT {} FROM {}", query.parent_field, query.parent_table);
    let mut placeholders = Placeholder::default();
    let mut params =
        append_conditions(&mut parent_sql, query.parent_conditions, Some(query.parent_table), &mut placeholders);
    let mut sql = format!("SELECT COUNT(*) FROM {} WHERE {} IN ({parent_sql})", query.child_table, query.child_field);
    append_and_conditions(&mut sql, &mut params, query.child_conditions, Some(query.child_table), &mut placeholders);

    (sql, params)
}

fn compile_relation_batch_query(query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
    let fields =
        query.query.fields.iter().map(|field| qualify_field(field, Some(query.query.from))).collect::<Vec<_>>();
    let mut select_fields = fields.join(", ");
    select_fields.push_str(", ");
    select_fields.push_str(&qualify_field(query.relation_key_field, Some(query.query.from)));
    select_fields.push_str(" AS __dinoco_relation_key");

    if query.query.limit >= 0 || query.query.skip >= 0 {
        return compile_partitioned_relation_batch_query(query, select_fields);
    }

    let table = query.query.from;
    let mut sql = format!("SELECT {select_fields} FROM {table}");
    let mut placeholders = Placeholder::default();
    let params = append_find_tail(
        &mut sql,
        query.query.conditions,
        query.query.order_by,
        query.query.limit,
        query.query.skip,
        Some(table),
        &mut placeholders,
    );

    (sql, params)
}

fn compile_relation_join_query(query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
    if query.query.limit >= 0 || query.query.skip >= 0 {
        return compile_partitioned_relation_join_query(query);
    }

    let mut placeholders = Placeholder::default();
    let in_placeholders = (0..query.key_count).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ");
    let mut sql = format!(
        "SELECT {} FROM {} LEFT JOIN {} ON {}.{} = {}.{}",
        relation_join_fields(&query).join(", "),
        query.parent_table,
        query.child_table,
        query.parent_table,
        query.parent_field,
        query.child_table,
        query.child_field,
    );
    let params = append_join_conditions(
        &mut sql,
        query.parent_table,
        query.parent_field,
        query.child_table,
        in_placeholders,
        query.query.conditions,
        &mut placeholders,
    );
    append_order_by(&mut sql, query.query.order_by, Some(query.child_table));

    (sql, params)
}

fn compile_partitioned_relation_batch_query(
    query: RelationBatchQuery,
    select_fields: String,
) -> (String, Vec<DinocoValue>) {
    let table = query.query.from;
    let partition_field = format!("{table}.{}", query.relation_key_field);
    let order_by = relation_partition_order_by(table, query.query.order_by, &partition_field);
    let mut inner_sql = format!(
        "SELECT {select_fields}, ROW_NUMBER() OVER (PARTITION BY {partition_field} ORDER BY {order_by}) AS __dinoco_row_num FROM {table}",
    );
    let mut placeholders = Placeholder::default();
    let mut params = append_conditions(&mut inner_sql, query.query.conditions, Some(table), &mut placeholders);

    append_row_window(&mut params, &mut inner_sql, query.query.skip, query.query.limit, &mut placeholders)
}

fn compile_partitioned_relation_join_query(query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
    let mut placeholders = Placeholder::default();
    let in_placeholders = (0..query.key_count).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ");
    let partition_field = format!("{}.{}", query.parent_table, query.parent_field);
    let order_by = relation_partition_order_by(
        query.child_table,
        query.query.order_by.clone(),
        &format!("{}.{}", query.child_table, query.child_field),
    );
    let mut fields = relation_join_fields(&query);
    fields.push(format!("ROW_NUMBER() OVER (PARTITION BY {partition_field} ORDER BY {order_by}) AS __dinoco_row_num"));

    let mut inner_sql = format!(
        "SELECT {} FROM {} LEFT JOIN {} ON {}.{} = {}.{}",
        fields.join(", "),
        query.parent_table,
        query.child_table,
        query.parent_table,
        query.parent_field,
        query.child_table,
        query.child_field,
    );
    let mut params = append_join_conditions(
        &mut inner_sql,
        query.parent_table,
        query.parent_field,
        query.child_table,
        in_placeholders,
        query.query.conditions,
        &mut placeholders,
    );

    append_row_window(&mut params, &mut inner_sql, query.query.skip, query.query.limit, &mut placeholders)
}

fn compile_many_to_many_relation_query(query: ManyToManyRelationQuery) -> (String, Vec<DinocoValue>) {
    let mut placeholders = Placeholder::default();
    let in_placeholders = (0..query.key_count).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ");
    let partitioned = query.query.limit >= 0 || query.query.skip >= 0;
    let partition_field = format!("{}.{}", query.join_table, query.join_parent_field);
    let mut fields =
        query.query.fields.iter().map(|field| qualify_field(field, Some(query.query.from))).collect::<Vec<_>>();
    fields.push(format!("{partition_field} AS __dinoco_relation_key"));

    if partitioned {
        let order_by = relation_partition_order_by(
            query.query.from,
            query.query.order_by.clone(),
            &format!("{}.{}", query.query.from, query.child_field),
        );
        fields.push(format!(
            "ROW_NUMBER() OVER (PARTITION BY {partition_field} ORDER BY {order_by}) AS __dinoco_row_num"
        ));
    }

    let mut sql = format!(
        "SELECT {} FROM {} INNER JOIN {} ON {}.{} = {}.{} WHERE {partition_field} IN ({in_placeholders})",
        fields.join(", "),
        query.join_table,
        query.query.from,
        query.join_table,
        query.join_child_field,
        query.query.from,
        query.child_field,
    );
    let mut params = Vec::new();
    append_and_conditions(&mut sql, &mut params, query.query.conditions, Some(query.query.from), &mut placeholders);

    if partitioned {
        append_row_window(&mut params, &mut sql, query.query.skip, query.query.limit, &mut placeholders)
    } else {
        append_order_by(&mut sql, query.query.order_by, Some(query.query.from));
        (sql, params)
    }
}

fn compile_many_to_many_relation_count_query(query: ManyToManyRelationCountQuery) -> (String, Vec<DinocoValue>) {
    let mut placeholders = Placeholder::default();
    let mut parent_sql = format!("SELECT {} FROM {}", query.parent_field, query.parent_table);
    let mut params =
        append_conditions(&mut parent_sql, query.parent_conditions, Some(query.parent_table), &mut placeholders);
    let mut sql = format!(
        "SELECT COUNT(*) FROM {} INNER JOIN {} ON {}.{} = {}.{} WHERE {}.{} IN ({parent_sql})",
        query.join_table,
        query.child_table,
        query.join_table,
        query.join_child_field,
        query.child_table,
        query.child_field,
        query.join_table,
        query.join_parent_field,
    );
    append_and_conditions(&mut sql, &mut params, query.child_conditions, Some(query.child_table), &mut placeholders);

    (sql, params)
}

fn append_row_window(
    params: &mut Vec<DinocoValue>,
    inner_sql: &mut str,
    skip: i32,
    limit: i32,
    placeholders: &mut Placeholder,
) -> (String, Vec<DinocoValue>) {
    let mut row_conditions = Vec::new();
    let skip = skip.max(0);

    if skip > 0 {
        row_conditions.push(format!("__dinoco_row_num > {}", placeholders.next()));
        params.push(DinocoValue::Integer(skip as i64));
    }

    if limit >= 0 {
        row_conditions.push(format!("__dinoco_row_num <= {}", placeholders.next()));
        params.push(DinocoValue::Integer((skip + limit) as i64));
    }

    let mut sql = format!("SELECT * FROM ({inner_sql})");

    if !row_conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&row_conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY __dinoco_relation_key, __dinoco_row_num");

    (sql, std::mem::take(params))
}

fn relation_join_fields(query: &RelationJoinQuery) -> Vec<String> {
    let mut fields =
        query.query.fields.iter().map(|field| qualify_field(field, Some(query.child_table))).collect::<Vec<_>>();

    fields.push(qualify_field(query.child_field, Some(query.child_table)));
    fields.push(format!("{} AS __dinoco_relation_key", qualify_field(query.parent_field, Some(query.parent_table))));

    fields
}

fn append_find_tail(
    sql: &mut String,
    conditions: Vec<FindWhere>,
    order_by: Option<FindOrderBy>,
    limit: i32,
    skip: i32,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) -> Vec<DinocoValue> {
    let mut params = append_conditions(sql, conditions, qualifier, placeholders);

    append_order_by(sql, order_by, qualifier);

    if limit >= 0 {
        sql.push_str(" LIMIT ");
        sql.push_str(&placeholders.next());
        params.push(DinocoValue::Integer(limit as i64));
    }

    if skip >= 0 {
        sql.push_str(" OFFSET ");
        sql.push_str(&placeholders.next());
        params.push(DinocoValue::Integer(skip as i64));
    }

    params
}

fn append_join_conditions(
    sql: &mut String,
    parent_table: &str,
    parent_field: &str,
    child_table: &str,
    placeholders_sql: String,
    child_conditions: Vec<FindWhere>,
    placeholders: &mut Placeholder,
) -> Vec<DinocoValue> {
    let mut conditions = vec![format!("{parent_table}.{parent_field} IN ({placeholders_sql})")];
    let mut params = Vec::new();

    collect_conditions(&mut conditions, &mut params, child_conditions, Some(child_table), placeholders);

    sql.push_str(" WHERE ");
    sql.push_str(&conditions.join(" AND "));

    params
}

fn append_conditions(
    sql: &mut String,
    conditions: Vec<FindWhere>,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) -> Vec<DinocoValue> {
    let mut params = Vec::new();
    let mut sql_conditions = Vec::new();

    collect_conditions(&mut sql_conditions, &mut params, conditions, qualifier, placeholders);

    if !sql_conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&sql_conditions.join(" AND "));
    }

    params
}

fn append_and_conditions(
    sql: &mut String,
    params: &mut Vec<DinocoValue>,
    conditions: Vec<FindWhere>,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) {
    let mut sql_conditions = Vec::new();

    collect_conditions(&mut sql_conditions, params, conditions, qualifier, placeholders);

    if !sql_conditions.is_empty() {
        sql.push_str(" AND ");
        sql.push_str(&sql_conditions.join(" AND "));
    }
}

fn collect_conditions(
    sql_conditions: &mut Vec<String>,
    params: &mut Vec<DinocoValue>,
    conditions: Vec<FindWhere>,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) {
    for condition in conditions {
        match condition {
            FindWhere::Eq(field, value) => {
                push_binary(sql_conditions, params, field, "=", value, qualifier, placeholders)
            }
            FindWhere::Neq(field, value) => {
                push_binary(sql_conditions, params, field, "!=", value, qualifier, placeholders)
            }
            FindWhere::Gt(field, value) => {
                push_binary(sql_conditions, params, field, ">", value, qualifier, placeholders)
            }
            FindWhere::Gte(field, value) => {
                push_binary(sql_conditions, params, field, ">=", value, qualifier, placeholders)
            }
            FindWhere::Lt(field, value) => {
                push_binary(sql_conditions, params, field, "<", value, qualifier, placeholders)
            }
            FindWhere::Lte(field, value) => {
                push_binary(sql_conditions, params, field, "<=", value, qualifier, placeholders)
            }
            FindWhere::Like(field, value) => {
                push_binary(sql_conditions, params, field, "LIKE", value, qualifier, placeholders)
            }
            FindWhere::FullText(fields, value) => {
                let placeholder = placeholders.next();
                let document = fields
                    .iter()
                    .map(|field| format!("COALESCE({}, '')", qualify_field(field, qualifier)))
                    .collect::<Vec<_>>()
                    .join(" || ' ' || ");
                sql_conditions
                    .push(format!("to_tsvector('simple', {document}) @@ plainto_tsquery('simple', {placeholder})"));
                params.push(value);
            }
            FindWhere::Between(field, start, end) => {
                let start_placeholder = placeholders.next();
                let end_placeholder = placeholders.next();
                let field = qualify_field(field, qualifier);
                sql_conditions.push(format!("{field} >= {start_placeholder} AND {field} <= {end_placeholder}"));
                params.push(start);
                params.push(end);
            }
            FindWhere::Batch(field, values) => {
                if values.is_empty() {
                    sql_conditions.push("1 = 0".to_string());
                } else {
                    let placeholder_sql = (0..values.len()).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ");
                    sql_conditions.push(format!("{} IN ({placeholder_sql})", qualify_field(field, qualifier)));
                    params.extend(values);
                }
            }
            FindWhere::Null(field) => {
                sql_conditions.push(format!("{} IS NULL", qualify_field(field, qualifier)));
            }
            FindWhere::NotNull(field) => {
                sql_conditions.push(format!("{} IS NOT NULL", qualify_field(field, qualifier)));
            }
            FindWhere::And(conditions) => {
                push_condition_group(sql_conditions, params, conditions, qualifier, placeholders, "AND", "1 = 1");
            }
            FindWhere::Or(conditions) => {
                push_condition_group(sql_conditions, params, conditions, qualifier, placeholders, "OR", "1 = 0");
            }
            FindWhere::Not(condition) => {
                let mut nested = Vec::new();
                collect_conditions(&mut nested, params, vec![*condition], qualifier, placeholders);
                let expression = if nested.is_empty() { "1 = 1".to_string() } else { nested.join(" AND ") };
                sql_conditions.push(format!("NOT ({expression})"));
            }
        }
    }
}

fn push_condition_group(
    sql_conditions: &mut Vec<String>,
    params: &mut Vec<DinocoValue>,
    conditions: Vec<FindWhere>,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
    operator: &str,
    empty_expression: &str,
) {
    let mut nested = Vec::new();
    collect_conditions(&mut nested, params, conditions, qualifier, placeholders);
    let expression =
        if nested.is_empty() { empty_expression.to_string() } else { nested.join(&format!(" {operator} ")) };
    sql_conditions.push(format!("({expression})"));
}

fn push_binary(
    sql_conditions: &mut Vec<String>,
    params: &mut Vec<DinocoValue>,
    field: &'static str,
    op: &str,
    value: DinocoValue,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) {
    sql_conditions.push(format!("{} {op} {}", qualify_field(field, qualifier), placeholders.next()));
    params.push(value);
}

fn append_order_by(sql: &mut String, order_by: Option<FindOrderBy>, qualifier: Option<&str>) {
    if let Some(order_by) = order_by {
        let (field, direction) = match order_by {
            FindOrderBy::Asc(field) => (field, "ASC"),
            FindOrderBy::Desc(field) => (field, "DESC"),
        };

        sql.push_str(" ORDER BY ");
        sql.push_str(&qualify_field(field, qualifier));
        sql.push(' ');
        sql.push_str(direction);
    }
}

fn relation_partition_order_by(table: &str, order_by: Option<FindOrderBy>, fallback: &str) -> String {
    let Some(order_by) = order_by else {
        return fallback.to_string();
    };

    let (field, direction) = match order_by {
        FindOrderBy::Asc(field) => (field, "ASC"),
        FindOrderBy::Desc(field) => (field, "DESC"),
    };

    format!("{} {direction}", qualify_field(field, Some(table)))
}

fn qualify_field(field: &str, qualifier: Option<&str>) -> String {
    match qualifier {
        Some(qualifier) if !field.contains('.') => {
            format!("{}.{}", sql_identifier(qualifier), sql_identifier(field))
        }
        _ if field.contains('.') => field.split('.').map(sql_identifier).collect::<Vec<_>>().join("."),
        _ => sql_identifier(field),
    }
}

fn sql_identifier(identifier: &str) -> String {
    if identifier == "*" {
        return identifier.to_string();
    }
    if is_reserved_identifier(identifier) || identifier_requires_quotes(identifier) {
        quoted_identifier(identifier)
    } else {
        identifier.to_string()
    }
}

fn identifier_requires_quotes(identifier: &str) -> bool {
    let mut chars = identifier.chars();
    !matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase() || ch == '_')
        || chars.any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'))
}

fn is_reserved_identifier(identifier: &str) -> bool {
    matches!(
        identifier.to_ascii_lowercase().as_str(),
        "group"
            | "order"
            | "select"
            | "table"
            | "where"
            | "from"
            | "limit"
            | "offset"
            | "primary"
            | "references"
            | "constraint"
            | "index"
            | "unique"
            | "default"
            | "check"
    )
}

#[derive(Default)]
struct Placeholder {
    index: usize,
}

impl Placeholder {
    fn next(&mut self) -> String {
        self.index += 1;
        format!("${}", self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_uses_a_named_native_enum_type() {
        assert_eq!(
            compile_create_enum_migration(CreateEnumMigration {
                name: "AuthMethod".to_string(),
                values: vec!["PASSWORD".to_string(), "GOOGLE".to_string()],
            }),
            ["CREATE TYPE \"AuthMethod\" AS ENUM ('PASSWORD', 'GOOGLE');"]
        );
        assert_eq!(
            migration_type(&MigrationColumnType::Enum {
                name: "AuthMethod".to_string(),
                values: vec!["PASSWORD".to_string(), "GOOGLE".to_string()],
            }),
            "\"AuthMethod\""
        );
    }
}
