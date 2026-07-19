use std::collections::{HashMap, HashSet};

use dinoco_compiler::{Attribute, AttributeValue, ConfigValue, Schema};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::document::{BlockInfo, BlockKind, DocumentIndex, FieldInfo, scalar_types};

pub const CODE_UNKNOWN_TYPE: &str = "dinoco.unknownType";
pub const CODE_MISSING_CONFIG: &str = "dinoco.missingConfig";
pub const CODE_MISSING_DATABASE_URL: &str = "dinoco.missingDatabaseUrl";

pub fn analyze(source: &str, index: &DocumentIndex) -> Vec<Diagnostic> {
    let schema = match dinoco_compiler::compile(source) {
        Ok(schema) => schema,
        Err(error) => {
            let start = compiler_position(source, error.line, error.column);
            let end = Position::new(start.line, start.character.saturating_add(1));
            return vec![diagnostic(Range::new(start, end), DiagnosticSeverity::ERROR, "dinoco.syntax", error.message)];
        }
    };

    let mut diagnostics = Vec::new();
    validate_top_level(index, &mut diagnostics);
    validate_config(&schema, index, &mut diagnostics);
    validate_models(&schema, index, &mut diagnostics);
    diagnostics
}

fn validate_top_level(index: &DocumentIndex, diagnostics: &mut Vec<Diagnostic>) {
    let mut declarations: HashMap<&str, Range> = HashMap::new();
    for block in &index.blocks {
        let Some(name) = &block.name else {
            continue;
        };
        if declarations.insert(&name.name, name.range).is_some() {
            let item = diagnostic(
                name.range,
                DiagnosticSeverity::ERROR,
                "dinoco.duplicateType",
                format!("Type `{}` is declared more than once.", name.name),
            );
            diagnostics.push(item);
        }
    }

    if index.blocks.iter().filter(|block| block.kind == BlockKind::Config).count() > 1 {
        for block in index.blocks.iter().filter(|block| block.kind == BlockKind::Config).skip(1) {
            diagnostics.push(diagnostic(
                block.range,
                DiagnosticSeverity::ERROR,
                "dinoco.duplicateConfig",
                "Only one `config` block is allowed.",
            ));
        }
    }
}

fn validate_config(schema: &Schema, index: &DocumentIndex, diagnostics: &mut Vec<Diagnostic>) {
    let Some(config) = schema.config() else {
        diagnostics.push(diagnostic(
            Range::new(Position::new(0, 0), Position::new(0, 0)),
            DiagnosticSeverity::ERROR,
            CODE_MISSING_CONFIG,
            "A `config` block is required to connect Dinoco to a database.",
        ));
        return;
    };

    let config_index = index.config();
    let known = ["database", "connection", "database_url", "read_replicas", "snowflake_node_id"];
    let mut seen = HashSet::new();
    for entry in &config.entries {
        let range = config_entry_range(config_index, &entry.key);
        if !seen.insert(entry.key.as_str()) {
            diagnostics.push(diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "dinoco.duplicateConfigKey",
                format!("Config key `{}` is declared more than once.", entry.key),
            ));
        }
        if !known.contains(&entry.key.as_str()) {
            diagnostics.push(diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "dinoco.unknownConfigKey",
                format!("Unknown config key `{}`.", entry.key),
            ));
        }
    }

    let database = config.entries.iter().find(|entry| entry.key == "database");
    match database.map(|entry| &entry.value) {
        Some(ConfigValue::String(value) | ConfigValue::Ident(value))
            if matches!(value.as_str(), "postgresql" | "postgres" | "mysql" | "sqlite") => {}
        Some(_) => diagnostics.push(diagnostic(
            config_entry_range(config_index, "database"),
            DiagnosticSeverity::ERROR,
            "dinoco.invalidDatabase",
            "Database must be `postgresql`, `mysql`, or `sqlite`.",
        )),
        None => diagnostics.push(diagnostic(
            config_index.map_or(default_range(), |block| block.body_range),
            DiagnosticSeverity::ERROR,
            "dinoco.missingDatabase",
            "Config key `database` is required.",
        )),
    }

    if !config.entries.iter().any(|entry| entry.key == "database_url") {
        diagnostics.push(diagnostic(
            config_index.map_or(default_range(), |block| block.body_range),
            DiagnosticSeverity::ERROR,
            CODE_MISSING_DATABASE_URL,
            "Config key `database_url = env(\"DATABASE_URL\")` is required.",
        ));
    }
}

