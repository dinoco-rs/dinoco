use dinoco_compiler::{ConfigValue, compile};

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
            user    User @index @relation(fields: [user_id], references: [id])
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
fn compile_accepts_attached_large_schema_when_available() {
    let path = "/Users/theuszastro/.codex/attachments/44beec3f-c803-46ca-8828-c624cf31b68f/pasted-text.txt";
    let Ok(schema_source) = std::fs::read_to_string(path) else {
        return;
    };

    let schema = compile(&schema_source).expect("attached schema should compile");
    assert!(schema.enums().any(|item| item.name == "AudioStatus"));
    assert!(schema.models().any(|model| model.name == "PlaylistTrack"));
}
