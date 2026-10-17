//! Prints the SQL and parameters every `DinocoSqlCompiler` method produces
//! for a broad corpus of queries and migrations, in every dialect. Diffing the
//! output of two revisions proves a compiler change is byte-for-byte neutral.

use dinoco_engine::*;

fn value(text: &str) -> DinocoValue {
    DinocoValue::String(text.to_string())
}

fn conditions() -> Vec<(&'static str, Vec<FindWhere>)> {
    let relation = |quantifier, conditions: Vec<FindWhere>, join: Option<RelationJoinTable>| {
        FindWhere::Relation(RelationMatch {
            parent_table: "account",
            parent_field: "id",
            child_table: "post",
            child_field: "account_id",
            join,
            quantifier,
            conditions,
        })
    };
    let join = RelationJoinTable { table: "_account_to_post", parent_field: "account_id", child_field: "post_id" };
    let nested = FindWhere::Relation(RelationMatch {
        parent_table: "post",
        parent_field: "id",
        child_table: "comment",
        child_field: "post_id",
        join: None,
        quantifier: RelationQuantifier::Some,
        conditions: vec![FindWhere::Eq("body", value("nice"))],
    });

    vec![
        ("none", vec![]),
        ("eq", vec![FindWhere::Eq("email", value("a@dinoco.rs"))]),
        (
            "comparisons",
            vec![
                FindWhere::Neq("status", value("deleted")),
                FindWhere::Gt("age", DinocoValue::Integer(18)),
                FindWhere::Gte("score", DinocoValue::Float(1.5)),
                FindWhere::Lt("created_at", DinocoValue::Integer(10)),
                FindWhere::Lte("rank", DinocoValue::Integer(3)),
                FindWhere::Like("name", value("%dino%")),
            ],
        ),
        ("between", vec![FindWhere::Between("age", DinocoValue::Integer(1), DinocoValue::Integer(9))]),
        (
            "batch",
            vec![FindWhere::Batch("id", vec![value("a"), value("b"), value("c")]), FindWhere::Batch("other", vec![])],
        ),
        ("nulls", vec![FindWhere::Null("deleted_at"), FindWhere::NotNull("verified_at")]),
        ("fulltext_one", vec![FindWhere::FullText(&["title"], value("rust orm"))]),
        ("fulltext_many", vec![FindWhere::FullText(&["title", "body"], value("rust orm"))]),
        ("fulltext_none", vec![FindWhere::FullText(&[], value("rust orm"))]),
        ("fulltext_integer", vec![FindWhere::FullText(&["title", "body"], DinocoValue::Integer(3))]),
        (
            "groups",
            vec![
                FindWhere::And(vec![
                    FindWhere::Eq("a", DinocoValue::Integer(1)),
                    FindWhere::Eq("b", DinocoValue::Integer(2)),
                ]),
                FindWhere::Or(vec![
                    FindWhere::Eq("c", DinocoValue::Boolean(true)),
                    FindWhere::Not(Box::new(FindWhere::Null("d"))),
                    FindWhere::And(vec![
                        FindWhere::Gt("e", DinocoValue::Integer(5)),
                        FindWhere::Lt("e", DinocoValue::Integer(9)),
                    ]),
                ]),
                FindWhere::And(vec![]),
                FindWhere::Or(vec![]),
                FindWhere::Not(Box::new(FindWhere::And(vec![]))),
            ],
        ),
        (
            "identifiers",
            vec![
                FindWhere::Eq("group", value("x")),
                FindWhere::Eq("createdAt", value("y")),
                FindWhere::Eq("other.column", value("z")),
                FindWhere::Eq("Weird\"Name`", value("w")),
                FindWhere::Eq("1digit", value("v")),
            ],
        ),
        (
            "many_to_many_match",
            vec![
                FindWhere::ManyToMany(ManyToManyMatch {
                    local_key: "id",
                    join_table: "_account_to_post",
                    join_local_field: "account_id",
                    join_target_field: "post_id",
                    negated: false,
                    predicate: Box::new(FindWhere::Eq("post_id", value("p1"))),
                }),
                FindWhere::ManyToMany(ManyToManyMatch {
                    local_key: "id",
                    join_table: "_account_to_post",
                    join_local_field: "account_id",
                    join_target_field: "post_id",
                    negated: true,
                    predicate: Box::new(FindWhere::Batch("post_id", vec![value("p1"), value("p2")])),
                }),
                FindWhere::ManyToMany(ManyToManyMatch {
                    local_key: "id",
                    join_table: "_account_to_post",
                    join_local_field: "account_id",
                    join_target_field: "post_id",
                    negated: false,
                    predicate: Box::new(FindWhere::And(vec![])),
                }),
            ],
        ),
        (
            "relations",
            vec![
                relation(RelationQuantifier::Some, vec![FindWhere::Eq("title", value("hello"))], None),
                relation(RelationQuantifier::None, vec![], None),
                relation(RelationQuantifier::Every, vec![], None),
                relation(
                    RelationQuantifier::Every,
                    vec![FindWhere::Gt("likes", DinocoValue::Integer(3)), nested],
                    None,
                ),
                relation(RelationQuantifier::Some, vec![FindWhere::Eq("title", value("m2m"))], Some(join)),
                relation(RelationQuantifier::None, vec![FindWhere::Null("title")], Some(join)),
            ],
        ),
        (
            "self_relation_write",
            vec![FindWhere::Relation(RelationMatch {
                parent_table: "account",
                parent_field: "manager_id",
                child_table: "account",
                child_field: "id",
                join: None,
                quantifier: RelationQuantifier::Some,
                conditions: vec![FindWhere::Eq("name", value("boss"))],
            })],
        ),
        (
            "nested_relation_in_groups",
            vec![FindWhere::Or(vec![
                FindWhere::Not(Box::new(relation(
                    RelationQuantifier::Some,
                    vec![FindWhere::Eq("x", DinocoValue::Null)],
                    None,
                ))),
                FindWhere::Relation(RelationMatch {
                    parent_table: "account",
                    parent_field: "id",
                    child_table: "account",
                    child_field: "referrer_id",
                    join: None,
                    quantifier: RelationQuantifier::None,
                    conditions: vec![],
                }),
            ])],
        ),
        (
            "values",
            vec![
                FindWhere::Eq("bytes", DinocoValue::Bytes(vec![1, 2, 3])),
                FindWhere::Eq("json", DinocoValue::Json(dinoco_engine::serde_json::json!({"a": 1}))),
                FindWhere::Eq("enum", DinocoValue::Enum("Status".to_string(), "active".to_string())),
                FindWhere::Eq("date", DinocoValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 5, 6).unwrap())),
                FindWhere::Eq(
                    "datetime",
                    DinocoValue::DateTime(chrono::DateTime::from_timestamp(1_700_000_000, 5_000).unwrap()),
                ),
                FindWhere::Eq("nothing", DinocoValue::Null),
            ],
        ),
    ]
}

