use crate::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CountQuery,
    CreateEnumMigration, CreateIndexMigration, CreateTableMigration, DeleteQuery, DinocoSqlCompiler, DinocoValue,
    DropColumnMigration, DropEnumMigration, DropForeignKeyMigration, DropIndexMigration, DropTableMigration,
    FindOrderBy, FindQuery, FindWhere, InsertQuery, ManyToManyRelationCountQuery, ManyToManyRelationQuery,
    ManyToManyWriteQuery, MigrationColumn, MigrationColumnType, MigrationDefault, MigrationForeignKey,
    MigrationIndexKind, ReferentialAction, RelationBatchQuery, RelationCountQuery, RelationJoinQuery,
    RenameColumnMigration, RenameTableMigration, SqliteAdapter, UpdateQuery,
};

impl DinocoSqlCompiler for SqliteAdapter {
    fn compile_find_query(&self, query: FindQuery) -> (String, Vec<DinocoValue>) {
        let fields = query.fields.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", ");
        let mut sql = format!("SELECT {fields} FROM {}", sql_identifier(query.from));
        let params = append_find_tail(&mut sql, query.conditions, query.order_by, query.limit, query.skip, None);

        (sql, params)
    }

    fn compile_insert_query(&self, query: InsertQuery) -> (String, Vec<DinocoValue>) {
        let fields_len = query.fields.len();
        let row_placeholders = format!("({})", vec!["?"; fields_len].join(", "));
        let placeholders = vec![row_placeholders; query.rows.len()].join(", ");
        let params = query.rows.into_iter().flatten().collect::<Vec<_>>();
        let fields = query.fields.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", ");
        let mut sql = format!("INSERT INTO {} ({fields}) VALUES {placeholders}", sql_identifier(query.table));

        if let Some(returning) = query.returning {
            sql.push_str(" RETURNING ");
            sql.push_str(&returning.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", "));
        }

        (sql, params)
    }

    fn compile_update_query(&self, query: UpdateQuery) -> (String, Vec<DinocoValue>) {
        let sets = query.sets.iter().filter(|set| set.operation == crate::UpdateOperation::Set).collect::<Vec<_>>();
        let mut params = sets.iter().map(|set| set.value.clone()).collect::<Vec<_>>();
        let set_sql =
            sets.iter().map(|set| format!("{} = ?", sql_identifier(set.field))).collect::<Vec<_>>().join(", ");
        let mut sql = format!("UPDATE {} SET {set_sql}", sql_identifier(query.table));

        params.extend(append_conditions(&mut sql, query.conditions, None));

        if let Some(returning) = query.returning {
            sql.push_str(" RETURNING ");
            sql.push_str(&returning.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", "));
        }

        (sql, params)
    }

    fn compile_delete_query(&self, query: DeleteQuery) -> (String, Vec<DinocoValue>) {
        let mut sql = format!("DELETE FROM {}", sql_identifier(query.table));
        let params = append_conditions(&mut sql, query.conditions, None);

        if let Some(returning) = query.returning {
            sql.push_str(" RETURNING ");
            sql.push_str(&returning.iter().map(|field| sql_identifier(field)).collect::<Vec<_>>().join(", "));
        }

        (sql, params)
    }

    fn compile_count_query(&self, query: CountQuery) -> (String, Vec<DinocoValue>) {
        let mut sql = format!("SELECT COUNT(*) FROM {}", sql_identifier(query.table));
        let params = append_conditions(&mut sql, query.conditions, None);

        (sql, params)
    }

    fn compile_relation_count_query(&self, query: RelationCountQuery) -> (String, Vec<DinocoValue>) {
        let mut parent_sql = format!("SELECT {} FROM {}", query.parent_field, query.parent_table);
        let mut params = append_conditions(&mut parent_sql, query.parent_conditions, Some(query.parent_table));
        let mut sql =
            format!("SELECT COUNT(*) FROM {} WHERE {} IN ({parent_sql})", query.child_table, query.child_field,);
        collect_conditions_prefixed_with_and(&mut sql, &mut params, query.child_conditions, Some(query.child_table));

        (sql, params)
    }

    fn compile_relation_batch_query(&self, query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
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
        let params = append_find_tail(
            &mut sql,
            query.query.conditions,
            query.query.order_by,
            query.query.limit,
            query.query.skip,
            Some(table),
        );

        (sql, params)
    }

    fn compile_relation_join_query(&self, query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
        if query.query.limit >= 0 || query.query.skip >= 0 {
            return compile_partitioned_relation_join_query(query);
        }

        let placeholders = vec!["?"; query.key_count].join(", ");
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
            placeholders,
            query.query.conditions,
        );
        append_order_by(&mut sql, query.query.order_by, Some(query.child_table));

        (sql, params)
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

    fn compile_connect_many_to_many_query(&self, query: ManyToManyWriteQuery) -> (String, Vec<DinocoValue>) {
        let mut sql = format!(
            "INSERT INTO {} ({}, {}) SELECT {}, ? FROM {}",
            sql_identifier(query.join_table),
            sql_identifier(query.join_parent_field),
            sql_identifier(query.join_child_field),
            qualify_field(query.parent_field, Some(query.parent_table)),
            sql_identifier(query.parent_table),
        );
        let mut params = vec![query.child_value];
        params.extend(append_conditions(&mut sql, query.parent_conditions, Some(query.parent_table)));

        (sql, params)
    }

    fn compile_disconnect_many_to_many_query(&self, query: ManyToManyWriteQuery) -> (String, Vec<DinocoValue>) {
        let mut parent_sql = format!(
            "SELECT {} FROM {}",
            qualify_field(query.parent_field, Some(query.parent_table)),
            sql_identifier(query.parent_table),
        );
        let mut params = vec![query.child_value];
        params.extend(append_conditions(&mut parent_sql, query.parent_conditions, Some(query.parent_table)));
        let sql = format!(
            "DELETE FROM {} WHERE {} = ? AND {} IN ({parent_sql})",
            sql_identifier(query.join_table),
            sql_identifier(query.join_child_field),
            sql_identifier(query.join_parent_field),
        );

        (sql, params)
    }

    fn compile_create_migrations_table(&self) -> String {
        "CREATE TABLE IF NOT EXISTS dinoco_migrations (name TEXT PRIMARY KEY, applied_at TEXT DEFAULT CURRENT_TIMESTAMP)"
            .to_string()
    }

    fn compile_insert_migration_record(&self, name: &str) -> String {
        format!("INSERT OR IGNORE INTO dinoco_migrations (name) VALUES ('{}')", escape_sql(name))
    }

    fn compile_create_table_migration(&self, migration: CreateTableMigration) -> String {
        compile_create_table_migration(migration, DatabaseDialect::Sqlite)
    }

    fn compile_drop_table_migration(&self, migration: DropTableMigration) -> String {
        let if_exists = if migration.if_exists { " IF EXISTS" } else { "" };
        format!("DROP TABLE{if_exists} {};", sql_identifier(&migration.table))
    }

    fn compile_rename_table_migration(&self, migration: RenameTableMigration) -> Vec<String> {
        if migration.from != migration.to && migration.from.eq_ignore_ascii_case(&migration.to) {
            let temporary = format!("__dinoco_legacy_rename_{}", migration.to);
            return vec![
                format!(
                    "ALTER TABLE {} RENAME TO {};",
                    quoted_identifier(&migration.from),
                    quoted_identifier(&temporary)
                ),
                format!(
                    "ALTER TABLE {} RENAME TO {};",
                    quoted_identifier(&temporary),
                    quoted_identifier(&migration.to)
                ),
            ];
        }
        vec![format!(
            "ALTER TABLE {} RENAME TO {};",
            quoted_identifier(&migration.from),
            quoted_identifier(&migration.to)
        )]
    }

    fn compile_add_column_migration(&self, migration: AddColumnMigration) -> String {
        format!(
            "ALTER TABLE {} ADD COLUMN {};",
            sql_identifier(&migration.table),
            compile_migration_column(&migration.column, DatabaseDialect::Sqlite, true)
        )
    }

    fn compile_drop_column_migration(&self, migration: DropColumnMigration) -> String {
        format!("ALTER TABLE {} DROP COLUMN {};", sql_identifier(&migration.table), sql_identifier(&migration.column))
    }

    fn compile_alter_column_migration(&self, migration: AlterColumnMigration) -> Vec<String> {
        vec![format!(
            "-- SQLite cannot safely alter column `{}` on `{}` in place. Rebuild the table manually or create a custom migration.",
            migration.desired.name, migration.table
        )]
    }

    fn compile_rename_column_migration(&self, migration: RenameColumnMigration) -> Vec<String> {
        vec![format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {};",
            sql_identifier(&migration.table),
            quoted_identifier(&migration.from),
            quoted_identifier(&migration.to)
        )]
    }

    fn compile_add_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
        vec![format!(
            "-- SQLite cannot add foreign key `{}` on `{}` after table creation. Rebuild the table manually or create a custom migration.",
            migration.foreign_key.name, migration.table
        )]
    }

    fn compile_drop_foreign_key_migration(&self, migration: DropForeignKeyMigration) -> Vec<String> {
        vec![format!(
            "-- SQLite cannot drop foreign key `{}` on `{}` in place. Rebuild the table manually or create a custom migration.",
            migration.name, migration.table
        )]
    }

    fn compile_create_index_migration(&self, migration: CreateIndexMigration) -> String {
        if migration.index.kind == MigrationIndexKind::FullText {
            return format!(
                "-- SQLite does not support native full-text indexes for `{}.{}`; Dinoco uses LIKE fallback queries.",
                migration.table,
                migration.index.columns.join(", ")
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

    fn compile_drop_index_migration(&self, migration: DropIndexMigration) -> String {
        format!("DROP INDEX {};", sql_identifier(&migration.index.name))
    }

    fn compile_create_enum_migration(&self, _migration: CreateEnumMigration) -> Vec<String> {
        Vec::new()
    }

    fn compile_drop_enum_migration(&self, _migration: DropEnumMigration) -> Vec<String> {
        Vec::new()
    }

    fn compile_alter_enum_migration(&self, _migration: AlterEnumMigration) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Clone, Copy)]
enum DatabaseDialect {
    Sqlite,
}

fn compile_create_table_migration(migration: CreateTableMigration, dialect: DatabaseDialect) -> String {
    let if_not_exists = if migration.if_not_exists { " IF NOT EXISTS" } else { "" };
    let primary_key_columns = migration
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| sql_identifier(&column.name))
        .collect::<Vec<_>>();
    let inline_primary_key = primary_key_columns.len() <= 1;
    let mut definitions = migration
        .columns
        .iter()
        .map(|column| compile_migration_column(column, dialect, inline_primary_key))
        .collect::<Vec<_>>();
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

fn compile_foreign_key(foreign_key: &MigrationForeignKey) -> String {
    format!(
        "CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) ON UPDATE {} ON DELETE {}",
        sql_identifier(&foreign_key.name),
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

fn compile_migration_column(column: &MigrationColumn, dialect: DatabaseDialect, inline_primary_key: bool) -> String {
    let mut parts = vec![sql_identifier(&column.name), migration_type(&column.ty, dialect).to_string()];

    if let MigrationColumnType::Enum { values, .. } = &column.ty {
        let values = values.iter().map(|value| format!("'{}'", escape_sql(value))).collect::<Vec<_>>().join(", ");
        parts.push(format!("CHECK ({} IN ({values}))", sql_identifier(&column.name)));
    }

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
        && let Some(default) = migration_default(default, dialect)
    {
        parts.push(default);
    }

    parts.join(" ")
}

fn migration_type(ty: &MigrationColumnType, _dialect: DatabaseDialect) -> &'static str {
    match ty {
        MigrationColumnType::String | MigrationColumnType::Text => "TEXT",
        MigrationColumnType::Boolean => "BOOLEAN",
        MigrationColumnType::Integer => "INTEGER",
        MigrationColumnType::Float => "REAL",
        MigrationColumnType::DateTime => "DATETIME",
        MigrationColumnType::Date => "DATE",
        MigrationColumnType::Json => "BLOB",
        MigrationColumnType::Enum { .. } => "TEXT",
    }
}

fn migration_default(default: &MigrationDefault, _dialect: DatabaseDialect) -> Option<String> {
    match default {
        MigrationDefault::String(value) => Some(format!("DEFAULT '{}'", escape_sql(value))),
        MigrationDefault::Boolean(value) => Some(format!("DEFAULT {}", if *value { 1 } else { 0 })),
        MigrationDefault::Integer(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::Float(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::CurrentTimestamp => Some("DEFAULT CURRENT_TIMESTAMP".to_string()),
        MigrationDefault::AutoIncrement => None,
    }
}

fn escape_sql(value: &str) -> String {
    value.replace('\'', "''")
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
    let mut params = append_conditions(&mut inner_sql, query.query.conditions, Some(table));

    append_row_window(&mut params, &mut inner_sql, query.query.skip, query.query.limit)
}

fn compile_partitioned_relation_join_query(query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
    let placeholders = vec!["?"; query.key_count].join(", ");
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
        placeholders,
        query.query.conditions,
    );

    append_row_window(&mut params, &mut inner_sql, query.query.skip, query.query.limit)
}

fn compile_many_to_many_relation_query(query: ManyToManyRelationQuery) -> (String, Vec<DinocoValue>) {
    let placeholders = vec!["?"; query.key_count].join(", ");
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
        "SELECT {} FROM {} INNER JOIN {} ON {}.{} = {}.{} WHERE {partition_field} IN ({placeholders})",
        fields.join(", "),
        query.join_table,
        query.query.from,
        query.join_table,
        query.join_child_field,
        query.query.from,
        query.child_field,
    );
    let mut params = Vec::new();
    collect_conditions_prefixed_with_and(&mut sql, &mut params, query.query.conditions, Some(query.query.from));

    if partitioned {
        append_row_window(&mut params, &mut sql, query.query.skip, query.query.limit)
    } else {
        append_order_by(&mut sql, query.query.order_by, Some(query.query.from));
        (sql, params)
    }
}

fn compile_many_to_many_relation_count_query(query: ManyToManyRelationCountQuery) -> (String, Vec<DinocoValue>) {
    let mut parent_sql = format!("SELECT {} FROM {}", query.parent_field, query.parent_table);
    let mut params = append_conditions(&mut parent_sql, query.parent_conditions, Some(query.parent_table));
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
    collect_conditions_prefixed_with_and(&mut sql, &mut params, query.child_conditions, Some(query.child_table));

    (sql, params)
}

fn append_row_window(
    params: &mut Vec<DinocoValue>,
    inner_sql: &mut str,
    skip: i32,
    limit: i32,
) -> (String, Vec<DinocoValue>) {
    let mut row_conditions = Vec::new();
    let skip = skip.max(0);

    if skip > 0 {
        row_conditions.push("__dinoco_row_num > ?".to_string());
        params.push(DinocoValue::Integer(skip as i64));
    }

    if limit >= 0 {
        row_conditions.push("__dinoco_row_num <= ?".to_string());
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
) -> Vec<DinocoValue> {
    let mut params = append_conditions(sql, conditions, qualifier);

    append_order_by(sql, order_by, qualifier);

    if limit >= 0 {
        sql.push_str(" LIMIT ?");
        params.push(DinocoValue::Integer(limit as i64));
    }

    if skip >= 0 {
        sql.push_str(" OFFSET ?");
        params.push(DinocoValue::Integer(skip as i64));
    }

    params
}

fn append_join_conditions(
    sql: &mut String,
    parent_table: &str,
    parent_field: &str,
    child_table: &str,
    placeholders: String,
    child_conditions: Vec<FindWhere>,
) -> Vec<DinocoValue> {
    let mut conditions = vec![format!("{parent_table}.{parent_field} IN ({placeholders})")];
    let mut params = Vec::new();

    collect_conditions(&mut conditions, &mut params, child_conditions, Some(child_table));

    sql.push_str(" WHERE ");
    sql.push_str(&conditions.join(" AND "));

    params
}

fn append_conditions(sql: &mut String, conditions: Vec<FindWhere>, qualifier: Option<&str>) -> Vec<DinocoValue> {
    let mut params = Vec::new();
    let mut sql_conditions = Vec::new();

    collect_conditions(&mut sql_conditions, &mut params, conditions, qualifier);

    if !sql_conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&sql_conditions.join(" AND "));
    }

    params
}

fn collect_conditions_prefixed_with_and(
    sql: &mut String,
    params: &mut Vec<DinocoValue>,
    conditions: Vec<FindWhere>,
    qualifier: Option<&str>,
) {
    let mut sql_conditions = Vec::new();

    collect_conditions(&mut sql_conditions, params, conditions, qualifier);

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
) {
    for condition in conditions {
        match condition {
            FindWhere::Eq(field, value) => {
                sql_conditions.push(format!("{} = ?", qualify_field(field, qualifier)));
                params.push(value);
            }
            FindWhere::Neq(field, value) => {
                sql_conditions.push(format!("{} != ?", qualify_field(field, qualifier)));
                params.push(value);
            }
            FindWhere::Gt(field, value) => {
                sql_conditions.push(format!("{} > ?", qualify_field(field, qualifier)));
                params.push(value);
            }
            FindWhere::Gte(field, value) => {
                sql_conditions.push(format!("{} >= ?", qualify_field(field, qualifier)));
                params.push(value);
            }
            FindWhere::Lt(field, value) => {
                sql_conditions.push(format!("{} < ?", qualify_field(field, qualifier)));
                params.push(value);
            }
            FindWhere::Lte(field, value) => {
                sql_conditions.push(format!("{} <= ?", qualify_field(field, qualifier)));
                params.push(value);
            }
            FindWhere::Like(field, value) => {
                sql_conditions.push(format!("{} LIKE ?", qualify_field(field, qualifier)));
                params.push(value);
            }
            FindWhere::FullText(fields, value) => {
                let value = match value {
                    DinocoValue::String(value) => DinocoValue::String(format!("%{value}%")),
                    value => value,
                };
                if fields.is_empty() {
                    sql_conditions.push("1 = 0".to_string());
                } else if let [field] = fields {
                    sql_conditions.push(format!("{} LIKE ?", qualify_field(field, qualifier)));
                    params.push(value);
                } else {
                    sql_conditions.push(format!(
                        "({})",
                        fields
                            .iter()
                            .map(|field| format!("{} LIKE ?", qualify_field(field, qualifier)))
                            .collect::<Vec<_>>()
                            .join(" OR ")
                    ));
                    params.extend((0..fields.len()).map(|_| value.clone()));
                }
            }
            FindWhere::Between(field, start, end) => {
                sql_conditions.push(format!(
                    "{} >= ? AND {} <= ?",
                    qualify_field(field, qualifier),
                    qualify_field(field, qualifier)
                ));
                params.push(start);
                params.push(end);
            }
            FindWhere::Batch(field, values) => {
                if values.is_empty() {
                    sql_conditions.push("1 = 0".to_string());
                } else {
                    let placeholders = vec!["?"; values.len()].join(", ");
                    sql_conditions.push(format!("{} IN ({placeholders})", qualify_field(field, qualifier)));
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
                push_condition_group(sql_conditions, params, conditions, qualifier, "AND", "1 = 1");
            }
            FindWhere::Or(conditions) => {
                push_condition_group(sql_conditions, params, conditions, qualifier, "OR", "1 = 0");
            }
            FindWhere::Not(condition) => {
                let mut nested = Vec::new();
                collect_conditions(&mut nested, params, vec![*condition], qualifier);
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
    operator: &str,
    empty_expression: &str,
) {
    let mut nested = Vec::new();
    collect_conditions(&mut nested, params, conditions, qualifier);
    let expression =
        if nested.is_empty() { empty_expression.to_string() } else { nested.join(&format!(" {operator} ")) };
    sql_conditions.push(format!("({expression})"));
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

fn quoted_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_uses_text_with_a_check_constraint_for_enums() {
        let sql = compile_migration_column(
            &MigrationColumn {
                name: "auth_method".to_string(),
                ty: MigrationColumnType::Enum {
                    name: "AuthMethod".to_string(),
                    values: vec!["PASSWORD".to_string(), "GOOGLE".to_string()],
                },
                primary_key: false,
                unique: false,
                nullable: false,
                default: None,
            },
            DatabaseDialect::Sqlite,
            true,
        );

        assert_eq!(sql, "auth_method TEXT CHECK (auth_method IN ('PASSWORD', 'GOOGLE')) NOT NULL");
    }
}
