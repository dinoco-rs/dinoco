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
            email   String @fulltext
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
    assert!(models.contains("#[dinoco(value = \"admin\")]\n    #[serde(rename = \"admin\")]\n    Admin,"));
    assert!(models.contains("#[dinoco(value = \"member\")]\n    #[serde(rename = \"member\")]\n    Member,"));
    assert!(models.contains("Admin,"));
    assert!(
        models.contains("#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]")
    );
    assert!(models.contains("#[serde(crate = \"::dinoco::serde\")]\n#[dinoco(table_name = \"user\")]"));
    assert!(models.contains("pub struct User"));
    assert!(models.contains("#[dinoco(primary_key, auto_generate = uuid)]"));
    assert!(models.contains("#[dinoco(fulltext)]\n    pub email: String"));
    assert!(models.contains("pub tokens: Vec<UserToken>"));
    assert!(models.contains("pub user: Option<User>"));
    assert!(dinoco_mod.contains("pub mod models;"));
    assert!(dinoco_mod.contains("pub const MIGRATIONS:"));
    assert!(dinoco_mod.contains("pub async fn migrate("));
    let connect = dinoco_mod.split("pub const MIGRATIONS:").next().expect("connect section");
    assert!(!connect.contains("run_migrations"), "connect must not apply migrations automatically");
    assert!(dinoco_mod.contains("pub async fn connect()"));
    assert!(dinoco_mod.contains("PostgresAdapter::direct_with_pool(database_url, 2, 10)"));
    assert!(dinoco_mod.contains("with_read_replicas(read_replicas)"));
    assert!(dinoco_mod.contains("with_logger(false)"));
}

#[test]
fn codegen_keeps_model_acronyms_together_in_generated_names() {
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

    let models = dinoco_codegen::render_models(&schema);

    assert!(models.contains("mod business_cnae;"));
    assert!(models.contains("#[dinoco(table_name = \"business_cnae\")]"));
    assert!(models.contains("mod business_office;"));
    assert!(models.contains("#[dinoco(table_name = \"business_office\")]"));
    assert!(!models.contains("business_c_n_a_e"));
}

#[test]
fn codegen_applies_logger_and_custom_postgres_pool_settings() {
    let schema = dinoco_compiler::compile(
        r#"
        config {
            database = "postgresql"
            connection = "direct"
            database_url = env("DATABASE_URL")
            with_logger = true
            min_connection = 4
            max_connection = 24
            read_replicas = [env("DATABASE_REPLICA_1"), env("DATABASE_REPLICA_2")]
        }
        "#,
    )
    .expect("schema");

    let dinoco_mod = dinoco_codegen::render_dinoco_mod(&schema);
    assert!(dinoco_mod.contains("PostgresAdapter::direct_with_pool(database_url, 4, 24)"));
    assert!(dinoco_mod.contains("PostgresAdapter::direct_with_pool(std::env::var(\"DATABASE_REPLICA_1\")?, 4, 24)"));
    assert!(dinoco_mod.contains("PostgresAdapter::direct_with_pool(std::env::var(\"DATABASE_REPLICA_2\")?, 4, 24)"));
    assert!(dinoco_mod.contains("with_read_replicas(read_replicas).with_logger(true)"));
    assert!(dinoco_mod.contains("with_logger(true)"));
}

#[test]
fn codegen_preserves_composite_primary_and_fulltext_capabilities() {
    let schema = dinoco_compiler::compile(
        r#"
        model Article {
            tenantId String
            id       String
            title    String
            body     String?

            @@ids([tenantId, id])
            @@fulltexts([title, body])
            @@table_name("search_articles")
        }
        "#,
    )
    .expect("composite schema");

    let models = dinoco_codegen::render_models(&schema);
    assert!(models.contains("#[dinoco(table_name = \"search_articles\")]"));
    assert!(models.contains("#[dinoco(primary_key)]\n    pub tenantId: String"));
    assert!(models.contains("#[dinoco(primary_key)]\n    pub id: String"));
    assert!(models.contains("#[dinoco(fulltext = \"title,body\")]\n    pub title: String"));
    assert!(models.contains("#[dinoco(fulltext = \"title,body\")]\n    pub body: Option<String>"));
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
            birthday   Date
            metadata   Json
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
    assert!(models.contains(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize, ::dinoco::DinocoEnum)]"
    ));
    assert!(models.contains("#[serde(crate = \"::dinoco::serde\")]"));
    assert!(models.contains("#[dinoco(value = \"USER\")]"));
    assert!(models.contains("#[default]\n    #[dinoco(value = \"USER\")]\n    #[serde(rename = \"USER\")]\n    USER,"));
    assert!(models.contains("pub createdAt: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>"));
    assert!(models.contains("pub birthday: ::dinoco::chrono::NaiveDate"));
    assert!(models.contains("pub metadata: ::dinoco::serde_json::Value"));
    assert!(models.contains("#[dinoco(default = Role::USER)]"));
    assert!(models.contains("#[dinoco(many_to_many, join_table = \"_post_to_user\""));
    assert!(models.contains("#[dinoco(many_to_many_key, join_table = \"_post_to_user\""));
    assert!(models.contains("pub post_id: Option<::dinoco::Uuid>"));
    assert!(models.contains("pub user_id: Option<::dinoco::Uuid>"));
    assert!(!models.contains("pub struct PostUser"));
    assert!(!models.contains("dinoco_engine"));

    let dinoco_mod = dinoco_codegen::render_dinoco_mod(&schema);
    assert!(dinoco_mod.contains("::dinoco::DinocoClient"));
    assert!(dinoco_mod.contains("::dinoco::anyhow::Result"));
    assert!(!dinoco_mod.contains("dinoco_engine"));
}

