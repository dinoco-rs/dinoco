use std::collections::HashSet;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
    ParameterInformation, ParameterLabel, Position, SignatureHelp, SignatureInformation,
};

use crate::document::{BlockKind, DocumentIndex, FieldInfo, line_prefix, scalar_types};

pub fn complete(source: &str, index: &DocumentIndex, position: Position) -> CompletionResponse {
    let prefix = line_prefix(source, position);
    let block = index.block_at(position);

    for attribute in ["ids", "uniques", "indexes", "fulltexts"] {
        if open_call_fragment(prefix, &format!("@@{attribute}(")).is_some() {
            return CompletionResponse::Array(model_attribute_field_completions(index, block));
        }
    }
    if let Some(fragment) = open_call_fragment(prefix, "@relation(") {
        return CompletionResponse::Array(relation_completions(index, block, position, fragment));
    }
    if open_call_fragment(prefix, "@default(").is_some() {
        return CompletionResponse::Array(default_completions(index.field_at(position).map(|(_, field)| field), index));
    }

    match block.map(|block| block.kind) {
        Some(BlockKind::Config) => CompletionResponse::Array(config_completions(prefix)),
        Some(BlockKind::Model) => CompletionResponse::Array(model_completions(prefix, index)),
        Some(BlockKind::Enum) => CompletionResponse::Array(enum_completions(prefix)),
        None => CompletionResponse::Array(root_completions()),
    }
}

pub fn signature(source: &str, position: Position) -> Option<SignatureHelp> {
    let prefix = line_prefix(source, position);
    for (attribute, description) in [
        ("ids", "Defines the model's composite primary key."),
        ("uniques", "Creates a composite unique constraint."),
        ("indexes", "Creates a composite standard database index."),
        ("fulltexts", "Creates one composite full-text index for String fields."),
    ] {
        if open_call_fragment(prefix, &format!("@@{attribute}(")).is_some() {
            return Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label: format!("@@{attribute}([fields])"),
                    documentation: Some(Documentation::MarkupContent(markdown(description))),
                    parameters: Some(vec![parameter("fields", "Ordered model fields included in this declaration.")]),
                    active_parameter: Some(0),
                }],
                active_signature: Some(0),
                active_parameter: None,
            });
        }
    }

    if let Some(fragment) = open_call_fragment(prefix, "@relation(") {
        return Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: "@relation(name, fields, references, onDelete, onUpdate)".to_string(),
                documentation: Some(Documentation::MarkupContent(markdown(
                    "Defines the local and referenced keys for a model relation.",
                ))),
                parameters: Some(vec![
                    parameter("name", "Disambiguates multiple relations between the same models."),
                    parameter("fields", "Local foreign-key fields."),
                    parameter("references", "Fields on the related model."),
                    parameter("onDelete", "Referential action when the related row is deleted."),
                    parameter("onUpdate", "Referential action when the related key is updated."),
                ]),
                active_parameter: Some(active_parameter(
                    fragment,
                    &["name", "fields", "references", "onDelete", "onUpdate"],
                )),
            }],
            active_signature: Some(0),
            active_parameter: None,
        });
    }

    if let Some(fragment) = open_call_fragment(prefix, "@default(") {
        return Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: "@default(value)".to_string(),
                documentation: Some(Documentation::MarkupContent(markdown(
                    "Sets a literal, enum, or generated default value for this field.",
                ))),
                parameters: Some(vec![parameter("value", "A literal, enum value, or supported generator function.")]),
                active_parameter: Some(active_parameter(fragment, &["value"])),
            }],
            active_signature: Some(0),
            active_parameter: None,
        });
    }

    if let Some(fragment) = open_call_fragment(prefix, "env(") {
        return Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: "env(name)".to_string(),
                documentation: Some(Documentation::MarkupContent(markdown(
                    "Reads a required Dinoco configuration value from the environment.",
                ))),
                parameters: Some(vec![parameter("name", "Environment variable name.")]),
                active_parameter: Some(active_parameter(fragment, &["name"])),
            }],
            active_signature: Some(0),
            active_parameter: None,
        });
    }

    None
}

