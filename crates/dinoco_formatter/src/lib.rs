mod config;
mod layout;
mod model;
mod utils;
mod value;

use dinoco_compiler::{CompileError, Schema, SchemaItem};

use crate::config::format_config;
use crate::layout::LayoutHints;
use crate::model::{format_enum, format_model, format_model_with_layout};

#[derive(Debug, Clone)]
pub struct FormatterConfig {
    pub indent_width: usize,
    pub final_newline: bool,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self { indent_width: 4, final_newline: true }
    }
}

impl FormatterConfig {
    pub fn indent(&self) -> String {
        " ".repeat(self.indent_width)
    }
}

pub fn format_from_raw(source: &str) -> Result<String, CompileError> {
    format_from_raw_with_config(source, &FormatterConfig::default())
}

pub fn format_from_raw_with_config(source: &str, config: &FormatterConfig) -> Result<String, CompileError> {
    let schema = dinoco_compiler::compile(source)?;
    let layout = LayoutHints::from_source(source, &schema);

    Ok(format_schema_with_layout(&schema, config, &layout))
}

pub fn format_schema(schema: &Schema, config: &FormatterConfig) -> String {
    let mut blocks = Vec::new();

    for item in &schema.items {
        blocks.push(match item {
            SchemaItem::Config(config_block) => format_config(config_block, config),
            SchemaItem::Enum(enum_def) => format_enum(enum_def, config),
            SchemaItem::Model(model) => format_model(model, config),
        });
    }

    let mut output = blocks.join("\n\n");

    if config.final_newline {
        output.push('\n');
    }

    output
}

fn format_schema_with_layout(schema: &Schema, config: &FormatterConfig, layout: &LayoutHints) -> String {
    let mut blocks = Vec::new();

    for item in &schema.items {
        blocks.push(match item {
            SchemaItem::Config(config_block) => format_config(config_block, config),
            SchemaItem::Enum(enum_def) => format_enum(enum_def, config),
            SchemaItem::Model(model) => format_model_with_layout(model, config, layout.model_blank_lines(&model.name)),
        });
    }

    let mut output = blocks.join("\n\n");
    if config.final_newline {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_schema() {
        let raw = r#"config{database="postgresql" database_url=env("DATABASE_URL") read_replicas=[env("A"),env("B")]}enum Status{Active Pending}model User{id String @id @default(uuid())email String tokens Token[]}model Token{id String @id @default(uuid())user User? @relation(fields:[user_id],references:[id],onDelete:Cascade)user_id String?}"#;

        let formatted = format_from_raw(raw).expect("format");

        assert!(formatted.contains("config {\n"));
        assert!(formatted.contains("read_replicas = [\n"));
        assert!(formatted.contains("enum Status {\n"));
        assert!(formatted.contains("model User {\n"));
        assert!(formatted.contains("@relation(fields: [user_id], references: [id], onDelete: Cascade)"));
    }

    #[test]
    fn preserves_single_model_field_group_separator() {
        let raw = r#"model AdminAccount {
    id String @id @default(uuid())
    email String
    password String

    tokens AdminToken[]
}

model AdminToken {
    id         String @id
    account_id String?
    account    AdminAccount? @relation(fields: [account_id], references: [id])
}"#;

        let formatted = format_from_raw(raw).expect("format");

        assert!(formatted.contains("password  String\n\n    tokens"));
    }

    #[test]
    fn collapses_multiple_model_field_group_separators() {
        let raw = r#"model AdminAccount {
    id String @id @default(uuid())
    email String
    password String



    tokens AdminToken[]
}

model AdminToken {
    id         String @id
    account_id String?
    account    AdminAccount? @relation(fields: [account_id], references: [id])
}"#;

        let formatted = format_from_raw(raw).expect("format");

        assert!(formatted.contains("password  String\n\n    tokens"));
        assert!(!formatted.contains("password  String\n\n\n"));
    }

    #[test]
    fn does_not_add_model_field_group_separator() {
        let raw = r#"model AdminAccount {
    id String @id @default(uuid())
    email String
    password String
    tokens AdminToken[]
}

model AdminToken {
    id         String @id
    account_id String?
    account    AdminAccount? @relation(fields: [account_id], references: [id])
}"#;

        let formatted = format_from_raw(raw).expect("format");

        assert!(formatted.contains("password  String\n    tokens"));
        assert!(!formatted.contains("password  String\n\n    tokens"));
    }
}
