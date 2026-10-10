use dinoco_cli::db::{DatabaseEnum, DatabaseSchema, DatabaseTable};
use dinoco_cli::sql::{
    MigrationStep, generate_create_table_migrations, plan_database_migration, plan_schema_migration,
};
use dinoco_engine::{MigrationColumn, MigrationColumnType, MigrationDefault, MigrationForeignKey, ReferentialAction};

#[test]
fn plan_renames_legacy_pascal_case_tables_without_dropping_data_or_recreating_foreign_keys() {
    let desired = DatabaseSchema {
        tables: vec![
            table("account", vec![integer_column("id", true)], vec![], 0),
            table(
                "audio_creation",
                vec![integer_column("id", true), nullable_integer_column("account_id")],
                vec![MigrationForeignKey {
                    name: "fk_audio_creation_account_id".to_string(),
                    columns: vec!["account_id".to_string()],
                    references_table: "account".to_string(),
                    references_columns: vec!["id".to_string()],
                    on_update: ReferentialAction::Cascade,
                    on_delete: ReferentialAction::Cascade,
                }],
                0,
            ),
        ],
        enums: Vec::new(),
    };
    let current = DatabaseSchema {
        tables: vec![
            table("Account", vec![integer_column("id", true)], vec![], 17),
            table(
                "AudioCreation",
                vec![integer_column("id", true), nullable_integer_column("account_id")],
                vec![MigrationForeignKey {
                    name: "fk_AudioCreation_account_id".to_string(),
                    columns: vec!["account_id".to_string()],
                    references_table: "Account".to_string(),
                    references_columns: vec!["id".to_string()],
                    on_update: ReferentialAction::Cascade,
                    on_delete: ReferentialAction::Cascade,
                }],
                23,
            ),
        ],
        enums: Vec::new(),
    };

    let plan = plan_database_migration(&desired, &current);

    assert_eq!(plan.steps.len(), 2, "{:#?}", plan.steps);
    assert!(plan.steps.iter().any(
        |step| matches!(step, MigrationStep::RenameTable(item) if item.from == "Account" && item.to == "account")
    ));
    assert!(plan.steps.iter().any(
        |step| matches!(step, MigrationStep::RenameTable(item) if item.from == "AudioCreation" && item.to == "audio_creation")
    ));
    assert!(!plan.warnings.iter().any(|warning| warning.destructive));
    assert!(!plan.steps.iter().any(|step| matches!(step, MigrationStep::CreateTable(_) | MigrationStep::DropTable(_))));
}

#[test]
fn plan_detects_table_rename_by_matching_column_names_without_dropping_data() {
    let desired = DatabaseSchema {
        tables: vec![table("orders", vec![integer_column("id", true), nullable_integer_column("total")], vec![], 0)],
        enums: Vec::new(),
    };
    let current = DatabaseSchema {
        tables: vec![table(
            "legacy_orders",
            vec![integer_column("id", true), nullable_integer_column("total")],
            vec![],
            12,
        )],
        enums: Vec::new(),
    };

    let plan = plan_database_migration(&desired, &current);

    assert_eq!(plan.steps.len(), 1, "{:#?}", plan.steps);
    assert!(plan.steps.iter().any(
        |step| matches!(step, MigrationStep::RenameTable(item) if item.from == "legacy_orders" && item.to == "orders")
    ));
    assert!(!plan.steps.iter().any(|step| matches!(step, MigrationStep::CreateTable(_) | MigrationStep::DropTable(_))));
    assert!(plan.errors.is_empty());
    assert!(plan.warnings.iter().any(|warning| warning.message.contains("looks like it was renamed")));
}

#[test]
fn plan_rejects_ambiguous_table_rename_and_falls_back_to_drop_and_create() {
    let desired = DatabaseSchema {
        tables: vec![table("orders", vec![integer_column("id", true), nullable_integer_column("total")], vec![], 0)],
        enums: Vec::new(),
    };
    let current = DatabaseSchema {
        tables: vec![
            table("legacy_orders_a", vec![integer_column("id", true), nullable_integer_column("total")], vec![], 5),
            table("legacy_orders_b", vec![integer_column("id", true), nullable_integer_column("total")], vec![], 7),
        ],
        enums: Vec::new(),
    };

    let plan = plan_database_migration(&desired, &current);

    assert!(!plan.errors.is_empty(), "ambiguous table rename must be reported as an error");
    assert!(!plan.steps.iter().any(|step| matches!(step, MigrationStep::RenameTable(_))));
    assert!(plan.steps.iter().any(|step| matches!(step, MigrationStep::CreateTable(item) if item.table == "orders")));
    assert!(
        plan.steps.iter().any(|step| matches!(step, MigrationStep::DropTable(item) if item.table == "legacy_orders_a"))
    );
    assert!(
        plan.steps.iter().any(|step| matches!(step, MigrationStep::DropTable(item) if item.table == "legacy_orders_b"))
    );
}