fn root_completions() -> Vec<CompletionItem> {
    vec![
        snippet(
            "config",
            CompletionItemKind::MODULE,
            "Database configuration",
            "config {\n    database = \"${1|postgresql,mysql,sqlite|}\"\n    database_url = env(\"${2:DATABASE_URL}\")\n    read_replicas = []\n}\n",
        ),
        snippet(
            "model",
            CompletionItemKind::CLASS,
            "Define a persisted model",
            "model ${1:ModelName} {\n    id String @id @default(uuid())\n    $0\n}",
        ),
        snippet(
            "enum",
            CompletionItemKind::ENUM,
            "Define a database enum",
            "enum ${1:EnumName} {\n    ${2:VALUE}\n    $0\n}",
        ),
    ]
}

fn config_completions(prefix: &str) -> Vec<CompletionItem> {
    let trimmed = prefix.trim();
    if let Some((key, _)) = trimmed.split_once('=') {
        return match key.trim() {
            "database" => vec![
                value("postgresql", CompletionItemKind::ENUM_MEMBER, "PostgreSQL adapter", "\"postgresql\""),
                value("mysql", CompletionItemKind::ENUM_MEMBER, "MySQL adapter", "\"mysql\""),
                value("sqlite", CompletionItemKind::ENUM_MEMBER, "SQLite adapter", "\"sqlite\""),
            ],
            "connection" => vec![
                value("direct", CompletionItemKind::ENUM_MEMBER, "Direct PostgreSQL pool", "\"direct\""),
                value("pgbouncer", CompletionItemKind::ENUM_MEMBER, "PgBouncer-compatible connection", "\"pgbouncer\""),
            ],
            "database_url" | "snowflake_node_id" => vec![snippet(
                "env(...) ",
                CompletionItemKind::FUNCTION,
                "Read from the environment",
                "env(\"${1:DATABASE_URL}\")",
            )],
            "read_replicas" => vec![snippet(
                "[env(...)]",
                CompletionItemKind::VALUE,
                "Environment-backed read replicas",
                "[env(\"${1:DATABASE_REPLICA_URL}\")]",
            )],
            _ => Vec::new(),
        };
    }

    vec![
        snippet(
            "database",
            CompletionItemKind::PROPERTY,
            "Database adapter",
            "database = \"${1|postgresql,mysql,sqlite|}\"",
        ),
        snippet(
            "connection",
            CompletionItemKind::PROPERTY,
            "PostgreSQL connection strategy",
            "connection = \"${1|direct,pgbouncer|}\"",
        ),
        snippet(
            "database_url",
            CompletionItemKind::PROPERTY,
            "Primary database URL",
            "database_url = env(\"${1:DATABASE_URL}\")",
        ),
        snippet("read_replicas", CompletionItemKind::PROPERTY, "Read replica URLs", "read_replicas = [${1}]"),
        snippet(
            "snowflake_node_id",
            CompletionItemKind::PROPERTY,
            "Snowflake node ID",
            "snowflake_node_id = env(\"${1:SNOWFLAKE_NODE_ID}\")",
        ),
    ]
}

fn enum_completions(prefix: &str) -> Vec<CompletionItem> {
    if prefix.trim().is_empty() {
        vec![snippet("enum value", CompletionItemKind::ENUM_MEMBER, "Add an enum value", "${1:VALUE}")]
    } else {
        Vec::new()
    }
}

