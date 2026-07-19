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
            friends    User[]
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
fn compile_accepts_attached_large_schema_when_available() {
    let path = "/Users/theuszastro/.codex/attachments/44beec3f-c803-46ca-8828-c624cf31b68f/pasted-text.txt";
    let Ok(schema_source) = std::fs::read_to_string(path) else {
        return;
    };

    let schema = compile(&schema_source).expect("attached schema should compile");
    assert!(schema.enums().any(|item| item.name == "AudioStatus"));
    assert!(schema.models().any(|model| model.name == "PlaylistTrack"));
}