#[test]
fn plan_detects_dropped_column_with_existing_rows_as_destructive() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id    String @id
                email String
            }
            "#,
    )
    .expect("schema");
    let current = DatabaseSchema {
        tables: vec![DatabaseTable {
            name: "user".to_string(),
            row_count: 2,
            columns: vec![
                MigrationColumn {
                    name: "id".to_string(),
                    ty: MigrationColumnType::String,
                    primary_key: true,
                    unique: false,
                    nullable: false,
                    default: None,
                },
                MigrationColumn {
                    name: "email".to_string(),
                    ty: MigrationColumnType::String,
                    primary_key: false,
                    unique: false,
                    nullable: false,
                    default: None,
                },
                MigrationColumn {
                    name: "password".to_string(),
                    ty: MigrationColumnType::String,
                    primary_key: false,
                    unique: false,
                    nullable: false,
                    default: None,
                },
            ],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }],
        enums: Vec::new(),
    };

    let plan = plan_schema_migration(&schema, &current);

    assert!(
        plan.steps.iter().any(|step| matches!(step, MigrationStep::DropColumn(column) if column.column == "password"))
    );
    assert!(plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("data will be lost")));
}

#[test]
fn plan_detects_added_required_column_on_populated_table_as_destructive() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id     String @id
                email  String
                office String
            }
            "#,
    )
    .expect("schema");
    let current = DatabaseSchema {
        tables: vec![DatabaseTable {
            name: "user".to_string(),
            row_count: 1,
            columns: vec![
                MigrationColumn {
                    name: "id".to_string(),
                    ty: MigrationColumnType::String,
                    primary_key: true,
                    unique: false,
                    nullable: false,
                    default: None,
                },
                MigrationColumn {
                    name: "email".to_string(),
                    ty: MigrationColumnType::String,
                    primary_key: false,
                    unique: false,
                    nullable: false,
                    default: None,
                },
            ],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }],
        enums: Vec::new(),
    };

    let plan = plan_schema_migration(&schema, &current);

    assert!(
        plan.steps
            .iter()
            .any(|step| matches!(step, MigrationStep::AddColumn(column) if column.column.name == "office"))
    );
    assert!(plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("without a default")));
}

#[test]
fn plan_detects_optional_field_becoming_required_as_destructive() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id    String @id
                email String
            }
            "#,
    )
    .expect("schema");
    let current = DatabaseSchema {
        tables: vec![DatabaseTable {
            name: "user".to_string(),
            row_count: 3,
            columns: vec![string_column("id", true), nullable_string_column("email")],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }],
        enums: Vec::new(),
    };

    let plan = plan_schema_migration(&schema, &current);

    assert!(plan.steps.iter().any(
        |step| matches!(step, MigrationStep::AlterColumn(column) if column.desired.name == "email" && !column.desired.nullable)
    ));
    assert!(
        plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("will become required"))
    );
}

#[test]
fn plan_detects_required_field_becoming_optional_as_safe_alter() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id    String  @id
                email String?
            }
            "#,
    )
    .expect("schema");
    let current = DatabaseSchema {
        tables: vec![DatabaseTable {
            name: "user".to_string(),
            row_count: 3,
            columns: vec![string_column("id", true), string_column("email", false)],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }],
        enums: Vec::new(),
    };

    let plan = plan_schema_migration(&schema, &current);

    assert!(plan.steps.iter().any(
        |step| matches!(step, MigrationStep::AlterColumn(column) if column.desired.name == "email" && column.desired.nullable)
    ));
    assert!(
        plan.warnings.iter().any(|warning| !warning.destructive && warning.message.contains("will become optional"))
    );
}

