use std::fs;

use dinoco_compiler::{ConfigValue, compile, compile_file, parse};
use tempfile::tempdir;

#[test]
fn compile_parses_config_enums_models_and_relations() {
    let schema = compile(
        r#"
        config {
            database = "postgresql"
            connection = "direct"
            database_url = env("DATABASE_URL")
            read_replicas = [env("DATABASE_REPLICA_URL")]
        }

        enum OfficeType {
            admin
            member
        }

        model User {
            id      String      @id @default(uuid())
            office  OfficeType
            tokens  UserToken[] @relation(fields: [id], references: [userId])
        }

        model UserToken {
            id      String  @id @default(uuid())
            userId  String?
            user    User?   @relation(fields: [userId], references: [id])
        }
        "#,
    )
    .expect("schema should compile");

    assert_eq!(schema.models().count(), 2);
    assert_eq!(schema.enums().next().expect("enum").name, "OfficeType");

    let config = schema.config().expect("config");
    let database_url = config.entries.iter().find(|entry| entry.key == "database_url").expect("database_url");
    assert!(matches!(&database_url.value, ConfigValue::Env(name) if name == "DATABASE_URL"));
}

#[test]
fn compile_parses_and_selects_workspace_configs() {
    let schema = compile(
        r#"
        config {
            workspace {
                dev {
                    database = "sqlite"
                    database_url = env("DEV_DATABASE_URL")
                }

                prod {
                    database = "postgresql"
                    connection = "pgbouncer"
                    database_url = env("PROD_DATABASE_URL")
                }
            }
        }

        model User {
            id String @id
        }
        "#,
    )
    .expect("workspace schema should compile");

    assert_eq!(schema.workspaces().map(|workspace| workspace.name.as_str()).collect::<Vec<_>>(), ["dev", "prod"]);
    assert!(schema.config().expect("config").entries.is_empty());

    let prod = schema.for_workspace("prod").expect("prod workspace");
    let config = prod.config().expect("selected config");
    assert!(config.workspaces.is_empty());
    assert!(matches!(
        config.entries.iter().find(|entry| entry.key == "database_url").map(|entry| &entry.value),
        Some(ConfigValue::Env(name)) if name == "PROD_DATABASE_URL"
    ));
    assert!(schema.for_workspace("missing").is_none());
}

#[test]
fn compile_rejects_ambiguous_or_invalid_workspace_configs() {
    let mixed = compile(
        r#"
        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
            workspace {
                dev { database = "sqlite" database_url = env("DEV_DATABASE_URL") }
            }
        }
        "#,
    )
    .expect_err("top-level settings and workspaces must not be mixed");
    assert!(mixed.message.contains("cannot mix"), "{mixed}");
    assert!(mixed.message.contains("database_url"), "{mixed}");

    let duplicate = compile(
        r#"
        config {
            workspace {
                dev { database = "sqlite" database_url = env("DEV_DATABASE_URL") }
                dev { database = "sqlite" database_url = env("OTHER_DATABASE_URL") }
            }
        }
        "#,
    )
    .expect_err("workspace names must be unique");
    assert!(duplicate.message.contains("Workspace `dev` is declared more than once"), "{duplicate}");

    let literal_url = compile(
        r#"
        config {
            workspace {
                dev { database = "sqlite" database_url = "dev.sqlite" }
            }
        }
        "#,
    )
    .expect_err("workspace database URLs must use env");
    assert!(literal_url.message.contains("config.workspace.dev.database_url"), "{literal_url}");

    let empty = compile("config { workspace {} }").expect_err("an empty workspace block must be rejected");
    assert!(empty.message.contains("workspace") || empty.message.contains("ident"), "{empty}");

    let incomplete = compile(
        r#"
        config {
            workspace {
                dev { with_logger = true }
            }
        }
        "#,
    )
    .expect_err("every workspace must contain a complete database configuration");
    assert!(incomplete.message.contains("config.workspace.dev.database"), "{incomplete}");
}

