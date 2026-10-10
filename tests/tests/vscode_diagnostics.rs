use std::collections::HashSet;

use dinoco_vscode::diagnostics::{
    CODE_MISSING_CONFIG, CODE_MISSING_DATABASE_URL, CODE_MISSING_SNOWFLAKE_NODE_ID, CODE_UNKNOWN_TYPE, analyze,
    analyze_imported,
};
use dinoco_vscode::document::DocumentIndex;
use dinoco_vscode::tower_lsp::lsp_types::NumberOrString;

#[test]
fn imported_documents_do_not_require_a_config_block() {
    let source = r#"enum AccountType {
            Personal
            Business
        }"#;
    let diagnostics = analyze_imported(source, &DocumentIndex::new(source));

    assert!(
        diagnostics.iter().all(|item| {
            item.code != Some(NumberOrString::String(CODE_MISSING_CONFIG.into()))
                && item.code != Some(NumberOrString::String("dinoco.schema".into()))
        }),
        "{diagnostics:#?}"
    );
}

#[test]
fn imported_documents_still_report_local_model_problems() {
    let source = "model Account { email String }";
    let diagnostics = analyze_imported(source, &DocumentIndex::new(source));

    assert!(
        diagnostics.iter().any(|item| item.code == Some(NumberOrString::String("dinoco.missingPrimaryKey".into()))),
        "{diagnostics:#?}"
    );
}

#[test]
fn imported_documents_never_report_project_config_problems() {
    let source = r#"config {
            database = "invalid"
            database = "sqlite"
            database_url = "inline.db"
            read_replicas = ["inline.db"]
            snowflake_node_id = 1
            with_logger = "yes"
            min_connection = 20
            max_connection = 10
            unknown = true
        }
        config {}
        model Account {
            id Integer @id @default(snowflake())
        }"#;
    let diagnostics = analyze_imported(source, &DocumentIndex::new(source));
    let project_config_codes = [
        CODE_MISSING_CONFIG,
        CODE_MISSING_DATABASE_URL,
        CODE_MISSING_SNOWFLAKE_NODE_ID,
        "dinoco.ambiguousConfig",
        "dinoco.duplicateConfig",
        "dinoco.duplicateConfigKey",
        "dinoco.invalidConnection",
        "dinoco.invalidDatabase",
        "dinoco.invalidDatabaseUrl",
        "dinoco.invalidImports",
        "dinoco.invalidLogger",
        "dinoco.invalidPoolRange",
        "dinoco.invalidPoolSize",
        "dinoco.invalidReadReplicas",
        "dinoco.invalidSnowflakeNodeId",
        "dinoco.missingDatabase",
        "dinoco.unknownConfigKey",
        "dinoco.unsupportedPoolSize",
    ];

    assert!(
        diagnostics.iter().all(|item| {
            !matches!(&item.code, Some(NumberOrString::String(code)) if project_config_codes.contains(&code.as_str()))
        }),
        "{diagnostics:#?}"
    );
}

#[test]
fn reports_semantic_schema_problems() {
    let source = r#"config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
        }
        model User {
            id String @id
            owner Missing
        }"#;
    let diagnostics = analyze(source, &DocumentIndex::new(source));
    assert!(diagnostics.iter().any(|item| item.code == Some(NumberOrString::String(CODE_UNKNOWN_TYPE.into()))));
}

#[test]
fn accepts_complete_workspace_configs() {
    let source = r#"config {
            workspace {
                dev { database = "sqlite" database_url = env("DEV_DATABASE_URL") }
                prod { database = "postgresql" database_url = env("PROD_DATABASE_URL") }
            }
        }
        model User { id String @id }
        "#;
    let diagnostics = analyze(source, &DocumentIndex::new(source));
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn accepts_types_loaded_by_main_config_imports() {
    let source = r#"config {
            imports = ["types.dinoco"]
            database = "sqlite"
            database_url = env("DATABASE_URL")
        }
        model Dashboard {
            id         String @id
            status     Status
            account_id String?
            account    Account? @relation(fields: [account_id], references: [id])
        }"#;
    let diagnostics = analyze(source, &DocumentIndex::new(source));

    assert!(
        !diagnostics.iter().any(|item| {
            matches!(
                &item.code,
                Some(NumberOrString::String(code))
                    if code == CODE_UNKNOWN_TYPE || code == "dinoco.invalidRelationTarget"
            )
        }),
        "{diagnostics:#?}"
    );
}

