use dinoco_compiler::{Attribute, EnumDef, FieldType, Model, ModelField};

use crate::FormatterConfig;
use crate::utils::grouped_field_widths;
use crate::value::format_attribute_argument;

pub fn format_enum(enum_def: &EnumDef, config: &FormatterConfig) -> String {
    let indent = config.indent();
    let mut out = format!("enum {} {{\n", enum_def.name);

    for value in &enum_def.values {
        out.push_str(&indent);
        out.push_str(value);
        out.push('\n');
    }

    out.push('}');
    out
}

pub fn format_model(model: &Model, config: &FormatterConfig) -> String {
    format_model_with_layout(model, config, None)
}

pub(crate) fn format_model_with_layout(
    model: &Model,
    config: &FormatterConfig,
    blank_lines: Option<&[bool]>,
) -> String {
    let indent = config.indent();
    let widths = grouped_field_widths(&model.fields);
    let mut out = format!("model {} {{\n", model.name);

    for (index, field) in model.fields.iter().enumerate() {
        if index > 0 && blank_lines.and_then(|hints| hints.get(index)).copied().unwrap_or(false) {
            out.push('\n');
        }
        let type_string = format_field_type(&field.ty);
        let (name_width, type_width) = widths;

        out.push_str(&indent);
        out.push_str(&format!("{:<width$}", field.name, width = name_width));
        out.push_str("  ");

        let attributes = format_attributes(&field.attributes);
        if attributes.is_empty() {
            out.push_str(&type_string);
        } else {
            out.push_str(&format!("{:<width$}", type_string, width = type_width));
            out.push_str("  ");
            out.push_str(&attributes);
        }

        out.push('\n');
    }

    out.push('}');
    out
}

pub fn format_field_type(field_type: &FieldType) -> String {
    let mut value = field_type.name.clone();

    if field_type.list {
        value.push_str("[]");
    }

    if field_type.optional {
        value.push('?');
    }

    value
}

fn format_attributes(attributes: &[Attribute]) -> String {
    attributes.iter().map(format_attribute).collect::<Vec<_>>().join(" ")
}

fn format_attribute(attribute: &Attribute) -> String {
    if attribute.arguments.is_empty() {
        return format!("@{}", attribute.name);
    }

    let arguments = attribute.arguments.iter().map(format_attribute_argument).collect::<Vec<_>>();

    format!("@{}({})", attribute.name, arguments.join(", "))
}

pub(crate) fn field_type_len(field: &ModelField) -> usize {
    format_field_type(&field.ty).len()
}