#[test]
fn plan_detects_enum_additions_and_removals() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            enum OfficeType {
                admin
                owner
            }
            "#,
    )
    .expect("schema");
    let current = DatabaseSchema {
        tables: Vec::new(),
        enums: vec![DatabaseEnum {
            name: "OfficeType".to_string(),
            values: vec!["admin".to_string(), "member".to_string()],
        }],
    };

    let plan = plan_schema_migration(&schema, &current);

    assert!(plan.steps.iter().any(|step| matches!(step, MigrationStep::AlterEnum(item) if item.name == "OfficeType")));
    assert!(plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("removes values")));
}

/// Dinoco has no reliable signal to tell an enum rename apart from an
/// unrelated enum being dropped and a new one created with the same
/// values by coincidence, so it must never guess: a same-name-only match
/// falls back to a plain drop + create, each with its own warning.
#[test]
fn plan_treats_enum_rename_as_drop_and_create_when_only_values_match() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            enum Role {
                admin
                member
            }
            "#,
    )
    .expect("schema");
    let current = DatabaseSchema {
        tables: Vec::new(),
        enums: vec![DatabaseEnum {
            name: "OldRole".to_string(),
            values: vec!["admin".to_string(), "member".to_string()],
        }],
    };

    let plan = plan_schema_migration(&schema, &current);

    assert!(plan.steps.iter().any(|step| matches!(step, MigrationStep::CreateEnum(item) if item.name == "Role")));
    assert!(plan.steps.iter().any(|step| matches!(step, MigrationStep::DropEnum(item) if item.name == "OldRole")));
    assert!(!plan.steps.iter().any(|step| matches!(step, MigrationStep::AlterEnum(_))));
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.destructive && warning.message.contains("`OldRole` will be dropped"))
    );
}

#[test]
fn desired_schema_includes_enum_columns_defaults_and_many_to_many_join_tables() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            enum Role {
                USER
                ADMIN
            }

            model User {
                id     Integer @id @default(autoincrement())
                role   Role    @default(USER)
                posts  Post[]
            }

            model Post {
                id     Integer @id @default(autoincrement())
                users  User[]
            }
            "#,
    )
    .expect("schema");

    let migrations = generate_create_table_migrations(&schema);
    let user = migrations.iter().find(|migration| migration.table == "user").expect("user table");
    let role = user.columns.iter().find(|column| column.name == "role").expect("role column");

    assert!(matches!(role.ty, MigrationColumnType::Enum { ref name, .. } if name == "Role"));
    assert_eq!(role.default, Some(MigrationDefault::String("USER".to_string())));
    assert!(migrations.iter().any(|migration| migration.table == "_post_to_user"));
}

#[test]
fn desired_schema_keeps_model_acronyms_together_in_table_names() {
    let schema = dinoco_compiler::compile(
        r#"
            model BusinessCNAE {
                id String @id
            }

            model BusinessOffice {
                id String @id
            }
            "#,
    )
    .expect("schema");

    let migrations = generate_create_table_migrations(&schema);
    let names = migrations.iter().map(|migration| migration.table.as_str()).collect::<Vec<_>>();

    assert!(names.contains(&"business_cnae"));
    assert!(names.contains(&"business_office"));
    assert!(!names.contains(&"business_c_n_a_e"));
}

#[test]
fn desired_schema_uses_relation_names_to_disambiguate_repeated_many_to_many_relations() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            model User {
                id         Integer @id @default(autoincrement())
                following  User[]  @relation(name: "following")
                followers  User[]  @relation(name: "following")
                blocked    User[]  @relation(name: "blocked")
                blocked_by User[]  @relation(name: "blocked")
            }
            "#,
    )
    .expect("schema");

    let migrations = generate_create_table_migrations(&schema);

    assert!(migrations.iter().any(|migration| migration.table == "_user_to_user_following"));
    assert!(migrations.iter().any(|migration| migration.table == "_user_to_user_blocked"));
    assert_eq!(migrations.iter().filter(|migration| migration.table.starts_with("_user_to_user")).count(), 2);
}

#[test]
fn relation_field_unique_is_materialized_on_its_single_foreign_key_column() {
    let schema = dinoco_compiler::compile(
        r#"
            model User {
                id      Integer  @id
                profile Profile?
            }

            model Profile {
                id      Integer @id
                user_id Integer?
                user    User?   @unique @relation(fields: [user_id], references: [id])
            }
            "#,
    )
    .expect("one-to-one schema");

    let profile = generate_create_table_migrations(&schema)
        .into_iter()
        .find(|migration| migration.table == "profile")
        .expect("profile table");
    assert!(profile.columns.iter().any(|column| column.name == "user_id" && column.unique));
}