fn validate_models(schema: &Schema, index: &DocumentIndex, diagnostics: &mut Vec<Diagnostic>) {
    let models = schema.models().map(|model| model.name.as_str()).collect::<HashSet<_>>();
    let enums = schema.enums().map(|item| item.name.as_str()).collect::<HashSet<_>>();

    for item in schema.enums() {
        let Some(enum_index) = index.enum_(&item.name) else {
            continue;
        };
        let mut seen = HashSet::new();
        for value in &enum_index.values {
            if !seen.insert(value.name.as_str()) {
                diagnostics.push(diagnostic(
                    value.range,
                    DiagnosticSeverity::ERROR,
                    "dinoco.duplicateEnumValue",
                    format!("Enum value `{}` is declared more than once.", value.name),
                ));
            }
        }
        if item.values.is_empty() {
            diagnostics.push(diagnostic(
                enum_index.body_range,
                DiagnosticSeverity::ERROR,
                "dinoco.emptyEnum",
                format!("Enum `{}` must contain at least one value.", item.name),
            ));
        }
    }

    for model in schema.models() {
        let Some(model_index) = index.model(&model.name) else {
            continue;
        };
        let mut seen = HashSet::new();
        let mut relation_targets: HashMap<&str, Vec<(&dinoco_compiler::ModelField, &FieldInfo)>> = HashMap::new();

        for field in &model.fields {
            let Some(field_index) = model_index.field(&field.name) else {
                continue;
            };
            if !seen.insert(field.name.as_str()) {
                diagnostics.push(diagnostic(
                    field_index.name.range,
                    DiagnosticSeverity::ERROR,
                    "dinoco.duplicateField",
                    format!("Field `{}.{}` is declared more than once.", model.name, field.name),
                ));
            }

            let known = scalar_types().contains(&field.ty.name.as_str())
                || models.contains(field.ty.name.as_str())
                || enums.contains(field.ty.name.as_str());
            if !known {
                diagnostics.push(diagnostic(
                    field_index.ty.range,
                    DiagnosticSeverity::ERROR,
                    CODE_UNKNOWN_TYPE,
                    format!("Unknown type `{}`.", field.ty.name),
                ));
            }

            if let Some(relation) = field.attributes.iter().find(|attribute| attribute.name == "relation") {
                relation_targets.entry(&field.ty.name).or_default().push((field, field_index));
                validate_relation(schema, model, field, relation, model_index, field_index, diagnostics);
            }
        }

        for (target, relations) in relation_targets {
            if relations.len() < 2 {
                continue;
            }
            for (_, field_index) in relations {
                let relation = field_index.attribute("relation").expect("indexed relation attribute");
                if relation.argument("name").is_none() {
                    diagnostics.push(diagnostic(
                        relation.name.range,
                        DiagnosticSeverity::ERROR,
                        "dinoco.ambiguousRelation",
                        format!(
                            "Multiple relations from `{}` to `{target}` require a unique `name` argument.",
                            model.name
                        ),
                    ));
                }
            }
        }
    }
}

fn validate_relation(
    schema: &Schema,
    model: &dinoco_compiler::Model,
    field: &dinoco_compiler::ModelField,
    relation: &Attribute,
    model_index: &BlockInfo,
    field_index: &FieldInfo,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(target) = schema.models().find(|candidate| candidate.name == field.ty.name) else {
        diagnostics.push(diagnostic(
            field_index.ty.range,
            DiagnosticSeverity::ERROR,
            "dinoco.invalidRelationTarget",
            format!("Relation target `{}` is not a model.", field.ty.name),
        ));
        return;
    };

    let local_names = attribute_array(relation, "fields");
    let reference_names = attribute_array(relation, "references");
    if local_names.is_empty() && reference_names.is_empty() {
        return;
    }
    let relation_range = field_index.attribute("relation").map_or(field_index.range, |item| item.range);
    if local_names.len() != reference_names.len() {
        diagnostics.push(diagnostic(
            relation_range,
            DiagnosticSeverity::ERROR,
            "dinoco.relationArity",
            "`fields` and `references` must contain the same number of fields.",
        ));
        return;
    }

    for (local_name, reference_name) in local_names.iter().zip(&reference_names) {
        let Some(local) = model.fields.iter().find(|candidate| candidate.name == *local_name) else {
            diagnostics.push(diagnostic(
                relation_argument_value_range(field_index, "fields", local_name),
                DiagnosticSeverity::ERROR,
                "dinoco.unknownRelationField",
                format!("Field `{}.{local_name}` does not exist.", model.name),
            ));
            continue;
        };
        let Some(reference) = target.fields.iter().find(|candidate| candidate.name == *reference_name) else {
            diagnostics.push(diagnostic(
                relation_argument_value_range(field_index, "references", reference_name),
                DiagnosticSeverity::ERROR,
                "dinoco.unknownReferenceField",
                format!("Field `{}.{reference_name}` does not exist.", target.name),
            ));
            continue;
        };

        if local.ty.name != reference.ty.name {
            diagnostics.push(diagnostic(
                relation_argument_value_range(field_index, "fields", local_name),
                DiagnosticSeverity::ERROR,
                "dinoco.relationTypeMismatch",
                format!(
                    "Relation fields have incompatible types: `{}.{}` is `{}` but `{}.{}` is `{}`.",
                    model.name, local.name, local.ty.name, target.name, reference.name, reference.ty.name
                ),
            ));
        }

        for action in ["onDelete", "onUpdate"] {
            if attribute_ident(relation, action) == Some("SetNull") && !local.ty.optional {
                diagnostics.push(diagnostic(
                    relation_argument_name_range(field_index, action),
                    DiagnosticSeverity::ERROR,
                    "dinoco.setNullRequiresOptionalField",
                    format!("`{action}: SetNull` requires local field `{local_name}` to be optional."),
                ));
            }
            if attribute_ident(relation, action) == Some("SetDefault")
                && !local.attributes.iter().any(|attribute| attribute.name == "default")
            {
                diagnostics.push(diagnostic(
                    relation_argument_name_range(field_index, action),
                    DiagnosticSeverity::ERROR,
                    "dinoco.setDefaultRequiresDefault",
                    format!("`{action}: SetDefault` requires local field `{local_name}` to define `@default(...)`."),
                ));
            }
        }
    }

    let _ = model_index;
}

