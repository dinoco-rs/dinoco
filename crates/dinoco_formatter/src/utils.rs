use dinoco_compiler::ModelField;

use crate::model::field_type_len;

pub fn grouped_field_widths(fields: &[ModelField]) -> (usize, usize) {
    let max_name_len = fields.iter().map(|field| field.name.len()).max().unwrap_or_default();
    let max_type_len = fields.iter().map(field_type_len).max().unwrap_or_default();

    (max_name_len, max_type_len)
}