#[test]
fn desired_schema_includes_relation_foreign_key_actions() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            model User {
                id    Integer @id @default(autoincrement())
                posts Post[]
            }

            model Post {
                id      Integer @id @default(autoincrement())
                user_id Integer?
                user    User?   @relation(fields: [user_id], references: [id], onDelete: SetNull, onUpdate: Cascade)
            }
            "#,
    )
    .expect("schema");

    let migrations = generate_create_table_migrations(&schema);
    let post = migrations.iter().find(|migration| migration.table == "post").expect("post table");
    let foreign_key = post.foreign_keys.iter().find(|foreign_key| foreign_key.name == "fk_post_user_id").unwrap();

    assert_eq!(foreign_key.on_delete, ReferentialAction::SetNull);
    assert_eq!(foreign_key.on_update, ReferentialAction::Cascade);
}

#[test]
fn plan_detects_column_rename_without_data_loss() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id        String @id
                full_name String
            }
            "#,
    )
    .expect("schema");
    let current = DatabaseSchema {
        tables: vec![DatabaseTable {
            name: "user".to_string(),
            row_count: 5,
            columns: vec![string_column("id", true), string_column("name", false)],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }],
        enums: Vec::new(),
    };

    let plan = plan_schema_migration(&schema, &current);

    assert!(plan.steps.iter().any(
        |step| matches!(step, MigrationStep::RenameColumn(item) if item.from == "name" && item.to == "full_name")
    ));
    assert!(!plan.steps.iter().any(|step| matches!(step, MigrationStep::DropColumn(_))));
    assert!(!plan.steps.iter().any(|step| matches!(step, MigrationStep::AddColumn(_))));
    assert!(plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("renamed")));
}

#[test]
fn plan_detects_relation_add_remove_and_referential_action_changes() {
    let current_fk = MigrationForeignKey {
        name: "fk_post_user_id".to_string(),
        columns: vec!["user_id".to_string()],
        references_table: "user".to_string(),
        references_columns: vec!["id".to_string()],
        on_update: ReferentialAction::NoAction,
        on_delete: ReferentialAction::NoAction,
    };
    let desired_fk = MigrationForeignKey {
        on_update: ReferentialAction::Cascade,
        on_delete: ReferentialAction::SetNull,
        ..current_fk.clone()
    };

    let current = DatabaseSchema {
        tables: vec![
            table("user", vec![integer_column("id", true)], vec![], 1),
            table("post", vec![integer_column("id", true), nullable_integer_column("user_id")], vec![current_fk], 2),
            table(
                "old_relation",
                vec![integer_column("id", true), integer_column("user_id", false)],
                vec![MigrationForeignKey {
                    name: "fk_old_relation_user_id".to_string(),
                    columns: vec!["user_id".to_string()],
                    references_table: "user".to_string(),
                    references_columns: vec!["id".to_string()],
                    on_update: ReferentialAction::NoAction,
                    on_delete: ReferentialAction::NoAction,
                }],
                0,
            ),
        ],
        enums: Vec::new(),
    };
    let desired = DatabaseSchema {
        tables: vec![
            table("user", vec![integer_column("id", true)], vec![], 0),
            table("post", vec![integer_column("id", true), nullable_integer_column("user_id")], vec![desired_fk], 0),
            table("old_relation", vec![integer_column("id", true), integer_column("user_id", false)], vec![], 0),
            table(
                "new_relation",
                vec![integer_column("id", true), integer_column("user_id", false)],
                vec![MigrationForeignKey {
                    name: "fk_new_relation_user_id".to_string(),
                    columns: vec!["user_id".to_string()],
                    references_table: "user".to_string(),
                    references_columns: vec!["id".to_string()],
                    on_update: ReferentialAction::Restrict,
                    on_delete: ReferentialAction::Cascade,
                }],
                0,
            ),
        ],
        enums: Vec::new(),
    };

    let plan = plan_database_migration(&desired, &current);

    assert!(
        plan.steps
            .iter()
            .any(|step| matches!(step, MigrationStep::DropForeignKey(item) if item.name == "fk_post_user_id"))
    );
    assert!(plan.steps.iter().any(
        |step| matches!(step, MigrationStep::AddForeignKey(item) if item.foreign_key.name == "fk_post_user_id" && item.foreign_key.on_delete == ReferentialAction::SetNull)
    ));
    assert!(
        plan.steps
            .iter()
            .any(|step| matches!(step, MigrationStep::DropForeignKey(item) if item.name == "fk_old_relation_user_id"))
    );
    assert!(plan.steps.iter().any(
        |step| matches!(step, MigrationStep::CreateTable(item) if item.table == "new_relation" && item.foreign_keys.len() == 1)
    ));
}