fn attribute_array<'a>(attribute: &'a Attribute, name: &str) -> Vec<&'a str> {
    match attribute.argument(name) {
        Some(AttributeValue::Array(values)) => values
            .iter()
            .filter_map(|value| match value {
                AttributeValue::Ident(value) | AttributeValue::String(value) => Some(value.as_str()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn attribute_ident<'a>(attribute: &'a Attribute, name: &str) -> Option<&'a str> {
    match attribute.argument(name) {
        Some(AttributeValue::Ident(value) | AttributeValue::String(value)) => Some(value),
        _ => None,
    }
}

fn config_entry_range(index: Option<&BlockInfo>, key: &str) -> Range {
    index
        .and_then(|block| block.entries.iter().find(|entry| entry.name == key))
        .map_or_else(default_range, |entry| entry.range)
}

fn relation_argument_value_range(field: &FieldInfo, argument: &str, value: &str) -> Range {
    field
        .attribute("relation")
        .and_then(|relation| relation.argument(argument))
        .and_then(|argument| argument.values.iter().find(|candidate| candidate.name == value))
        .map_or(field.range, |symbol| symbol.range)
}

fn relation_argument_name_range(field: &FieldInfo, argument: &str) -> Range {
    field
        .attribute("relation")
        .and_then(|relation| relation.argument(argument))
        .map_or(field.range, |argument| argument.name.range)
}

fn compiler_position(source: &str, one_based_line: usize, one_based_column: usize) -> Position {
    let line = one_based_line.saturating_sub(1);
    let requested = one_based_column.saturating_sub(1);
    let text = source.lines().nth(line).unwrap_or_default();
    let character = text.chars().take(requested).map(char::len_utf16).sum::<usize>() as u32;
    Position::new(line as u32, character)
}

fn default_range() -> Range {
    Range::new(Position::new(0, 0), Position::new(0, 0))
}

fn diagnostic(range: Range, severity: DiagnosticSeverity, code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("dinoco".to_string()),
        message: message.into(),
        ..Diagnostic::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_semantic_schema_problems() {
        let source = r#"config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
        }
        model User {
            id String @id
            owner Missing
        }"#;
        let diagnostics = analyze(source, &DocumentIndex::new(source));
        assert!(diagnostics.iter().any(|item| item.code == Some(NumberOrString::String(CODE_UNKNOWN_TYPE.into()))));
    }

    #[test]
    fn reports_ambiguous_repeated_relations() {
        let source = r#"config { database = "sqlite" database_url = env("DATABASE_URL") }
        model User { id String @id posts Post[] comments Post[] }
        model Post {
            id String @id
            author User? @relation(fields: [author_id], references: [id])
            author_id String?
            editor User? @relation(fields: [editor_id], references: [id])
            editor_id String?
        }"#;
        let diagnostics = analyze(source, &DocumentIndex::new(source));
        assert!(
            diagnostics.iter().any(|item| item.code == Some(NumberOrString::String("dinoco.ambiguousRelation".into())))
        );
    }
}