fn pages() -> Vec<(i32, i32, Option<FindOrderBy>)> {
    vec![
        (-1, -1, None),
        (1, -1, Some(FindOrderBy::Desc("id"))),
        (10, 20, Some(FindOrderBy::Asc("createdAt"))),
        (0, 0, Some(FindOrderBy::Asc("group"))),
        (-1, 5, None),
        (5, -1, Some(FindOrderBy::Desc("other.column"))),
    ]
}

fn find(
    fields: &'static [&'static str],
    from: &'static str,
    conditions: Vec<FindWhere>,
    page: (i32, i32, Option<FindOrderBy>),
) -> FindQuery {
    FindQuery { fields, from, conditions, limit: page.0, skip: page.1, order_by: page.2 }
}

fn print(dialect: &str, case: &str, (sql, params): (String, Vec<DinocoValue>)) {
    println!("## {dialect} {case}\n{sql}\n{params:?}\n");
}

fn print_statements(dialect: &str, case: &str, statements: Vec<String>) {
    println!("## {dialect} {case}");
    for statement in statements {
        println!("{statement}");
    }
    println!();
}

fn column(name: &str, ty: MigrationColumnType) -> MigrationColumn {
    MigrationColumn { name: name.to_string(), ty, primary_key: false, unique: false, nullable: false, default: None }
}

fn migration_columns() -> Vec<MigrationColumn> {
    vec![
        MigrationColumn {
            primary_key: true,
            default: Some(MigrationDefault::AutoIncrement),
            ..column("id", MigrationColumnType::Integer)
        },
        MigrationColumn { unique: true, ..column("email", MigrationColumnType::String) },
        MigrationColumn { nullable: true, ..column("bio", MigrationColumnType::Text) },
        MigrationColumn {
            default: Some(MigrationDefault::Boolean(true)),
            ..column("active", MigrationColumnType::Boolean)
        },
        MigrationColumn { default: Some(MigrationDefault::Float(1.5)), ..column("score", MigrationColumnType::Float) },
        MigrationColumn {
            default: Some(MigrationDefault::CurrentTimestamp),
            ..column("createdAt", MigrationColumnType::DateTime)
        },
        column("birthday", MigrationColumnType::Date),
        MigrationColumn { nullable: true, ..column("meta", MigrationColumnType::Json) },
        MigrationColumn {
            default: Some(MigrationDefault::String("it's".to_string())),
            ..column(
                "status",
                MigrationColumnType::Enum {
                    name: "Status".to_string(),
                    values: vec!["active".to_string(), "it's".to_string()],
                },
            )
        },
        MigrationColumn {
            default: Some(MigrationDefault::Integer(-4)),
            ..column("group", MigrationColumnType::Integer)
        },
    ]
}

