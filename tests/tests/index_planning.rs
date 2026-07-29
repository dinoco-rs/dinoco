use dinoco_cli::db::DatabaseSchema;
use dinoco_cli::sql::{MigrationStep, desired_database_schema, plan_database_migration};
use dinoco_engine::MigrationIndexKind;

#[test]
fn desired_schema_indexes_explicit_fields_primary_keys_and_every_foreign_key() {
    let schema = dinoco_compiler::compile(
        r#"
        model User {
            tenant Integer @unique
            id     Integer @id
            posts  Post[]
            groups Group[]

            @@uniques([tenant, id])
        }

        model Post {
            id        Integer @id
            slug      String  @index(map: "idx_post_slug_custom")
            tenant_id Integer
            user_id   Integer
            author    User @relation(fields: [tenant_id, user_id], references: [tenant, id])
        }

        model Group {
            id    Integer @id
            users User[]
        }
        "#,
    )
    .expect("indexed schema");

    let desired = desired_database_schema(&schema);
    let post = desired.tables.iter().find(|table| table.name == "post").expect("post table");
    assert!(post.indexes.iter().any(|index| {
        index.name == "idx_post_slug_custom" && index.columns == ["slug".to_string()] && !index.automatic
    }));
    assert!(post.indexes.iter().any(|index| {
        index.name == "idx_post_tenant_id_user_id"
            && index.columns == ["tenant_id".to_string(), "user_id".to_string()]
            && index.automatic
    }));

    for table in &desired.tables {
        let primary_columns = table
            .columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.clone())
            .collect::<Vec<_>>();
        assert!(
            table.indexes.iter().any(|index| index.automatic && index.columns == primary_columns),
            "primary key on {} must be represented in the desired indexes",
            table.name
        );

        for foreign_key in &table.foreign_keys {
            assert!(
                table.indexes.iter().any(|index| index.automatic && index.columns == foreign_key.columns),
                "foreign key {}.{} must receive an index",
                table.name,
                foreign_key.name
            );
        }
    }

    let join = desired.tables.iter().find(|table| table.name == "_group_to_user").expect("join table");
    assert_eq!(join.foreign_keys.len(), 2);
    assert_eq!(
        join.indexes.iter().filter(|index| index.automatic).count(),
        3,
        "the pivot needs its composite primary-key index and one index per foreign key"
    );

    let plan = plan_database_migration(&desired, &DatabaseSchema::default());
    for table in &desired.tables {
        let primary_columns = table
            .columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.clone())
            .collect::<Vec<_>>();
        assert!(
            !plan.steps.iter().any(|step| matches!(
                step,
                MigrationStep::CreateIndex(item)
                    if item.table == table.name && item.index.columns == primary_columns
            )),
            "PRIMARY KEY must not be followed by a redundant CREATE INDEX on {}",
            table.name
        );
    }

    let mut introspected = desired.clone();
    for table in &mut introspected.tables {
        let primary_columns = table
            .columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.clone())
            .collect::<Vec<_>>();
        table.indexes.retain(|index| !(index.automatic && index.columns == primary_columns));
    }
    assert!(
        plan_database_migration(&desired, &introspected).steps.is_empty(),
        "a primary-key constraint must satisfy its automatic index"
    );

    let mut database_managed = desired.clone();
    let post = database_managed.tables.iter_mut().find(|table| table.name == "post").expect("database-managed post");
    let relation_index =
        post.indexes.iter_mut().find(|index| index.name == "idx_post_tenant_id_user_id").expect("relation index");
    relation_index.name = "fk_post_tenant_id_user_id".to_string();
    relation_index.automatic = false;
    assert!(
        plan_database_migration(&desired, &database_managed).steps.is_empty(),
        "an existing index with the same ordered foreign-key columns must satisfy the automatic index"
    );
}

