use dinoco_compiler::ModelField;

use crate::model::field_type_len;

pub fn grouped_field_widths(fields: &[ModelField], blank_lines: Option<&[bool]>) -> Vec<(usize, usize)> {
    let mut widths = vec![(0, 0); fields.len()];
    let mut group_start = 0;

    for index in 1..=fields.len() {
        let starts_new_group =
            index == fields.len() || blank_lines.and_then(|hints| hints.get(index)).copied().unwrap_or(false);
        if !starts_new_group {
            continue;
        }

        let group = &fields[group_start..index];
        let max_name_len = group.iter().map(|field| field.name.len()).max().unwrap_or_default();
        let max_type_len = group.iter().map(field_type_len).max().unwrap_or_default();
        widths[group_start..index].fill((max_name_len, max_type_len));
        group_start = index;
    }

    widths
}