#[test]
fn compile_supports_logger_and_postgres_direct_pool_settings() {
    let schema = compile(
        r#"
        config {
            database = "postgresql"
            connection = "direct"
            database_url = env("DATABASE_URL")
            with_logger = true
            min_connection = 4
            max_connection = 20
        }
        "#,
    )
    .expect("logger and pool settings should compile");

    let config = schema.config().expect("config");
    assert!(matches!(
        config.entries.iter().find(|entry| entry.key == "with_logger").map(|entry| &entry.value),
        Some(ConfigValue::Boolean(true))
    ));
    assert!(matches!(
        config.entries.iter().find(|entry| entry.key == "min_connection").map(|entry| &entry.value),
        Some(ConfigValue::Integer(4))
    ));

    compile(
        r#"
        config {
            workspace {
                dev {
                    database = "sqlite"
                    database_url = env("DEV_DATABASE_URL")
                    with_logger = true
                }
            }
        }
        "#,
    )
    .expect("with_logger should also work inside a workspace");
}

#[test]
fn compile_rejects_invalid_postgres_pool_settings() {
    let wrong_order = compile(
        r#"
        config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
            min_connection = 11
            max_connection = 10
        }
        "#,
    )
    .expect_err("minimum pool size cannot exceed maximum");
    assert!(wrong_order.message.contains("cannot be greater"), "{wrong_order}");

    let pgbouncer = compile(
        r#"
        config {
            database = "postgresql"
            connection = "pgbouncer"
            database_url = env("DATABASE_URL")
            min_connection = 2
        }
        "#,
    )
    .expect_err("pool settings are direct-only");
    assert!(pgbouncer.message.contains("supported only for PostgreSQL"), "{pgbouncer}");

    let non_boolean = compile(
        r#"
        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
            with_logger = "true"
        }
        "#,
    )
    .expect_err("logger must be a boolean");
    assert!(non_boolean.message.contains("with_logger"), "{non_boolean}");
}

#[test]
fn compile_rejects_literal_database_urls_and_replicas() {
    let database_url = compile(
        r#"
        config {
            database = "postgresql"
            database_url = "postgres://localhost"
        }
        "#,
    )
    .expect_err("literal database_url must be rejected");

    assert!(database_url.message.contains("database_url"));

    let replica = compile(
        r#"
        config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
            read_replicas = ["postgres://replica"]
        }
        "#,
    )
    .expect_err("literal read replica must be rejected");

    assert!(replica.message.contains("read_replicas"));
}

#[test]
fn compile_accepts_relations_enum_defaults_and_snowflake_config() {
    let schema = compile(
        r#"
        config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
            snowflake_node_id = env("SNOWFLAKE_NODE_ID")
        }

        enum Role {
            USER
            ADMIN
        }

        model User {
            id         Integer  @id @default(autoincrement())
            public_id  Integer  @default(snowflake())
            role       Role     @default(USER)

            profile    Profile?
            posts      Post[]
            friends     User[] @relation(name: "Friendships")
            friended_by User[] @relation(name: "Friendships")
        }

        model Profile {
            id       Integer  @id @default(autoincrement())
            user_id  Integer?
            user     User?    @unique @relation(fields: [user_id], references: [id])
        }

        model Post {
            id        Integer  @id @default(autoincrement())
            author_id Integer?
            author    User?    @relation(fields: [author_id], references: [id])
            tags      Tag[]
        }

        model Tag {
            id     Integer @id @default(autoincrement())
            posts  Post[]
        }
        "#,
    )
    .expect("schema with all relation shapes should compile");

    assert_eq!(schema.models().count(), 4);
    let user = schema.models().find(|model| model.name == "User").expect("user");
    assert!(user.fields.iter().any(|field| field.name == "friends" && field.ty.list && field.ty.name == "User"));
}

#[test]
fn compile_rejects_snowflake_without_env_node_id() {
    let err = compile(
        r#"
        config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
        }

        model Item {
            id Integer @id @default(snowflake())
        }
        "#,
    )
    .expect_err("snowflake requires node id env");

    assert!(err.message.contains("snowflake_node_id"));
}

#[test]
fn compile_accepts_explicit_indexes_and_multiple_fulltext_fields() {
    let schema = compile(
        r#"
        model Article {
            id          Integer @id
            slug        String  @index
            publishedAt DateTime @index(map: "idx_article_published")
            title       String  @fulltext
            summary     String? @fulltext
        }
        "#,
    )
    .expect("index attributes should compile");

    let article = schema.models().next().expect("article model");
    assert!(
        article
            .fields
            .iter()
            .find(|field| field.name == "slug")
            .expect("slug")
            .attributes
            .iter()
            .any(|attribute| attribute.name == "index")
    );
    assert_eq!(
        article
            .fields
            .iter()
            .filter(|field| field.attributes.iter().any(|attribute| attribute.name == "fulltext"))
            .count(),
        2
    );
}