fn model_completions(prefix: &str, index: &DocumentIndex) -> Vec<CompletionItem> {
    let without_comment = prefix.split(['#']).next().unwrap_or(prefix);
    let trimmed = without_comment.trim_start();
    if trimmed.is_empty() {
        return vec![
            snippet("id (UUID)", CompletionItemKind::FIELD, "UUID primary key", "id String @id @default(uuid())"),
            snippet(
                "id (autoincrement)",
                CompletionItemKind::FIELD,
                "Auto-increment primary key",
                "id Integer @id @default(autoincrement())",
            ),
            snippet(
                "relation",
                CompletionItemKind::FIELD,
                "Relation field with a local foreign key",
                "${1:relation} ${2:Model}? @relation(fields: [${3:model_id}], references: [${4:id}])",
            ),
        ];
    }

    if current_word(trimmed).starts_with("@@") || trimmed.ends_with("@@") {
        return model_attribute_completions();
    }
    if current_word(trimmed).starts_with('@') || trimmed.ends_with('@') {
        return field_attribute_completions();
    }
    if is_field_type_context(without_comment) {
        let mut items = scalar_types()
            .iter()
            .map(|name| value(name, CompletionItemKind::TYPE_PARAMETER, scalar_documentation(name), name))
            .collect::<Vec<_>>();
        for block in &index.blocks {
            let Some(name) = &block.name else {
                continue;
            };
            match block.kind {
                BlockKind::Model => {
                    items.push(value(&name.name, CompletionItemKind::CLASS, "Relation model", &name.name))
                }
                BlockKind::Enum => items.push(value(&name.name, CompletionItemKind::ENUM, "Schema enum", &name.name)),
                BlockKind::Config => {}
            }
        }
        return items;
    }

    if trimmed.split_whitespace().count() >= 2 { field_attribute_completions() } else { Vec::new() }
}

fn model_attribute_completions() -> Vec<CompletionItem> {
    vec![
        snippet("@@ids", CompletionItemKind::FUNCTION, "Composite primary key", "ids([${1:id}, ${2:tenant_id}])"),
        snippet(
            "@@uniques",
            CompletionItemKind::FUNCTION,
            "Composite unique constraint",
            "uniques([${1:field_a}, ${2:field_b}])",
        ),
        snippet(
            "@@indexes",
            CompletionItemKind::FUNCTION,
            "Composite database index",
            "indexes([${1:field_a}, ${2:field_b}])",
        ),
        snippet(
            "@@fulltexts",
            CompletionItemKind::FUNCTION,
            "Composite full-text index",
            "fulltexts([${1:title}, ${2:body}])",
        ),
        snippet(
            "@@table_name",
            CompletionItemKind::FUNCTION,
            "Mapped database table name",
            "table_name(\"${1:table_name}\")",
        ),
    ]
}

fn model_attribute_field_completions(
    index: &DocumentIndex,
    block: Option<&crate::document::BlockInfo>,
) -> Vec<CompletionItem> {
    block
        .into_iter()
        .flat_map(|model| &model.fields)
        .filter(|field| index.model(&field.ty.name).is_none())
        .map(|field| value(&field.name.name, CompletionItemKind::FIELD, &field.display_type(), &field.name.name))
        .collect()
}

