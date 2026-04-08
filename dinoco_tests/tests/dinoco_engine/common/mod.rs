#![allow(dead_code)]

use std::env;

use dinoco_compiler::{
    ConnectionUrl, Database, ParsedConfig, ParsedEnum, ParsedField, ParsedFieldDefault, ParsedFieldType,
    ParsedRelation, ParsedSchema, ParsedTable, ReferentialAction,
};
use dinoco_engine::{DinocoAdapter, DinocoError, DinocoResult, MigrationStep};
use uuid::Uuid;

pub fn postgres_url() -> String {
    database_url(
        &["DINOCO_POSTGRES_DATABASE_URL", "POSTGRES_DATABASE_URL"],
        "postgres://postgres:root@localhost:5432/dinoco",
    )
}

pub fn mysql_url() -> String {
    database_url(&["DINOCO_MYSQL_DATABASE_URL", "MYSQL_DATABASE_URL"], "mysql://root:root@localhost:3306/dinoco")
}

pub fn sqlite_url(name: &str) -> String {
    let mut path = env::temp_dir();

    path.push(format!("dinoco-engine-{name}-{}.sqlite", Uuid::now_v7()));

    format!("file:{}", path.display())
}

pub fn unique_name(prefix: &str) -> String {
    let id = Uuid::now_v7().to_string().replace('-', "");
    let suffix = &id[..10];

    format!("{prefix}_{suffix}")
}

pub async fn apply_sqls<A: DinocoAdapter>(adapter: &A, sqls: &[String]) -> DinocoResult<()> {
    for sql in sqls {
        adapter.execute(sql, &[]).await?;
    }

    Ok(())
}

pub fn migration_schema(prefix: &str) -> ParsedSchema {
    let status_enum = format!("{prefix}_status");
    let teams_table = format!("{prefix}_teams");
    let users_table = format!("{prefix}_users");

    ParsedSchema {
        config: ParsedConfig {
            database: Database::Sqlite,
            database_url: ConnectionUrl::Literal("file:./dinoco/database.sqlite".to_string()),
            read_replicas: vec![],
        },
        enums: vec![ParsedEnum {
            name: status_enum.clone(),
            values: vec!["ACTIVE".to_string(), "DISABLED".to_string()],
        }],
        tables: vec![
            ParsedTable {
                name: teams_table.clone(),
                database_name: teams_table,
                primary_key_fields: vec!["id".to_string()],
                fields: vec![integer_field("id", true), string_field("name", false, false)],
            },
            ParsedTable {
                name: users_table.clone(),
                database_name: users_table,
                primary_key_fields: vec!["id".to_string()],
                fields: vec![
                    integer_field("id", true),
                    string_field("email", false, true),
                    ParsedField {
                        name: "status".to_string(),
                        field_type: ParsedFieldType::Enum(status_enum),
                        is_primary_key: false,
                        is_optional: false,
                        is_unique: false,
                        is_list: false,
                        relation: ParsedRelation::NotDefined,
                        default_value: ParsedFieldDefault::EnumValue("ACTIVE".to_string()),
                    },
                    ParsedField {
                        name: "team_id".to_string(),
                        field_type: ParsedFieldType::Integer,
                        is_primary_key: false,
                        is_optional: false,
                        is_unique: false,
                        is_list: false,
                        relation: ParsedRelation::NotDefined,
                        default_value: ParsedFieldDefault::NotDefined,
                    },
                    ParsedField {
                        name: "team".to_string(),
                        field_type: ParsedFieldType::Relation(format!("{prefix}_teams")),
                        is_primary_key: false,
                        is_optional: false,
                        is_unique: false,
                        is_list: false,
                        relation: ParsedRelation::ManyToOne(
                            None,
                            vec!["team_id".to_string()],
                            vec!["id".to_string()],
                            Some(ReferentialAction::Cascade),
                            Some(ReferentialAction::Cascade),
                        ),
                        default_value: ParsedFieldDefault::NotDefined,
                    },
                ],
            },
        ],
    }
}

