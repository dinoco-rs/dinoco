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

pub fn format_from_raw(raw_input: &str) -> Option<String> {
    let schema = match compile_only_ast(&raw_input) {
        Ok(result) => result,
        Err(errs) => {
            eprintln!("Compile error:\n{:#?}", errs);

            return None;
        }
    };

    return Some(format_from_ast(&schema, &FormatterConfig::default()));
}

pub fn format_from_ast(schema: &Schema, config: &FormatterConfig) -> String {
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