#[test]
fn desired_schema_supports_all_relation_shapes() {
    let schema = dinoco_compiler::compile(
        r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            model User {
                id         Integer @id @default(autoincrement())
                manager_id Integer?
                manager    User?   @relation(name: "management", fields: [manager_id], references: [id], onDelete: SetNull)
                reports    User[]  @relation(name: "management")
                posts      Post[]
                profile    Profile?
                groups     Group[]
                following  User[]  @relation(name: "following")
                followers  User[]  @relation(name: "following")
            }

            model Post {
                id        Integer @id @default(autoincrement())
                author_id Integer
                author    User?    @relation(fields: [author_id], references: [id], onDelete: Cascade)
            }

            model Profile {
                id      Integer @id @default(autoincrement())
                user_id Integer @unique
                user    User?    @relation(fields: [user_id], references: [id], onDelete: Cascade)
            }

            model Group {
                id    Integer @id @default(autoincrement())
                users User[]
            }
            "#,
    )
    .expect("schema");

    let migrations = generate_create_table_migrations(&schema);
    let user = migrations.iter().find(|migration| migration.table == "user").expect("user table");
    let post = migrations.iter().find(|migration| migration.table == "post").expect("post table");
    let profile = migrations.iter().find(|migration| migration.table == "profile").expect("profile table");

    assert!(user.foreign_keys.iter().any(|fk| {
        fk.name == "fk_user_manager_id" && fk.references_table == "user" && fk.on_delete == ReferentialAction::SetNull
    }));
    assert!(post.foreign_keys.iter().any(|fk| {
        fk.name == "fk_post_author_id" && fk.references_table == "user" && fk.on_delete == ReferentialAction::Cascade
    }));
    assert!(profile.foreign_keys.iter().any(|fk| {
        fk.name == "fk_profile_user_id" && fk.references_table == "user" && fk.on_delete == ReferentialAction::Cascade
    }));
    assert!(
        profile.columns.iter().any(|column| column.name == "user_id" && column.unique),
        "one-to-one relation uniqueness must be materialized in the database"
    );
    assert!(
        !migrations.iter().any(|migration| migration.table == "_post_to_user"),
        "one-to-many list sides must not create an implicit join table"
    );
    for join_table in ["_group_to_user", "_user_to_user_following"] {
        let join = migrations.iter().find(|migration| migration.table == join_table).expect("many-to-many table");
        assert_eq!(
            join.columns.iter().filter(|column| column.primary_key).count(),
            2,
            "implicit many-to-many tables need a composite primary key"
        );
    }
}

fn table(
    name: &str,
    columns: Vec<MigrationColumn>,
    foreign_keys: Vec<MigrationForeignKey>,
    row_count: i64,
) -> DatabaseTable {
    DatabaseTable { name: name.to_string(), row_count, columns, foreign_keys, indexes: Vec::new() }
}

fn string_column(name: &str, primary_key: bool) -> MigrationColumn {
    MigrationColumn {
        name: name.to_string(),
        ty: MigrationColumnType::String,
        primary_key,
        unique: false,
        nullable: false,
        default: None,
    }
}

fn nullable_string_column(name: &str) -> MigrationColumn {
    MigrationColumn {
        name: name.to_string(),
        ty: MigrationColumnType::String,
        primary_key: false,
        unique: false,
        nullable: true,
        default: None,
    }
}

fn integer_column(name: &str, primary_key: bool) -> MigrationColumn {
    MigrationColumn {
        name: name.to_string(),
        ty: MigrationColumnType::Integer,
        primary_key,
        unique: false,
        nullable: false,
        default: None,
    }
}

fn nullable_integer_column(name: &str) -> MigrationColumn {
    MigrationColumn {
        name: name.to_string(),
        ty: MigrationColumnType::Integer,
        primary_key: false,
        unique: false,
        nullable: true,
        default: None,
    }
}
