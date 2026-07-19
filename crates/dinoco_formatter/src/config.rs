use dinoco_compiler::ConfigBlock;

use crate::FormatterConfig;
use crate::value::format_config_value;

pub fn format_config(block: &ConfigBlock, config: &FormatterConfig) -> String {
    let indent = config.indent();
    let max_key_len = block.entries.iter().map(|entry| entry.key.len()).max().unwrap_or_default();
    let mut out = String::from("config {\n");

    for entry in &block.entries {
        out.push_str(&indent);
        out.push_str(&format!("{:<width$}", entry.key, width = max_key_len));
        out.push_str(" = ");
        out.push_str(&format_config_value(&entry.value, config, 1));
        out.push('\n');
    }

    out.push('}');
    out
}
