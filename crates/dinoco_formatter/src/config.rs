use dinoco_compiler::ConfigBlock;

use crate::FormatterConfig;
use crate::value::format_config_value;

pub fn format_config(block: &ConfigBlock, config: &FormatterConfig) -> String {
    let indent = config.indent();
    let mut out = String::from("config {\n");

    format_entries(&mut out, &block.entries, config, 1);
    if !block.workspaces.is_empty() {
        if !block.entries.is_empty() {
            out.push('\n');
        }
        out.push_str(&indent);
        out.push_str("workspace {\n");
        for (index, workspace) in block.workspaces.iter().enumerate() {
            out.push_str(&indent.repeat(2));
            out.push_str(&workspace.name);
            out.push_str(" {\n");
            format_entries(&mut out, &workspace.entries, config, 3);
            out.push_str(&indent.repeat(2));
            out.push_str("}\n");
            if index + 1 != block.workspaces.len() {
                out.push('\n');
            }
        }
        out.push_str(&indent);
        out.push_str("}\n");
    }

    out.push('}');
    out
}

fn format_entries(
    out: &mut String,
    entries: &[dinoco_compiler::ConfigEntry],
    config: &FormatterConfig,
    indent_level: usize,
) {
    let max_key_len = entries.iter().map(|entry| entry.key.len()).max().unwrap_or_default();
    let indent = config.indent().repeat(indent_level);
    for entry in entries {
        out.push_str(&indent);
        out.push_str(&format!("{:<width$}", entry.key, width = max_key_len));
        out.push_str(" = ");
        out.push_str(&format_config_value(&entry.value, config, indent_level));
        out.push('\n');
    }
}