#[test]
fn compile_rejects_invalid_index_and_fulltext_declarations() {
    let relation_index = compile(
        r#"
        model User {
            id    Integer @id
            posts Post[]
        }

        model Post {
            id      Integer @id
            user_id Integer
            user    User? @index @relation(fields: [user_id], references: [id])
        }
        "#,
    )
    .expect_err("@index on a relation must be rejected");
    assert!(relation_index.message.contains("indexes must be declared on scalar or enum fields"));

    let non_string = compile(
        r#"
        model Article {
            id    Integer @id
            score Integer @fulltext
        }
        "#,
    )
    .expect_err("@fulltext on a non-String field must be rejected");
    assert!(non_string.message.contains("only supported on String fields"));

    let conflicting = compile(
        r#"
        model Article {
            id    Integer @id
            title String @index @fulltext
        }
        "#,
    )
    .expect_err("@index and @fulltext on one field must be rejected");
    assert!(conflicting.message.contains("cannot combine @index and @fulltext"));
}

#[test]
fn every_model_requires_exactly_one_primary_key_declaration() {
    let missing = compile(
        r#"
        model Account {
            email String
        }
        "#,
    )
    .expect_err("a model without a primary key must fail");
    assert!(missing.message.contains("must declare exactly one primary key"));

    let repeated = compile(
        r#"
        model Account {
            id       String @id
            legacyId String @id
        }
        "#,
    )
    .expect_err("two field primary keys must fail");
    assert!(repeated.message.contains("declares multiple primary keys"));

    let mixed = compile(
        r#"
        model Account {
            tenantId String @id
            id       String

            @@ids([tenantId, id])
        }
        "#,
    )
    .expect_err("@id and @@ids may not be combined");
    assert!(mixed.message.contains("declares multiple primary keys"));

    let composite = compile(
        r#"
        model Account {
            tenantId String
            id       String

            @@ids([tenantId, id])
        }
        "#,
    )
    .expect("one composite primary key is valid");
    let account = composite.models().next().expect("account");
    assert_eq!(account.attribute("ids").and_then(|attribute| attribute.field_names()), Some(vec!["tenantId", "id"]));
}

#[test]
fn compile_supports_composite_unique_index_and_fulltext_declarations() {
    let schema = compile(
        r#"
        model Article {
            tenantId String
            id       String
            title    String
            body     String?
            slug     String
            category String

            @@ids([tenantId, id])
            @@uniques([tenantId, slug])
            @@indexes([tenantId, category])
            @@fulltexts([title, body])
        }
        "#,
    )
    .expect("composite declarations should compile");

    let article = schema.models().next().expect("article");
    assert_eq!(article.attributes("uniques").count(), 1);
    assert_eq!(article.attributes("indexes").count(), 1);
    assert_eq!(
        article.attribute("fulltexts").and_then(|attribute| attribute.field_names()),
        Some(vec!["title", "body"])
    );

    let non_string = compile(
        r#"
        model Article {
            id    String @id
            title String
            score Integer

            @@fulltexts([title, score])
        }
        "#,
    )
    .expect_err("composite full-text fields must all be String");
    assert!(non_string.message.contains("may only contain String fields"));

    let conflicting = compile(
        r#"
        model Article {
            id    String @id
            title String
            body  String

            @@indexes([id, title])
            @@fulltexts([title, body])
        }
        "#,
    )
    .expect_err("standard and full-text indexes may not share a field");
    assert!(conflicting.message.contains("cannot combine @index and @fulltext"));
}

#[test]
fn compile_rejects_model_fields_that_are_reserved_rust_keywords() {
    for keyword in ["type", "match", "self", "async", "gen"] {
        let source = format!(
            r#"
            model KeywordField {{
                id        Integer @id
                {keyword} String
            }}
            "#,
        );
        let error = compile(&source).expect_err("Rust keywords cannot become generated struct fields");

        assert!(error.message.contains(&format!("KeywordField.{keyword}")), "{error}");
        assert!(error.message.contains("reserved Rust keyword"), "{error}");
    }

    compile(
        r#"
        model SafeField {
            id    Integer @id
            _type String
        }
        "#,
    )
    .expect("prefixing a keyword keeps the generated Rust field valid");
}