#[test]
fn reports_ambiguous_repeated_relations() {
    let source = r#"config { database = "sqlite" database_url = env("DATABASE_URL") }
        model User { id String @id posts Post[] comments Post[] }
        model Post {
            id String @id
            author User? @relation(fields: [author_id], references: [id])
            author_id String?
            editor User? @relation(fields: [editor_id], references: [id])
            editor_id String?
        }"#;
    let diagnostics = analyze(source, &DocumentIndex::new(source));
    assert!(
        diagnostics.iter().any(|item| item.code == Some(NumberOrString::String("dinoco.ambiguousRelation".into())))
    );
}

#[test]
fn reports_relation_and_snowflake_errors_together_at_their_fields() {
    let source = r#"config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
        }
        model Account {
            id       Integer @id @default(snowflake())
            business Business[]
        }
        model Business {
            id Integer @id
        }"#;
    let diagnostics = analyze(source, &DocumentIndex::new(source));

    let snowflake = diagnostics
        .iter()
        .find(|item| item.code == Some(NumberOrString::String(CODE_MISSING_SNOWFLAKE_NODE_ID.into())))
        .expect("snowflake diagnostic");
    assert_eq!(snowflake.range.start.line, 5);

    let relation = diagnostics
        .iter()
        .find(|item| item.code == Some(NumberOrString::String("dinoco.missingOppositeRelation".into())))
        .expect("opposite relation diagnostic");
    assert_eq!(relation.range.start.line, 6);
    assert!(relation.message.contains("Account.business"));
}

#[test]
fn reports_relation_key_ownership_and_cardinality_problems() {
    let source = r#"model User {
            id      Integer @id
            profile Profile?
            posts   Post[]
        }
        model Profile {
            id      Integer @id
            user_id Integer?
            user    User? @relation(fields: [user_id], references: [id])
        }
        model Post {
            id      Integer @id
            user_id Integer?
            user    User @relation(fields: [user_id], references: [id])
        }"#;
    let diagnostics = analyze(source, &DocumentIndex::new(source));
    let codes = diagnostics
        .iter()
        .filter_map(|item| match &item.code {
            Some(NumberOrString::String(code)) => Some(code.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();

    assert!(codes.contains("dinoco.oneToOneRequiresUnique"), "{diagnostics:#?}");
    assert!(codes.contains("dinoco.singularRelationMustBeOptional"), "{diagnostics:#?}");
}

#[test]
fn reports_unmaterializable_composite_relation_uniqueness() {
    let source = r#"model User {
            tenant Integer @unique
            id     Integer @unique
            detail Detail?
        }
        model Detail {
            id        Integer @id
            tenant_id Integer
            user_id   Integer
            user      User? @unique @relation(fields: [tenant_id, user_id], references: [tenant, id])
        }"#;
    let diagnostics = analyze(source, &DocumentIndex::new(source));
    let item = diagnostics
        .iter()
        .find(|item| item.code == Some(NumberOrString::String("dinoco.compositeRelationUnique".into())))
        .expect("composite uniqueness diagnostic");
    assert!(item.message.contains("Detail.user"));
}

#[test]
fn reports_invalid_inverse_mapping_and_inverse_actions() {
    let source = r#"model User {
            id     Integer @id
            legacy Integer @unique
            posts  Post[] @relation(fields: [legacy], references: [user_id], onDelete: Cascade)
        }
        model Post {
            id      Integer @id
            user_id Integer
            user    User? @relation(fields: [user_id], references: [id])
        }"#;
    let diagnostics = analyze(source, &DocumentIndex::new(source));

    assert!(
        diagnostics
            .iter()
            .any(|item| item.code == Some(NumberOrString::String("dinoco.invalidInverseRelationKeys".into())))
    );
    assert!(
        diagnostics
            .iter()
            .any(|item| item.code == Some(NumberOrString::String("dinoco.referentialOptionOnInverse".into())))
    );
}
