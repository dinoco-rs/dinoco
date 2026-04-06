use dinoco_formatter::{FormatterConfig, format_from_ast, format_from_raw};
use dinoco_compiler::compile_only_ast;

#[test]
fn format_from_raw_returns_canonical_schema() {
    let raw = r#"
model User{
id Integer @id @default(autoincrement())
email String @unique
}
"#;

    let formatted = format_from_raw(raw).expect("formatter should succeed");

    assert!(formatted.contains("model User {"));
    assert!(formatted.contains("id"));
    assert!(formatted.contains("Integer"));
    assert!(formatted.contains("@default(autoincrement())"));
    assert!(formatted.contains("email"));
    assert!(formatted.contains("@unique"));
    assert!(formatted.ends_with('\n'));
}

#[test]
fn format_from_ast_is_idempotent() {
    let raw = r#"
config {
  database = "sqlite"
  database_url = env("DATABASE_URL")
}
"#;
    let schema = compile_only_ast(raw).expect("schema should parse");
    let config = FormatterConfig::default();

    let once = format_from_ast(&schema, &config);
    let reparsed = compile_only_ast(&once).expect("formatted schema should parse");
    let twice = format_from_ast(&reparsed, &config);

    assert_eq!(once, twice);
}