#[test]
fn compile_accepts_attached_large_schema_when_available() {
    let path = "/Users/theuszastro/.codex/attachments/44beec3f-c803-46ca-8828-c624cf31b68f/pasted-text.txt";
    let Ok(schema_source) = std::fs::read_to_string(path) else {
        return;
    };

    let schema = compile(&schema_source).expect("attached schema should compile");
    assert!(schema.enums().any(|item| item.name == "AudioStatus"));
    assert!(schema.models().any(|model| model.name == "PlaylistTrack"));
}

#[test]
fn compile_file_resolves_recursive_explicit_imports_and_normalized_paths() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::create_dir_all(project.path().join("models")).expect("models directory");
    fs::create_dir_all(project.path().join("shared")).expect("shared directory");
    fs::write(
        &root,
        r#"import { Business } from "./models/../models/business.dinoco"
import { AccountType } from "shared/enums.dinoco"

model Account {
    id   String @id
    kind AccountType
}
"#,
    )
    .expect("root schema");
    fs::write(
        project.path().join("models/business.dinoco"),
        r#"import { BusinessStatus } from "../shared/enums.dinoco"

model Business {
    id     String @id
    status BusinessStatus
}
"#,
    )
    .expect("business schema");
    fs::write(
        project.path().join("shared/enums.dinoco"),
        "enum BusinessStatus { active inactive }\nenum AccountType { owner member }\n",
    )
    .expect("enum schema");

    let schema = compile_file(&root).expect("complete import tree should compile");

    assert!(schema.models().any(|model| model.name == "Account"));
    let business = schema.models().find(|model| model.name == "Business").expect("imported model");
    assert_eq!(business.origin.file, "models/business.dinoco");
    assert_eq!(business.origin.line, 3);
    assert!(schema.enums().any(|item| item.name == "BusinessStatus"));
    assert!(schema.enums().any(|item| item.name == "AccountType"));
}

#[test]
fn compile_file_imports_all_symbols_listed_in_the_main_config() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::create_dir_all(project.path().join("domain")).expect("domain directory");
    fs::create_dir_all(project.path().join("shared")).expect("shared directory");
    fs::write(
        &root,
        r#"config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    imports = ["./domain/models.dinoco", "shared/../shared/enums.dinoco"]
}

model Dashboard {
    id     String @id
    status Status
}
"#,
    )
    .expect("root schema");
    fs::write(
        project.path().join("domain/models.dinoco"),
        r#"import { Status } from "../shared/enums.dinoco"

model Account { id String @id }
model Business { id String @id status Status }
"#,
    )
    .expect("models schema");
    fs::write(project.path().join("shared/enums.dinoco"), "enum Status { active inactive }\n").expect("enum schema");

    let schema = compile_file(&root).expect("config imports should expose every direct declaration to the root");

    assert_eq!(schema.config_imports().count(), 2);
    assert!(schema.models().any(|model| model.name == "Account"));
    assert!(schema.models().any(|model| model.name == "Business"));
    assert!(schema.models().any(|model| model.name == "Dashboard"));
    assert!(schema.enums().any(|item| item.name == "Status"));
}

#[test]
fn config_imports_do_not_leak_the_main_scope_into_child_files() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(
        &root,
        r#"config {
    imports = ["business.dinoco", "status.dinoco"]
}
"#,
    )
    .expect("root schema");
    fs::write(project.path().join("business.dinoco"), "model Business {\n    id String @id\n    status Status\n}\n")
        .expect("business schema");
    fs::write(project.path().join("status.dinoco"), "enum Status { active }\n").expect("status schema");

    let error = compile_file(&root).expect_err("children must keep using named imports");

    assert!(error.message.contains("neither declared nor imported"), "{error}");
    assert_eq!(error.file.as_deref(), Some("business.dinoco"));
    assert_eq!(error.line, 3);
}