fn relation_completions(
    index: &DocumentIndex,
    block: Option<&crate::document::BlockInfo>,
    position: Position,
    fragment: &str,
) -> Vec<CompletionItem> {
    let segment = fragment.rsplit(',').next().unwrap_or(fragment).trim();
    if let Some((key, _)) = segment.split_once(':').or_else(|| segment.split_once('=')) {
        return match key.trim() {
            "onDelete" | "onUpdate" => relation_action_completions(),
            "fields" => block
                .into_iter()
                .flat_map(|model| &model.fields)
                .filter(|field| !index.model(&field.ty.name).is_some())
                .map(|field| {
                    value(&field.name.name, CompletionItemKind::FIELD, &field.display_type(), &field.name.name)
                })
                .collect(),
            "references" => index
                .field_at(position)
                .and_then(|(_, field)| index.model(&field.ty.name))
                .into_iter()
                .flat_map(|model| &model.fields)
                .filter(|field| !index.model(&field.ty.name).is_some())
                .map(|field| {
                    value(&field.name.name, CompletionItemKind::FIELD, &field.display_type(), &field.name.name)
                })
                .collect(),
            "name" => vec![snippet(
                "relation name",
                CompletionItemKind::VALUE,
                "Unique relation name",
                "\"${1:RelationName}\"",
            )],
            _ => Vec::new(),
        };
    }

    let used = fragment
        .split(',')
        .filter_map(|part| part.split_once(':').or_else(|| part.split_once('=')))
        .map(|(name, _)| name.trim())
        .collect::<HashSet<_>>();
    let candidates = [
        ("name", "name: \"${1:RelationName}\"", "Disambiguate repeated relations"),
        ("fields", "fields: [${1:field_id}]", "Local foreign-key fields"),
        ("references", "references: [${1:id}]", "Referenced model fields"),
        ("onDelete", "onDelete: ${1:Cascade}", "Action when a related row is deleted"),
        ("onUpdate", "onUpdate: ${1:Cascade}", "Action when a referenced key changes"),
    ];
    candidates
        .into_iter()
        .filter(|(name, _, _)| !used.contains(name))
        .map(|(name, insert, detail)| snippet(name, CompletionItemKind::PROPERTY, detail, insert))
        .collect()
}

fn default_completions(field: Option<&FieldInfo>, index: &DocumentIndex) -> Vec<CompletionItem> {
    let Some(field) = field else {
        return vec![
            value("true", CompletionItemKind::VALUE, "Boolean literal", "true"),
            value("false", CompletionItemKind::VALUE, "Boolean literal", "false"),
        ];
    };

    if let Some(item) = index.enum_(&field.ty.name) {
        return item
            .values
            .iter()
            .map(|item| {
                value(&item.name, CompletionItemKind::ENUM_MEMBER, &format!("{} value", field.ty.name), &item.name)
            })
            .collect();
    }

    match field.ty.name.as_str() {
        "String" => vec![
            snippet("string", CompletionItemKind::VALUE, "String literal", "\"${1:value}\""),
            value("uuid()", CompletionItemKind::FUNCTION, "Library-generated UUID", "uuid()"),
        ],
        "Integer" => vec![
            value("autoincrement()", CompletionItemKind::FUNCTION, "Database-generated integer", "autoincrement()"),
            value("snowflake()", CompletionItemKind::FUNCTION, "Library-generated Snowflake ID", "snowflake()"),
            snippet("integer", CompletionItemKind::VALUE, "Integer literal", "${1:0}"),
        ],
        "Float" => vec![snippet("float", CompletionItemKind::VALUE, "Float literal", "${1:0.0}")],
        "Boolean" => vec![
            value("true", CompletionItemKind::VALUE, "Boolean literal", "true"),
            value("false", CompletionItemKind::VALUE, "Boolean literal", "false"),
        ],
        "Date" | "DateTime" => vec![value("now()", CompletionItemKind::FUNCTION, "Current date/time", "now()")],
        "Json" => vec![snippet("JSON string", CompletionItemKind::VALUE, "JSON default", "\"${1:{}}\"")],
        _ => Vec::new(),
    }
}

fn field_attribute_completions() -> Vec<CompletionItem> {
    vec![
        value("@id", CompletionItemKind::PROPERTY, "Primary key", "id"),
        value("@unique", CompletionItemKind::PROPERTY, "Unique constraint", "unique"),
        value("@index", CompletionItemKind::PROPERTY, "Database index", "index"),
        value("@fulltext", CompletionItemKind::PROPERTY, "Full-text search index", "fulltext"),
        snippet("@default", CompletionItemKind::FUNCTION, "Default value", "default(${1})"),
        snippet(
            "@relation",
            CompletionItemKind::FUNCTION,
            "Model relation",
            "relation(fields: [${1:field_id}], references: [${2:id}], onDelete: ${3:Cascade}, onUpdate: ${4:Cascade})",
        ),
    ]
}

