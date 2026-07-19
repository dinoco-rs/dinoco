#[test]
fn codegen_generates_entities_enums_defaults_and_relations() {
    let schema = dinoco_compiler::compile(
        r#"
        config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
        }

        enum OfficeType {
            admin
            member
        }

        model User {
            id      String      @id @default(uuid())
            email   String
            office  OfficeType
            tokens  UserToken[] @relation(fields: [id], references: [userId])
        }

        model UserToken {
            id         String   @id @default(uuid())
            isExpired  Boolean  @default(false)
            userId     String?
            user       User?    @relation(fields: [userId], references: [id])
        }
        "#,
    )
    .expect("schema");

    let models = dinoco_codegen::render_models(&schema);
    let dinoco_mod = dinoco_codegen::render_dinoco_mod(&schema);

    assert!(models.contains("pub enum OfficeType"));
    assert!(models.contains("Admin,"));
    assert!(models.contains("#[derive(Debug, Entity)]"));
    assert!(models.contains("pub struct User"));
    assert!(models.contains("#[dinoco(primary_key, auto_generate = uuid)]"));
    assert!(models.contains("pub tokens: Vec<UserToken>"));
    assert!(models.contains("pub user: Option<User>"));
    assert!(dinoco_mod.contains("pub mod models;"));
    assert!(dinoco_mod.contains("pub async fn connect()"));
    assert!(dinoco_mod.contains("PostgresAdapter::direct"));
}

#[test]
fn codegen_respects_uuid_snowflake_enum_defaults_and_implicit_relations() {
    let schema = dinoco_compiler::compile(
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
            id         String    @id @default(uuid())
            sequence   Integer   @default(snowflake())
            role       Role      @default(USER)
            createdAt  DateTime  @default(now())
            posts      Post[]
        }

        model Post {
            id     String @id @default(uuid())
            users  User[]
        }
        "#,
    )
    .expect("schema");

    let models = dinoco_codegen::render_models(&schema);

    assert!(models.contains("pub id: ::dinoco::Uuid"));
    assert!(models.contains("pub sequence: ::dinoco::Snowflake"));
    assert!(models.contains("pub createdAt: ::dinoco_engine::chrono::DateTime<::dinoco_engine::chrono::Utc>"));
    assert!(models.contains("#[dinoco(default = Role::USER)]"));
    assert!(models.contains("#[dinoco(many_to_many)]"));
}