pub fn migration_steps(prefix: &str) -> Vec<MigrationStep> {
    let schema = migration_schema(prefix);
    let status_enum = format!("{prefix}_status");
    let teams_table = format!("{prefix}_teams");
    let users_table = format!("{prefix}_users");
    let teams = schema.tables[0].clone();
    let users = schema.tables[1].clone();

    vec![
        MigrationStep::CreateEnum { name: status_enum, variants: vec!["ACTIVE".to_string(), "DISABLED".to_string()] },
        MigrationStep::CreateTable(teams),
        MigrationStep::CreateTable(users),
        MigrationStep::AddForeignKey {
            table_name: users_table.clone(),
            columns: vec!["team_id".to_string()],
            referenced_table: teams_table,
            referenced_columns: vec!["id".to_string()],
            on_delete: Some(ReferentialAction::Cascade),
            on_update: Some(ReferentialAction::Cascade),
            constraint_name: format!("fk_{users_table}_team_id"),
        },
        MigrationStep::CreateIndex {
            table_name: users_table.clone(),
            columns: vec!["status".to_string()],
            index_name: format!("idx_{users_table}_status"),
            is_unique: false,
        },
        MigrationStep::CreateIndex {
            table_name: users_table.clone(),
            columns: vec!["email".to_string()],
            index_name: format!("idx_{users_table}_email_lookup"),
            is_unique: true,
        },
    ]
}

pub fn alter_enum_schema(prefix: &str) -> ParsedSchema {
    let status_enum = format!("{prefix}_status");
    let users_table = format!("{prefix}_users");

    ParsedSchema {
        config: ParsedConfig {
            database: Database::Sqlite,
            database_url: ConnectionUrl::Literal("file:./dinoco/database.sqlite".to_string()),
            read_replicas: vec![],
        },
        enums: vec![ParsedEnum {
            name: status_enum.clone(),
            values: vec!["ACTIVE".to_string(), "ARCHIVED".to_string()],
        }],
        tables: vec![ParsedTable {
            name: users_table.clone(),
            database_name: users_table,
            primary_key_fields: vec!["id".to_string()],
            fields: vec![
                integer_field("id", true),
                ParsedField {
                    name: "status".to_string(),
                    field_type: ParsedFieldType::Enum(status_enum),
                    is_primary_key: false,
                    is_optional: false,
                    is_unique: false,
                    is_list: false,
                    relation: ParsedRelation::NotDefined,
                    default_value: ParsedFieldDefault::EnumValue("ACTIVE".to_string()),
                },
            ],
        }],
    }
}

pub fn alter_enum_step(prefix: &str) -> MigrationStep {
    MigrationStep::AlterEnum {
        name: format!("{prefix}_status"),
        old_variants: vec!["ACTIVE".to_string(), "DISABLED".to_string()],
        new_variants: vec!["ACTIVE".to_string(), "ARCHIVED".to_string()],
    }
}

fn database_url(keys: &[&str], default: &str) -> String {
    keys.iter()
        .find_map(|key| env::var(key).ok())
        .or_else(|| env::var("DATABASE_URL").ok())
        .unwrap_or_else(|| default.to_string())
}

pub fn should_skip_external_adapter_test(error: &DinocoError) -> bool {
    match error {
        DinocoError::ConnectionError(_) => true,
        DinocoError::MySql(mysql_error) => mysql_error.to_string().contains("Operation not permitted"),
        DinocoError::Postgres(postgres_error) => postgres_error.to_string().contains("error connecting to server"),
        _ => false,
    }
}

fn integer_field(name: &str, is_primary_key: bool) -> ParsedField {
    ParsedField {
        name: name.to_string(),
        field_type: ParsedFieldType::Integer,
        is_primary_key,
        is_optional: false,
        is_unique: false,
        is_list: false,
        relation: ParsedRelation::NotDefined,
        default_value: ParsedFieldDefault::NotDefined,
    }
}

fn string_field(name: &str, is_optional: bool, is_unique: bool) -> ParsedField {
    ParsedField {
        name: name.to_string(),
        field_type: ParsedFieldType::String,
        is_primary_key: false,
        is_optional,
        is_unique,
        is_list: false,
        relation: ParsedRelation::NotDefined,
        default_value: ParsedFieldDefault::NotDefined,
    }
}
