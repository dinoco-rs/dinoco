pub mod _enum;
pub mod config;
pub mod table;
pub mod utils;

use crate::_enum::*;
use crate::config::*;
use crate::table::*;

use dinoco_compiler::{Schema, compile_only_ast};

#[derive(Debug, Clone)]
pub struct FormatterConfig {
    pub ident_width: usize,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self { ident_width: 4 }
    }
}

impl FormatterConfig {
    #[inline]
    pub fn indent_str(&self) -> String {
        " ".repeat(self.ident_width)
    }
}

pub fn format(file_path: &str, config: FormatterConfig) -> bool {
    let raw_input = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("Failed to read file: {err}");
            return false;
        }
    };

    let schema = match compile_only_ast(&raw_input) {
        Ok(result) => result,
        Err(errs) => {
            eprintln!("Compile error:\n{:#?}", errs);

            return false;
        }
    };

    let result = format_schema(&schema, &config);

    if let Err(err) = std::fs::write(file_path, result) {
        eprintln!("Failed to write file: {err}");
        return false;
    }

    true
}

pub fn format_schema(schema: &Schema, config: &FormatterConfig) -> String {
    let mut out = String::new();

    for position in 0..schema.total_blocks {
        if let Some((_, comment)) = schema.comments.iter().find(|x| x.0 == position) {
            let clean = comment.replace('#', "");

            out.push_str("# ");
            out.push_str(clean.trim());
            out.push('\n');
        }

        if let Some(table) = schema.tables.iter().find(|x| x.position == position) {
            out.push_str(&format_table(table, config));
        }

        if let Some(cfg) = schema.configs.iter().find(|x| x.position == position) {
            out.push_str(&format_config(cfg, config));
        }

        if let Some(en) = schema.enums.iter().find(|x| x.position == position) {
            out.push_str(&format_enum(en, config));
        }
    }

    out.trim_end().to_string() + "\n"
}