#[test]
fn config_imports_support_workspace_configs() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(
        &root,
        r#"config {
    imports = ["models.dinoco"]

    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }
    }
}
"#,
    )
    .expect("root schema");
    fs::write(project.path().join("models.dinoco"), "model Account { id String @id }\n").expect("models schema");

    let schema = compile_file(&root).expect("global imports may accompany workspace database settings");
    let selected = schema.for_workspace("dev").expect("dev workspace");
    assert_eq!(selected.config_imports().count(), 1);
    assert!(selected.models().any(|model| model.name == "Account"));
}

#[test]
fn config_imports_validate_shape_scope_and_duplicate_paths() {
    compile(r#"config { imports = [] }"#).expect("an empty import list is valid");

    for (source, expected) in [
        (r#"config { imports = "models.dinoco" }"#, "must be an array"),
        (r#"config { imports = [models] }"#, "must be a non-empty string"),
        (r#"config { imports = [1] }"#, "must be a non-empty string"),
        (r#"config { imports = [true] }"#, "must be a non-empty string"),
        (r#"config { imports = [env("SCHEMA_FILE")] }"#, "must be a non-empty string"),
        (r#"config { imports = [{}] }"#, "must be a non-empty string"),
        (r#"config { imports = [""] }"#, "cannot be empty"),
        (r#"config { imports = ["models.dinoco", "models.dinoco"] }"#, "listed more than once"),
        (
            r#"config { workspace { dev { imports = ["models.dinoco"] database = "sqlite" database_url = env("URL") } } }"#,
            "must be declared at the top level",
        ),
    ] {
        let error = compile(source).expect_err(expected);
        assert!(error.message.contains(expected), "{error}");
        assert_eq!(error.file.as_deref(), Some("schema.dinoco"));
    }

    let file_mode = compile(r#"config { imports = ["models.dinoco"] }"#)
        .expect_err("valid config imports still require schema.dinoco file resolution");
    assert!(file_mode.message.contains("requires file-based compilation"), "{file_mode}");

    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(&root, r#"config { imports = ["models.dinoco", "./models.dinoco"] }"#).expect("root schema");
    fs::write(project.path().join("models.dinoco"), "model Account { id String @id }\n").expect("models schema");
    let duplicate = compile_file(&root).expect_err("normalized duplicate paths must fail");
    assert!(duplicate.message.contains("imported more than once"), "{duplicate}");
}

#[test]
fn compile_file_keeps_each_files_symbol_scope_explicit() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(&root, "import { Business } from \"business.dinoco\"\nimport { Status } from \"status.dinoco\"\n")
        .expect("root schema");
    fs::write(project.path().join("business.dinoco"), "model Business {\n    id String @id\n    status Status\n}\n")
        .expect("business schema");
    fs::write(project.path().join("status.dinoco"), "enum Status { active }\n").expect("enum schema");

    let error = compile_file(&root).expect_err("an import in the root must not leak into another file");

    assert!(error.message.contains("neither declared nor imported"), "{error}");
    assert_eq!(error.file.as_deref(), Some("business.dinoco"));
    assert_eq!(error.line, 3);

    fs::write(
        &root,
        "import { Business } from \"business.dinoco\"\nmodel Account {\n    id String @id\n    status Status\n}\n",
    )
    .expect("root schema");
    fs::write(
        project.path().join("business.dinoco"),
        "import { Status } from \"status.dinoco\"\nmodel Business { id String @id status Status }\n",
    )
    .expect("business schema");
    let root_error = compile_file(&root).expect_err("transitive imports must not leak into the main file");
    assert!(root_error.message.contains("neither declared nor imported"), "{root_error}");
    assert_eq!(root_error.file.as_deref(), Some("schema.dinoco"));
    assert_eq!(root_error.line, 4);
}

#[test]
fn compile_file_reports_unknown_relations_at_the_original_field() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(&root, "import { Business } from \"business.dinoco\"\n").expect("root schema");
    fs::write(
        project.path().join("business.dinoco"),
        "model Business {\n    id         String @id\n    account_id String\n    account    Account @relation(fields: [account_id], references: [id])\n}\n",
    )
    .expect("business schema");

    let error = compile_file(&root).expect_err("unknown relation model must fail in the declaring file");

    assert_eq!(error.message, "Relation `Business.account` references unknown model `Account`");
    assert_eq!(error.file.as_deref(), Some("business.dinoco"));
    assert_eq!(error.line, 4);
    assert_eq!(error.column, 5);
}

#[test]
fn compile_file_reports_invalid_imports_with_their_origin() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(&root, "import { Missing } from \"shared.dinoco\"\n").expect("root schema");
    fs::write(project.path().join("shared.dinoco"), "enum Present { yes }\n").expect("shared schema");

    let unknown = compile_file(&root).expect_err("unknown imported symbols must fail");
    assert!(unknown.message.contains("Imported symbol `Missing` does not exist"), "{unknown}");
    assert_eq!(unknown.file.as_deref(), Some("schema.dinoco"));
    assert_eq!(unknown.line, 1);

    fs::write(&root, "import { Present } from \"shared.dinoco\"\nimport { Extra } from \"./shared.dinoco\"\n")
        .expect("duplicate imports");
    fs::write(project.path().join("shared.dinoco"), "enum Present { yes }\nenum Extra { yes }\n")
        .expect("shared schema");
    let duplicate = compile_file(&root).expect_err("normalized duplicate file imports must fail");
    assert!(duplicate.message.contains("imported more than once"), "{duplicate}");
    assert_eq!(duplicate.line, 2);

    fs::write(&root, "import { MissingFile } from \"absent.dinoco\"\n").expect("missing import");
    let missing = compile_file(&root).expect_err("missing imported files must fail");
    assert!(missing.message.contains("could not be resolved"), "{missing}");
    assert_eq!(missing.file.as_deref(), Some("schema.dinoco"));
}

#[test]
fn compile_file_supports_circular_imports_for_relations_without_duplicating_models() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(&root, "import { Account } from \"entities/account.dinoco\"\n").expect("root schema");
    fs::create_dir(project.path().join("entities")).expect("entities directory");
    fs::write(
        project.path().join("entities/account.dinoco"),
        r#"import { Session } from "session.dinoco"

model Account {
    id       String    @id
    sessions Session[] @relation(fields: [id], references: [account_id])
}
"#,
    )
    .expect("account schema");
    fs::write(
        project.path().join("entities/session.dinoco"),
        r#"import { Account } from "account.dinoco"

model Session {
    id         String  @id
    account_id String
    account    Account? @relation(fields: [account_id], references: [id])
}
"#,
    )
    .expect("session schema");

    let schema = compile_file(&root).expect("circular relation imports should compile");
    let model_names = schema.models().map(|model| model.name.as_str()).collect::<Vec<_>>();

    assert_eq!(model_names.len(), 2);
    assert!(model_names.contains(&"Account"));
    assert!(model_names.contains(&"Session"));
}

#[test]
fn compile_file_rejects_config_outside_the_main_schema() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(&root, "import { A } from \"a.dinoco\"\n").expect("root schema");
    fs::write(project.path().join("a.dinoco"), "config { database = \"sqlite\" }\nmodel A { id String @id }\n")
        .expect("imported config");
    let config = compile_file(&root).expect_err("only the main schema may configure the project");
    assert!(config.message.contains("Only `schema.dinoco` may declare a `config` block"), "{config}");
    assert_eq!(config.file.as_deref(), Some("a.dinoco"));
}

#[test]
fn compile_file_reports_both_origins_for_global_duplicate_symbols() {
    let project = tempdir().expect("temp project");
    let root = project.path().join("schema.dinoco");
    fs::write(&root, "import { MarkerA } from \"a.dinoco\"\nimport { MarkerB } from \"b.dinoco\"\n")
        .expect("root schema");
    fs::write(project.path().join("a.dinoco"), "model User { id String @id }\nenum MarkerA { yes }\n")
        .expect("a schema");
    fs::write(project.path().join("b.dinoco"), "\nmodel User { id String @id }\nenum MarkerB { yes }\n")
        .expect("b schema");

    let error = compile_file(&root).expect_err("global duplicate declarations must fail");

    assert!(error.message.contains("Symbol `User` is declared more than once"), "{error}");
    assert_eq!(error.file.as_deref(), Some("b.dinoco"));
    assert_eq!(error.line, 2);
    assert_eq!(error.related.len(), 1);
    assert_eq!(error.related[0].file, "a.dinoco");
    assert_eq!(error.related[0].line, 1);
}

#[test]
fn compile_file_only_accepts_schema_dinoco_as_the_entrypoint() {
    let project = tempdir().expect("temp project");
    let path = project.path().join("main.dinoco");
    fs::write(&path, "model User { id String @id }\n").expect("schema");

    let error = compile_file(&path).expect_err("the entrypoint name is fixed");
    assert!(error.message.contains("must be named `schema.dinoco`"), "{error}");
}

#[test]
fn parser_preserves_origins_for_imports_enums_models_fields_relations_and_custom_derives() {
    let schema = parse(
        r#"import { Account } from "account.dinoco"
config {
    custom_derives = [{ into = "struct" derive = "Validate" import = "use validator::Validate;" }]
}
enum Status { active }
model Business {
    id      String  @id
    account Account? @relation(fields: [id], references: [id])
}
"#,
    )
    .expect("syntax and custom derive config should parse");

    let import = schema.imports().next().expect("import");
    assert_eq!((import.origin.file.as_str(), import.origin.line, import.origin.column), ("schema.dinoco", 1, 1));
    let custom = schema.custom_derives().next().expect("custom derive");
    assert_eq!((custom.origin.file.as_str(), custom.origin.line), ("schema.dinoco", 3));
    let status = schema.enums().next().expect("enum");
    assert_eq!((status.origin.file.as_str(), status.origin.line), ("schema.dinoco", 5));
    let business = schema.models().next().expect("model");
    assert_eq!((business.origin.file.as_str(), business.origin.line), ("schema.dinoco", 6));
    let account = business.fields.iter().find(|field| field.name == "account").expect("relation field");
    assert_eq!((account.origin.file.as_str(), account.origin.line, account.origin.column), ("schema.dinoco", 8, 5));
    let relation = account.attributes.iter().find(|attribute| attribute.name == "relation").expect("relation");
    assert_eq!((relation.origin.file.as_str(), relation.origin.line), ("schema.dinoco", 8));
}

#[test]
fn compile_validates_custom_derive_objects_with_source_locations() {
    let invalid_into = compile(
        r#"config {
    custom_derives = [
        { into = "table" derive = "Validate" import = "use validator::Validate;" }
    ]
}
"#,
    )
    .expect_err("custom derive targets are restricted");
    assert!(invalid_into.message.contains("must be `enum` or `struct`"), "{invalid_into}");
    assert_eq!(invalid_into.file.as_deref(), Some("schema.dinoco"));
    assert_eq!(invalid_into.line, 3);

    for (source, expected) in [
        (r#"config { custom_derives = [{}] }"#, "missing `into`, `derive`, `import`"),
        (r#"config { custom_derives = [{ derive = "X" import = "use x::X;" }] }"#, "missing `into`"),
        (r#"config { custom_derives = [{ into = "enum" import = "use x::X;" }] }"#, "missing `derive`"),
        (r#"config { custom_derives = [{ into = "enum" derive = "X" }] }"#, "missing `import`"),
        (r#"config { custom_derives = [{ into = "enum" }] }"#, "missing `derive`, `import`"),
        (
            r#"config { custom_derives = [{ into = "enum" derive = "X" derive = "Y" import = "use x::X;" }] }"#,
            "declared more than once",
        ),
        (r#"config { custom_derives = ["X"] }"#, "must be an object"),
        (
            r#"config { custom_derives = [{ into = "" derive = "X" import = "use x::X;" }] }"#,
            "`into` must be a non-empty string",
        ),
        (
            r#"config { custom_derives = [{ into = "enum" derive = "" import = "use x::X;" }] }"#,
            "`derive` must be a non-empty string",
        ),
        (
            r#"config { custom_derives = [{ into = "enum" derive = "X" import = "" }] }"#,
            "`import` must be a non-empty string",
        ),
        (
            r#"config { custom_derives = [{ into = "enum" derive = "not a path" import = "use x::X;" }] }"#,
            "valid Rust derive path",
        ),
        (r#"config { custom_derives = [{ into = "enum" derive = "X" import = "x::X" }] }"#, "Rust `use ...` statement"),
    ] {
        let error = compile(source).expect_err(expected);
        assert!(error.message.contains(expected), "{error}");
    }
}