#[test]
fn codegen_preserves_uuid_and_snowflake_types_on_relation_keys() {
    let schema = dinoco_compiler::compile(
        r#"
        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
            snowflake_node_id = env("SNOWFLAKE_NODE_ID")
        }

        model Organization {
            id      String   @id @default(uuid())
            members Member[]
        }

        model Member {
            id              String        @id @default(uuid())
            organization_id String?
            organization    Organization? @relation(fields: [organization_id], references: [id])
        }

        model Account {
            id       Integer   @id @default(snowflake())
            sessions Session[]
            systems  System[]
        }

        model Session {
            id         Integer  @id @default(snowflake())
            account_id Integer
            account    Account  @relation(fields: [account_id], references: [id])
        }

        model System {
            id       Integer   @id @default(snowflake())
            accounts Account[]
        }
        "#,
    )
    .expect("schema");

    let models = dinoco_codegen::render_models(&schema);

    assert!(models.contains("pub organization_id: Option<::dinoco::Uuid>"));
    assert!(models.contains("pub account_id: ::dinoco::Snowflake"));
    assert!(!models.contains("pub struct AccountSystem"));
    assert!(models.contains("pub account_id: Option<::dinoco::Snowflake>"));
    assert!(models.contains("pub system_id: Option<::dinoco::Snowflake>"));
}

#[test]
fn codegen_infers_one_to_many_from_the_owning_relation_side() {
    let schema = dinoco_compiler::compile(
        r#"
        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
        }

        model Device {
            id      String        @id @default(uuid())
            unlocks OrixaUnlock[]
        }

        model OrixaUnlock {
            id        String  @id @default(uuid())
            device_id String?
            device    Device? @relation(fields: [device_id], references: [id])
        }
        "#,
    )
    .expect("schema");

    let device = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "Device").expect("device model"),
        &schema,
    );
    let unlock = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "OrixaUnlock").expect("unlock model"),
        &schema,
    );

    assert!(device.contains("#[dinoco(one_to_many, foreign_key = \"device_id\", references = \"id\")]"));
    assert!(!device.contains("#[dinoco(many_to_many)]"));
    assert!(unlock.contains("#[dinoco(many_to_one, foreign_key = \"device_id\", references = \"id\")]"));
}

#[test]
fn codegen_reverses_explicit_list_relation_keys_for_the_derive() {
    let schema = dinoco_compiler::compile(
        r#"
        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
        }

        model User {
            id     String      @id @default(uuid())
            tokens UserToken[] @relation(fields: [id], references: [user_id])
        }

        model UserToken {
            id      String @id @default(uuid())
            user_id String?
            user    User?  @relation(fields: [user_id], references: [id])
        }
        "#,
    )
    .expect("schema");

    let user = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "User").expect("user model"),
        &schema,
    );

    assert!(user.contains("#[dinoco(one_to_many, foreign_key = \"user_id\", references = \"id\")]"));
}

