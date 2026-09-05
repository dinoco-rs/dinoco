mod comments;
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
    /// Indent using a single tab per level instead of `indent_width` spaces.
    pub use_tabs: bool,
    /// Maximum preferred line width before wrappable constructs (imports,
    /// attributes with named arguments) are broken into multiple lines.
    pub max_width: usize,
    /// Drop all `#`/`//` comments instead of reattaching them to the
    /// formatted output.
    pub strip_comments: bool,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self { indent_width: 4, final_newline: true, use_tabs: false, max_width: 100, strip_comments: false }
    }
}

impl FormatterConfig {
    pub fn indent(&self) -> String {
        if self.use_tabs { "\t".to_string() } else { " ".repeat(self.indent_width) }
    }
}

pub fn format_from_raw(source: &str) -> Result<String, CompileError> {
    format_from_raw_with_config(source, &FormatterConfig::default())
}

pub fn format_from_raw_with_config(source: &str, config: &FormatterConfig) -> Result<String, CompileError> {
    let parsed = dinoco_compiler::parse(source)?;
    let schema = if parsed.imports().next().is_some() || parsed.config_imports().next().is_some() {
        parsed
    } else {
        dinoco_compiler::compile(source)?
    };
    let layout = LayoutHints::from_source(source, &schema);
    let formatted = format_schema_with_layout(&schema, config, &layout);

    Ok(if config.strip_comments { formatted } else { comments::restore(source, &formatted) })
}

/// Formats an imported schema document without requiring the main project's
/// database configuration to be present in the fragment.
pub fn format_fragment_from_raw_with_config(source: &str, config: &FormatterConfig) -> Result<String, CompileError> {
    let schema = dinoco_compiler::parse(source)?;
    let layout = LayoutHints::from_source(source, &schema);
    let formatted = format_schema_with_layout(&schema, config, &layout);

    Ok(if config.strip_comments { formatted } else { comments::restore(source, &formatted) })
}

pub fn format_schema(schema: &Schema, config: &FormatterConfig) -> String {
    let mut blocks = Vec::new();

    for item in &schema.items {
        blocks.push((
            matches!(item, SchemaItem::Import(_)),
            match item {
                SchemaItem::Import(import) => format_import(import, config),
                SchemaItem::Config(config_block) => format_config(config_block, config),
                SchemaItem::Enum(enum_def) => format_enum(enum_def, config),
                SchemaItem::Model(model) => format_model(model, config),
            },
        ));
    }

    let mut output = join_blocks(blocks);

    if config.final_newline {
        output.push('\n');
    }

    output
}

fn format_schema_with_layout(schema: &Schema, config: &FormatterConfig, layout: &LayoutHints) -> String {
    let mut blocks = Vec::new();

    for item in &schema.items {
        blocks.push((
            matches!(item, SchemaItem::Import(_)),
            match item {
                SchemaItem::Import(import) => format_import(import, config),
                SchemaItem::Config(config_block) => format_config(config_block, config),
                SchemaItem::Enum(enum_def) => format_enum(enum_def, config),
                SchemaItem::Model(model) => {
                    format_model_with_layout(model, config, layout.model_blank_lines(&model.name))
                }
            },
        ));
    }

    let mut output = join_blocks(blocks);
    if config.final_newline {
        output.push('\n');
    }
    output
}

fn format_import(import: &dinoco_compiler::Import, config: &FormatterConfig) -> String {
    let path = import.path.replace('\\', "\\\\").replace('"', "\\\"");
    let single_line = format!("import {{ {} }} from \"{path}\"", import.symbols.join(", "));
    if single_line.len() <= config.max_width {
        return single_line;
    }

    let indent = config.indent();
    let mut output = String::from("import {\n");
    for symbol in &import.symbols {
        output.push_str(&indent);
        output.push_str(symbol);
        output.push_str(",\n");
    }
    output.push_str(&format!("}} from \"{path}\""));
    output
}

fn join_blocks(blocks: Vec<(bool, String)>) -> String {
    let mut output = String::new();
    let mut previous_import = false;
    for (index, (is_import, block)) in blocks.into_iter().enumerate() {
        if index > 0 {
            output.push_str(if previous_import && is_import { "\n" } else { "\n\n" });
        }
        output.push_str(&block);
        previous_import = is_import;
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
    fn formats_imports_and_custom_derive_objects_without_resolving_files() {
        let raw = r#"import{BusinessStatus,AccountType}from"./shared/enums.dinoco"config{imports=["models.dinoco","enums.dinoco"] custom_derives=[{into="enum" derive="ZodSchema" import="use zod_rs::prelude::*"}]}"#;

        let formatted = format_from_raw(raw).expect("format");

        assert!(formatted.contains("import { BusinessStatus, AccountType } from \"./shared/enums.dinoco\""));
        assert!(formatted.contains("imports        = ["));
        assert!(formatted.contains("\"models.dinoco\","));
        assert!(formatted.contains("custom_derives = ["));
        assert!(formatted.contains("into = \"enum\""));
        assert!(formatted.contains("derive = \"ZodSchema\""));
        assert!(formatted.contains("import = \"use zod_rs::prelude::*\""));
    }

    #[test]
    fn formats_imported_snowflake_models_without_project_config() {
        let raw = "model Account{id Integer @id @default(snowflake())}";

        let formatted =
            format_fragment_from_raw_with_config(raw, &FormatterConfig::default()).expect("format fragment");

        assert!(formatted.contains("@default(snowflake())"));
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
