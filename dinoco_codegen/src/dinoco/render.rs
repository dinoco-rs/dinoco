use dinoco_compiler::{
    ConnectionUrl, Database, FunctionCall, ParsedField, ParsedFieldDefault, ParsedFieldType, ParsedRelation,
    ParsedSchema, ReferentialAction,
};

pub fn render_schema(schema: &ParsedSchema) -> String {
    let mut output = String::new();

    output.push_str(&render_config(schema));

    if !schema.enums.is_empty() {
        output.push('\n');

        for parsed_enum in &schema.enums {
            output.push_str(&format!("enum {} {{\n", parsed_enum.name));

            for value in &parsed_enum.values {
                output.push_str(&format!("    {}\n", value));
            }

            output.push_str("}\n\n");
        }
    }

    for table in &schema.tables {
        output.push_str(&format!("model {} {{\n", table.name));

        for field in &table.fields {
            output.push_str("    ");
            output.push_str(&render_field(field));
            output.push('\n');
        }

        output.push_str("}\n\n");
    }

    output.trim_end().to_string()
}

fn render_config(schema: &ParsedSchema) -> String {
    let mut output = String::from("config {\n");

    output.push_str(&format!("    database = {}\n", render_database(&schema.config.database)));
    output.push_str(&format!("    database_url = {}\n", render_connection_url(&schema.config.database_url)));

    if !schema.config.read_replicas.is_empty() {
        let read_replicas =
            schema.config.read_replicas.iter().map(render_connection_url).collect::<Vec<_>>().join(", ");

        output.push_str(&format!("    read_replicas = [{}]\n", read_replicas));
    }

    output.push_str("}\n");

    output
}

fn render_field(field: &ParsedField) -> String {
    let mut output = format!("{} {}", field.name, render_field_type(field));

    for attribute in render_field_attributes(field) {
        output.push(' ');
        output.push_str(&attribute);
    }

    output
}

fn render_field_type(field: &ParsedField) -> String {
    let base = match &field.field_type {
        ParsedFieldType::String => "String".to_string(),
        ParsedFieldType::Boolean => "Boolean".to_string(),
        ParsedFieldType::Integer => "Integer".to_string(),
        ParsedFieldType::Float => "Float".to_string(),
        ParsedFieldType::Json => "Json".to_string(),
        ParsedFieldType::DateTime => "DateTime".to_string(),
        ParsedFieldType::Date => "Date".to_string(),
        ParsedFieldType::Enum(name) => name.clone(),
        ParsedFieldType::Relation(name) => name.clone(),
    };

    if field.is_list {
        format!("{base}[]")
    } else if field.is_optional {
        format!("{base}?")
    } else {
        base
    }
}

fn render_field_attributes(field: &ParsedField) -> Vec<String> {
    let mut attributes = Vec::new();

    if field.is_primary_key {
        attributes.push("@id".to_string());
    }

    if field.is_unique && !field.is_primary_key {
        attributes.push("@unique".to_string());
    }

    if !matches!(field.default_value, ParsedFieldDefault::NotDefined) {
        attributes.push(format!("@default({})", render_default_value(&field.default_value)));
    }

    if let Some(relation_attribute) = render_relation_attribute(&field.relation) {
        attributes.push(relation_attribute);
    }

    attributes
}

fn render_default_value(default_value: &ParsedFieldDefault) -> String {
    match default_value {
        ParsedFieldDefault::NotDefined => String::new(),
        ParsedFieldDefault::String(value) => format!("\"{}\"", value),
        ParsedFieldDefault::Boolean(value) => value.to_string(),
        ParsedFieldDefault::Integer(value) => value.to_string(),
        ParsedFieldDefault::Float(value) => value.to_string(),
        ParsedFieldDefault::EnumValue(value) => value.clone(),
        ParsedFieldDefault::Function(function) => match function {
            FunctionCall::Uuid => "uuid()".to_string(),
            FunctionCall::Snowflake => "snowflake()".to_string(),
            FunctionCall::AutoIncrement => "autoincrement()".to_string(),
            FunctionCall::Now => "now()".to_string(),
            FunctionCall::Env(value) => format!("env(\"{}\")", value),
        },
    }
}

fn render_relation_attribute(relation: &ParsedRelation) -> Option<String> {
    match relation {
        ParsedRelation::NotDefined => None,
        ParsedRelation::OneToOneInverse(name) | ParsedRelation::OneToMany(name) | ParsedRelation::ManyToMany(name) => {
            name.as_ref().map(|name| format!("@relation(name: {})", name))
        }
        ParsedRelation::ManyToOne(name, fields, references, on_delete, on_update)
        | ParsedRelation::OneToOneOwner(name, fields, references, on_delete, on_update) => {
            let mut parts = Vec::new();

            if let Some(name) = name {
                parts.push(format!("name: {}", name));
            }

            parts.push(format!("fields: [{}]", fields.join(", ")));
            parts.push(format!("references: [{}]", references.join(", ")));

            if let Some(action) = on_delete {
                parts.push(format!("onDelete: {}", render_referential_action(action)));
            }

            if let Some(action) = on_update {
                parts.push(format!("onUpdate: {}", render_referential_action(action)));
            }

            Some(format!("@relation({})", parts.join(", ")))
        }
    }
}

fn render_database(database: &Database) -> &'static str {
    match database {
        Database::Mysql => "\"mysql\"",
        Database::Postgresql => "\"postgresql\"",
        Database::Sqlite => "\"sqlite\"",
    }
}

fn render_connection_url(url: &ConnectionUrl) -> String {
    match url {
        ConnectionUrl::Literal(value) => format!("\"{}\"", value),
        ConnectionUrl::Env(value) => format!("env(\"{}\")", value),
    }
}

fn render_referential_action(action: &ReferentialAction) -> &'static str {
    match action {
        ReferentialAction::Cascade => "Cascade",
        ReferentialAction::SetNull => "SetNull",
        ReferentialAction::SetDefault => "SetDefault",
    }
}