#[test]
fn codegen_preserves_multiple_named_relations_to_the_same_model() {
    let schema = dinoco_compiler::compile(
        r#"
        model Business {
            id                   Integer           @id @default(snowflake())
            analyses             BusinessAnalyse[] @relation(name: "owned")
            registration_changes BusinessAnalyse[] @relation(name: "changes")
        }

        model BusinessAnalyse {
            id                  Integer   @id @default(snowflake())
            _type               String
            owned_business_id   Integer?
            owned_business      Business? @relation(name: "owned", fields: [owned_business_id], references: [id])
            changes_business_id Integer?
            changes_business    Business? @relation(name: "changes", fields: [changes_business_id], references: [id])
        }

        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
            snowflake_node_id = env("SNOWFLAKE_NODE_ID")
        }
        "#,
    )
    .expect("named relations to the same model must compile");

    let business = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "Business").expect("business model"),
        &schema,
    );
    let analyse = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "BusinessAnalyse").expect("analysis model"),
        &schema,
    );

    assert!(business.contains(
        "#[dinoco(one_to_many, relation_name = \"owned\", foreign_key = \"owned_business_id\", references = \"id\")]"
    ));
    assert!(business.contains(
        "#[dinoco(one_to_many, relation_name = \"changes\", foreign_key = \"changes_business_id\", references = \"id\")]"
    ));
    assert!(analyse.contains(
        "#[dinoco(many_to_one, relation_name = \"owned\", foreign_key = \"owned_business_id\", references = \"id\")]"
    ));
    assert!(analyse.contains(
        "#[dinoco(many_to_one, relation_name = \"changes\", foreign_key = \"changes_business_id\", references = \"id\")]"
    ));
    assert!(analyse.contains("pub _type: String"));
}

#[test]
fn codegen_generates_copy_only_for_models_whose_fields_are_copy() {
    let schema = dinoco_compiler::compile(
        r#"
        enum BeaconMode {
            steady
            pulse
        }

        model BeaconReading {
            id          Integer    @id
            mode        BeaconMode
            acknowledged Boolean
            sampled_at  DateTime
        }

        model BeaconLabel {
            id    Integer @id
            label String
        }

        model BeaconUuid {
            id String @id @default(uuid())
        }
        "#,
    )
    .expect("copy derive schema");

    let enums_and_modules = dinoco_codegen::render_models_mod(&schema);
    let reading = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "BeaconReading").expect("reading model"),
        &schema,
    );
    let label = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "BeaconLabel").expect("label model"),
        &schema,
    );
    let uuid = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "BeaconUuid").expect("uuid model"),
        &schema,
    );

    assert!(enums_and_modules.contains(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize, ::dinoco::DinocoEnum)]"
    ));
    assert!(
        reading.contains(
            "#[derive(Debug, Clone, Copy, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]"
        )
    );
    assert!(
        label.contains("#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]")
    );
    assert!(!label.contains("Debug, Clone, Copy, Entity"));
    assert!(uuid.contains("#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]"));
    assert!(!uuid.contains("Debug, Clone, Copy, Entity"));
}

#[test]
fn codegen_keeps_named_inverse_relations_bound_to_their_distinct_foreign_keys() {
    let schema = dinoco_compiler::compile(
        r#"
        model GalleryMember {
            id             Integer       @id
            collected     ArtworkLoan[] @relation(name: "collector")
            authenticated ArtworkLoan[] @relation(name: "curator")
        }

        model ArtworkLoan {
            id           Integer        @id
            collector_id Integer?
            collector    GalleryMember? @relation(name: "collector", fields: [collector_id], references: [id])
            curator_id   Integer?
            curator      GalleryMember? @relation(name: "curator", fields: [curator_id], references: [id])
        }
        "#,
    )
    .expect("named inverse relation schema");

    let member = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "GalleryMember").expect("member model"),
        &schema,
    );
    let loan = dinoco_codegen::render_model_file(
        schema.models().find(|model| model.name == "ArtworkLoan").expect("loan model"),
        &schema,
    );

    assert!(member.contains(
        "#[dinoco(one_to_many, relation_name = \"collector\", foreign_key = \"collector_id\", references = \"id\")]"
    ));
    assert!(member.contains(
        "#[dinoco(one_to_many, relation_name = \"curator\", foreign_key = \"curator_id\", references = \"id\")]"
    ));
    assert!(loan.contains(
        "#[dinoco(many_to_one, relation_name = \"collector\", foreign_key = \"collector_id\", references = \"id\")]"
    ));
    assert!(loan.contains(
        "#[dinoco(many_to_one, relation_name = \"curator\", foreign_key = \"curator_id\", references = \"id\")]"
    ));
}

#[test]
fn codegen_qualifies_try_from_error_when_enum_has_error_variant() {
    let schema = dinoco_compiler::compile(
        r#"
        config {
            database = "mysql"
            database_url = env("DATABASE_URL")
        }

        enum AudioStatus {
            generated
            building
            error
        }
        "#,
    )
    .expect("schema");

    let models = dinoco_codegen::render_models_mod(&schema);

    assert!(models.contains("Error,"));
    assert!(models.contains("::dinoco::DinocoEnum"));
    assert!(models.contains("#[dinoco(value = \"error\")]"));
    assert!(!models.contains("impl ::core::convert::TryFrom<::dinoco::mysql_async::Value>"));
}