fn foreign_key() -> MigrationForeignKey {
    MigrationForeignKey {
        name: "post_author_fk".to_string(),
        columns: vec!["author_id".to_string(), "Tenant".to_string()],
        references_table: "User".to_string(),
        references_columns: vec!["id".to_string(), "tenant".to_string()],
        on_update: ReferentialAction::Cascade,
        on_delete: ReferentialAction::SetNull,
    }
}

/// Integer and text parent keys, the types a relation key usually has.
fn parent_keys(key_count: usize) -> [(&'static str, Vec<DinocoValue>); 2] {
    [
        ("integer", (1..=key_count as i64).map(DinocoValue::Integer).collect()),
        ("text", (0..key_count).map(|key| DinocoValue::String(format!("key-{key}"))).collect()),
    ]
}

/// A relation occurrence query as it runs: the parent keys bound in front.
fn occurrence_with_keys<C: DinocoSqlCompiler>(
    compiler: &C,
    query: RelationOccurrenceQuery,
    keys: Vec<DinocoValue>,
) -> (String, Vec<DinocoValue>) {
    compiler.compile_relation_occurrence_query_with_keys(query, keys)
}

fn corpus<C: DinocoSqlCompiler>(dialect: &str, compiler: &C) {
    for (case, conditions) in conditions() {
        for (index, page) in pages().into_iter().enumerate() {
            print(
                dialect,
                &format!("find {case} page{index}"),
                compiler.compile_find_query(find(
                    &["id", "email", "group", "*", "createdAt"],
                    "account",
                    conditions.clone(),
                    page.clone(),
                )),
            );
            print(
                dialect,
                &format!("relation_batch {case} page{index}"),
                compiler.compile_relation_batch_query(RelationBatchQuery {
                    query: find(&["id", "title"], "post", conditions.clone(), page.clone()),
                    relation_key_field: "account_id",
                }),
            );
            for (parent_table, child_table) in [("account", "post"), ("topic", "topic")] {
                print(
                    dialect,
                    &format!("relation_join {case} page{index} {parent_table}->{child_table}"),
                    compiler.compile_relation_join_query(RelationJoinQuery {
                        query: find(&["id", "title"], child_table, conditions.clone(), page.clone()),
                        parent_table,
                        child_table,
                        parent_field: "post_id",
                        child_field: "id",
                        key_count: 3,
                    }),
                );
            }
            for key_count in [1, 3] {
                print(
                    dialect,
                    &format!("relation_occurrence {case} page{index} keys{key_count}"),
                    compiler.compile_relation_occurrence_query(RelationOccurrenceQuery {
                        query: find(&["id", "title", "createdAt"], "post", conditions.clone(), page.clone()),
                        child_field: "account_id",
                        key_count,
                    }),
                );
                for (kind, keys) in parent_keys(key_count) {
                    print(
                        dialect,
                        &format!("relation_occurrence {case} page{index} keys{key_count} bound {kind}"),
                        occurrence_with_keys(
                            compiler,
                            RelationOccurrenceQuery {
                                query: find(&["id", "title"], "post", conditions.clone(), page.clone()),
                                child_field: "account_id",
                                key_count,
                            },
                            keys,
                        ),
                    );
                }
                print(
                    dialect,
                    &format!("many_to_many {case} page{index} keys{key_count}"),
                    compiler.compile_many_to_many_relation_query(ManyToManyRelationQuery {
                        query: find(&["id", "title"], "post", conditions.clone(), page.clone()),
                        join_table: "_account_to_post",
                        parent_field: "id",
                        child_field: "id",
                        join_parent_field: "account_id",
                        join_child_field: "post_id",
                        key_count,
                    }),
                );
            }
        }

        print(
            dialect,
            &format!("count {case}"),
            compiler.compile_count_query(CountQuery { table: "account", conditions: conditions.clone() }),
        );
        print(
            dialect,
            &format!("exists {case}"),
            compiler.compile_exists_query(ExistsQuery { table: "Account", conditions: conditions.clone() }),
        );
        for returning in [None, Some(&["id", "email", "group"][..])] {
            print(
                dialect,
                &format!("delete {case} returning={}", returning.is_some()),
                compiler.compile_delete_query(DeleteQuery {
                    table: "account",
                    conditions: conditions.clone(),
                    returning,
                }),
            );
            print(
                dialect,
                &format!("update {case} returning={}", returning.is_some()),
                compiler.compile_update_query(UpdateQuery {
                    table: "account",
                    sets: update_sets(),
                    conditions: conditions.clone(),
                    returning,
                }),
            );
        }
        print(
            dialect,
            &format!("relation_count {case}"),
            compiler.compile_relation_count_query(RelationCountQuery {
                parent_table: "account",
                child_table: "post",
                parent_field: "id",
                child_field: "account_id",
                parent_conditions: conditions.clone(),
                child_conditions: conditions.clone(),
            }),
        );
        print(
            dialect,
            &format!("many_to_many_count {case}"),
            compiler.compile_many_to_many_relation_count_query(ManyToManyRelationCountQuery {
                parent_table: "account",
                child_table: "post",
                join_table: "_account_to_post",
                parent_field: "id",
                child_field: "id",
                join_parent_field: "account_id",
                join_child_field: "post_id",
                parent_conditions: conditions.clone(),
                child_conditions: conditions,
            }),
        );
    }

    for (rows, returning) in [(1, None), (3, Some(&["id", "group"][..])), (0, None)] {
        print(
            dialect,
            &format!("insert rows{rows} returning={}", returning.is_some()),
            compiler.compile_insert_query(InsertQuery {
                table: "Account",
                fields: vec!["id", "email", "group", "createdAt"],
                rows: (0..rows)
                    .map(|row| {
                        vec![
                            DinocoValue::Integer(row),
                            value("a@dinoco.rs"),
                            DinocoValue::Null,
                            DinocoValue::Boolean(row % 2 == 0),
                        ]
                    })
                    .collect(),
                returning,
            }),
        );
    }

    print(
        dialect,
        "update only relation sets",
        compiler.compile_update_query(UpdateQuery {
            table: "account",
            sets: vec![UpdateSet { field: "team_id", value: value("t"), operation: UpdateOperation::Connect }],
            conditions: vec![FindWhere::Eq("id", value("a"))],
            returning: None,
        }),
    );

    print_statements(dialect, "create migrations table", vec![compiler.compile_create_migrations_table()]);
    print_statements(
        dialect,
        "insert migration record",
        vec![compiler.compile_insert_migration_record("2024_it's_init")],
    );
    for (if_not_exists, composite) in [(false, false), (true, true)] {
        let mut columns = migration_columns();
        if composite {
            columns[1].primary_key = true;
        }
        print_statements(
            dialect,
            &format!("create table if_not_exists={if_not_exists} composite={composite}"),
            vec![compiler.compile_create_table_migration(CreateTableMigration {
                table: "User".to_string(),
                columns,
                foreign_keys: vec![foreign_key()],
                if_not_exists,
            })],
        );
    }
    for if_exists in [false, true] {
        print_statements(
            dialect,
            &format!("drop table if_exists={if_exists}"),
            vec![compiler.compile_drop_table_migration(DropTableMigration { table: "order".to_string(), if_exists })],
        );
    }
    for (from, to) in [("user", "account"), ("user", "User")] {
        print_statements(
            dialect,
            &format!("rename table {from}->{to}"),
            compiler
                .compile_rename_table_migration(RenameTableMigration { from: from.to_string(), to: to.to_string() }),
        );
    }
    for column in migration_columns() {
        print_statements(
            dialect,
            &format!("add column {}", column.name),
            vec![compiler.compile_add_column_migration(AddColumnMigration { table: "user".to_string(), column })],
        );
    }
    print_statements(
        dialect,
        "drop column",
        vec![compiler.compile_drop_column_migration(DropColumnMigration {
            table: "user".to_string(),
            column: "Bio".to_string(),
        })],
    );
    let columns = migration_columns();
    for (current, desired) in [(0usize, 1usize), (2, 2), (3, 4), (8, 9), (5, 2)] {
        let mut desired = columns[desired].clone();
        desired.name = columns[current].name.clone();
        let mut desired_nullable = desired.clone();
        desired_nullable.nullable = !desired.nullable;
        desired_nullable.default = None;
        for desired in [desired, desired_nullable] {
            print_statements(
                dialect,
                &format!("alter column {current}->{}", desired.name),
                compiler.compile_alter_column_migration(AlterColumnMigration {
                    table: "user".to_string(),
                    current: columns[current].clone(),
                    desired,
                }),
            );
        }
    }
    print_statements(
        dialect,
        "rename column",
        compiler.compile_rename_column_migration(RenameColumnMigration {
            table: "group".to_string(),
            from: "oldName".to_string(),
            to: "new_name".to_string(),
        }),
    );
    print_statements(
        dialect,
        "add foreign key",
        compiler.compile_add_foreign_key_migration(AddForeignKeyMigration {
            table: "post".to_string(),
            foreign_key: foreign_key(),
        }),
    );
    print_statements(
        dialect,
        "add unvalidated foreign key",
        compiler.compile_add_unvalidated_foreign_key_migration(AddForeignKeyMigration {
            table: "post".to_string(),
            foreign_key: foreign_key(),
        }),
    );
    print_statements(
        dialect,
        "drop foreign key",
        compiler.compile_drop_foreign_key_migration(DropForeignKeyMigration {
            table: "post".to_string(),
            name: "post_author_fk".to_string(),
        }),
    );
    for kind in [MigrationIndexKind::Standard, MigrationIndexKind::Unique, MigrationIndexKind::FullText] {
        let index = MigrationIndex {
            name: "post_title_idx".to_string(),
            columns: vec!["title".to_string(), "Body".to_string()],
            automatic: false,
            kind,
        };
        print_statements(
            dialect,
            &format!("create index {kind:?}"),
            vec![compiler.compile_create_index_migration(CreateIndexMigration {
                table: "post".to_string(),
                index: index.clone(),
            })],
        );
        print_statements(
            dialect,
            &format!("drop index {kind:?}"),
            vec![compiler.compile_drop_index_migration(DropIndexMigration { table: "post".to_string(), index })],
        );
    }
    print_statements(
        dialect,
        "create enum",
        compiler.compile_create_enum_migration(CreateEnumMigration {
            name: "Status".to_string(),
            values: vec!["a".to_string(), "it's".to_string()],
        }),
    );
    print_statements(
        dialect,
        "drop enum",
        compiler.compile_drop_enum_migration(DropEnumMigration { name: "Status".to_string() }),
    );
    print_statements(
        dialect,
        "alter enum",
        compiler.compile_alter_enum_migration(AlterEnumMigration {
            name: "Status".to_string(),
            current_values: vec!["a".to_string()],
            desired_values: vec!["a".to_string(), "b".to_string(), "it's".to_string()],
        }),
    );
}

fn update_sets() -> Vec<UpdateSet> {
    let set = |field, value, operation| UpdateSet { field, value, operation };
    let many_to_many = ManyToManyUpdate {
        join_table: "_a_to_b",
        parent_field: "id",
        join_parent_field: "a_id",
        join_child_field: "b_id",
    };
    vec![
        set("email", value("new@dinoco.rs"), UpdateOperation::Set),
        set("group", DinocoValue::Integer(1), UpdateOperation::Increment),
        set("score", DinocoValue::Float(2.0), UpdateOperation::Decrement),
        set("ratio", DinocoValue::Integer(3), UpdateOperation::Multiply),
        set("half", DinocoValue::Integer(2), UpdateOperation::Divide),
        set("team_id", value("t1"), UpdateOperation::Connect),
        set("old_team_id", value("t0"), UpdateOperation::Disconnect),
        set("tags", value("x"), UpdateOperation::ConnectManyToMany(many_to_many)),
        set("tags", value("y"), UpdateOperation::DisconnectManyToMany(many_to_many)),
        UpdatedAtField::timestamp("updatedAt").update_set(),
        UpdatedAtField::date("touched_on").update_set(),
    ]
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
    let pgbouncer = PgBouncerAdapter::new("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");

    corpus("sqlite", &sqlite);
    corpus("postgres", &postgres);
    corpus("pgbouncer", &pgbouncer);
    corpus("mysql", &mysql);

    let post_update = UpdateQuery {
        table: "account",
        sets: update_sets(),
        conditions: vec![FindWhere::Eq("status", value("pending")), FindWhere::Gte("balance", DinocoValue::Integer(1))],
        returning: Some(&["id"]),
    };
    println!("## post_update_reload_conditions\n{:?}\n", post_update.post_update_reload_conditions());

    Ok(())
}
