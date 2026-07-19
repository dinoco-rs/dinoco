#[test]
fn compiler_rejects_literal_database_url() {
    let err = dinoco_compiler::compile(
        r#"
        config {
            database = "postgresql"
            database_url = "postgres://localhost"
        }
        "#,
    )
    .expect_err("literal database_url must fail");

    assert!(err.message.contains("database_url"));
}

#[test]
fn compiler_formatter_and_codegen_handle_enums() {
    let source = r#"
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
        }
    "#;

    let schema = dinoco_compiler::compile(source).expect("schema compiles");
    let formatted = dinoco_formatter::format_schema(&schema, &dinoco_formatter::FormatterConfig::default());
    let models = dinoco_codegen::render_models(&schema);

    assert!(formatted.contains("read_replicas = [\n"));
    assert!(models.contains("pub enum OfficeType"));
    assert!(models.contains("Admin,"));
    assert!(models.contains("Member,"));
    assert!(models.contains("pub office: OfficeType"));
}