fn relation_action_completions() -> Vec<CompletionItem> {
    [
        ("Cascade", "Propagate the update or deletion"),
        ("Restrict", "Reject the operation when related rows exist"),
        ("NoAction", "Defer behavior to the database"),
        ("SetNull", "Set the optional local foreign key to null"),
        ("SetDefault", "Set the local foreign key to its default"),
    ]
    .into_iter()
    .map(|(name, detail)| value(name, CompletionItemKind::ENUM_MEMBER, detail, name))
    .collect()
}

fn open_call_fragment<'a>(prefix: &'a str, call: &str) -> Option<&'a str> {
    let start = prefix.rfind(call)?;
    let fragment = &prefix[start + call.len()..];
    if fragment.contains(')') { None } else { Some(fragment) }
}

fn active_parameter(fragment: &str, names: &[&str]) -> u32 {
    let segment = fragment.rsplit(',').next().unwrap_or(fragment).trim();
    if let Some((name, _)) = segment.split_once(':').or_else(|| segment.split_once('=')) {
        return names.iter().position(|candidate| *candidate == name.trim()).unwrap_or(0) as u32;
    }
    fragment.chars().filter(|character| *character == ',').count().min(names.len().saturating_sub(1)) as u32
}

fn is_field_type_context(prefix: &str) -> bool {
    let trimmed = prefix.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('@') || trimmed.contains('=') {
        return false;
    }
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    tokens.len() == 1 && prefix.chars().last().is_some_and(char::is_whitespace)
        || tokens.len() == 2 && !trimmed.contains('@')
}

fn current_word(prefix: &str) -> &str {
    prefix.split_whitespace().last().unwrap_or_default()
}

fn scalar_documentation(name: &str) -> &'static str {
    match name {
        "String" => "UTF-8 text",
        "Boolean" => "True or false",
        "Integer" => "Signed 64-bit integer",
        "Float" => "Double-precision number",
        "DateTime" => "UTC date and time",
        "Date" => "Calendar date without time",
        "Json" => "Structured JSON value",
        _ => "Dinoco scalar",
    }
}

fn value(label: &str, kind: CompletionItemKind, detail: &str, insert_text: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        documentation: Some(Documentation::MarkupContent(markdown(detail))),
        insert_text: Some(insert_text.to_string()),
        ..CompletionItem::default()
    }
}

fn snippet(label: &str, kind: CompletionItemKind, detail: &str, insert_text: &str) -> CompletionItem {
    CompletionItem { insert_text_format: Some(InsertTextFormat::SNIPPET), ..value(label, kind, detail, insert_text) }
}

fn markdown(value: &str) -> MarkupContent {
    MarkupContent { kind: MarkupKind::Markdown, value: value.to_string() }
}

fn parameter(label: &str, documentation: &str) -> ParameterInformation {
    ParameterInformation {
        label: ParameterLabel::Simple(label.to_string()),
        documentation: Some(Documentation::MarkupContent(markdown(documentation))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_target_fields_inside_references() {
        let source = r#"model User { id String @id }
model Token {
    user User? @relation(fields: [user_id], references: [i
    user_id String?
}"#;
        let index = DocumentIndex::new(source);
        let response = complete(source, &index, Position::new(2, 65));
        let CompletionResponse::Array(items) = response else {
            panic!("array response");
        };
        assert!(items.iter().any(|item| item.label == "id"));
    }

    #[test]
    fn suggests_enum_defaults() {
        let source = "enum Role { USER ADMIN }\nmodel User { role Role @default( }";
        let index = DocumentIndex::new(source);
        let cursor = source.lines().nth(1).expect("model line").encode_utf16().count() as u32 - 1;
        let response = complete(source, &index, Position::new(1, cursor));
        let CompletionResponse::Array(items) = response else {
            panic!("array response");
        };
        assert!(items.iter().any(|item| item.label == "USER"));
    }
}