#[test]
fn fulltext_indexes_are_native_only_on_postgres_and_mysql() {
    for database in ["postgresql", "mysql"] {
        let schema = dinoco_compiler::compile(&format!(
            r#"
            config {{
                database = "{database}"
                database_url = env("DATABASE_URL")
            }}

            model Article {{
                id      Integer @id
                title   String  @fulltext
                summary String? @fulltext
            }}
            "#
        ))
        .expect("native full-text schema");
        let desired = desired_database_schema(&schema);
        let article = desired.tables.iter().find(|table| table.name == "article").expect("article");
        assert_eq!(article.indexes.iter().filter(|index| index.kind == MigrationIndexKind::FullText).count(), 2);

        let mut wrong_kind = desired.clone();
        wrong_kind.tables[0]
            .indexes
            .iter_mut()
            .find(|index| index.kind == MigrationIndexKind::FullText)
            .expect("full-text index")
            .kind = MigrationIndexKind::Standard;
        let plan = plan_database_migration(&desired, &wrong_kind);
        assert!(plan.steps.iter().any(|step| matches!(step, MigrationStep::DropIndex(_))));
        assert!(plan.steps.iter().any(|step| matches!(step, MigrationStep::CreateIndex(_))));
    }

    let sqlite = dinoco_compiler::compile(
        r#"
        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
        }

        model Article {
            id    Integer @id
            title String @fulltext
        }
        "#,
    )
    .expect("SQLite full-text fallback schema");
    assert!(
        desired_database_schema(&sqlite)
            .tables
            .iter()
            .flat_map(|table| &table.indexes)
            .all(|index| index.kind != MigrationIndexKind::FullText)
    );
}

#[test]
fn composite_model_attributes_drive_columns_and_indexes() {
    let schema = dinoco_compiler::compile(
        r#"
        config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
        }

        model Article {
            tenantId String
            id       String
            slug     String
            category String
            title    String
            body     String?

            @@ids([tenantId, id])
            @@uniques([tenantId, slug])
            @@indexes([tenantId, category])
            @@fulltexts([title, body])
            @@table_name("search_articles")
        }
        "#,
    )
    .expect("composite schema");

    let desired = desired_database_schema(&schema);
    let article = desired.tables.iter().find(|table| table.name == "search_articles").expect("mapped article");
    assert_eq!(
        article
            .columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["tenantId", "id"]
    );
    assert!(article.indexes.iter().any(|index| {
        index.kind == MigrationIndexKind::Unique && index.columns == ["tenantId".to_string(), "slug".to_string()]
    }));
    assert!(article.indexes.iter().any(|index| {
        index.kind == MigrationIndexKind::Standard
            && !index.automatic
            && index.columns == ["tenantId".to_string(), "category".to_string()]
    }));
    assert!(article.indexes.iter().any(|index| {
        index.kind == MigrationIndexKind::FullText && index.columns == ["title".to_string(), "body".to_string()]
    }));

    let plan = plan_database_migration(&desired, &DatabaseSchema::default());
    assert!(plan.steps.iter().any(|step| matches!(
        step,
        MigrationStep::CreateIndex(item) if item.index.kind == MigrationIndexKind::Unique
    )));
}

#[test]
fn planner_creates_and_drops_explicit_indexes() {
    let schema = dinoco_compiler::compile(
        r#"
        model User {
            id    Integer @id
            email String  @index
        }
        "#,
    )
    .expect("indexed schema");
    let desired = desired_database_schema(&schema);
    let mut current = desired.clone();
    current.tables[0].indexes.retain(|index| index.name != "idx_user_email");

    let create = plan_database_migration(&desired, &current);
    assert!(
        create
            .steps
            .iter()
            .any(|step| matches!(step, MigrationStep::CreateIndex(item) if item.index.name == "idx_user_email"))
    );

    let drop = plan_database_migration(&current, &desired);
    assert!(
        drop.steps
            .iter()
            .any(|step| matches!(step, MigrationStep::DropIndex(item) if item.index.name == "idx_user_email"))
    );
}

#[test]
fn legacy_schema_snapshots_without_indexes_remain_readable() {
    let legacy = dinoco_engine::serde_json::json!({
        "tables": [{
            "name": "post",
            "row_count": 0,
            "columns": [],
            "foreign_keys": []
        }],
        "enums": []
    });

    let schema: DatabaseSchema = dinoco_engine::serde_json::from_value(legacy).expect("legacy snapshot");
    assert_eq!(schema.tables.len(), 1);
    assert!(schema.tables[0].indexes.is_empty());
}
