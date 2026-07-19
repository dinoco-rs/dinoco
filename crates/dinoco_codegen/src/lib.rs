use std::fs;
use std::path::Path;

use dinoco_compiler::{AttributeValue, ConfigValue, Model, ModelField, Schema};

pub fn generate_models(schema: &Schema) -> anyhow::Result<()> {
    fs::create_dir_all("dinoco/models")?;
    let stale_flat_file = Path::new("dinoco/models.rs");
    if stale_flat_file.exists() {
        fs::remove_file(stale_flat_file)?;
    }

    fs::write("dinoco/models/mod.rs", render_models_mod(schema))?;
    for model in schema.models() {
        fs::write(format!("dinoco/models/{}.rs", to_snake_case(&model.name)), render_model_file(&model, schema))?;
    }
    fs::write("dinoco/mod.rs", render_dinoco_mod(schema))?;
    Ok(())
}

pub fn render_models(schema: &Schema) -> String {
    let mut out = String::new();
    out.push_str(&render_models_mod(schema));
    for model in schema.models() {
        out.push('\n');
        out.push_str(&render_model_file(&model, schema));
    }
    out
}

pub fn render_models_mod(schema: &Schema) -> String {
    let mut out = String::new();
    for item in schema.enums() {
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq, Default)]\n");
        out.push_str(&format!("pub enum {} {{\n", item.name));
        for (index, value) in item.values.iter().enumerate() {
            if index == 0 {
                out.push_str("    #[default]\n");
            }
            out.push_str("    ");
            out.push_str(&to_pascal_case(value));
            out.push_str(",\n");
        }
        out.push_str("}\n\n");
        out.push_str(&format!("impl ::core::convert::From<&{}> for ::dinoco::DinocoValue {{\n", item.name));
        out.push_str("    fn from(value: &");
        out.push_str(&item.name);
        out.push_str(") -> Self {\n");
        out.push_str("        match value {\n");
        for value in &item.values {
            out.push_str(&format!(
                "            {}::{} => ::dinoco::DinocoValue::Enum(\"{}\".to_string(), \"{}\".to_string()),\n",
                item.name,
                to_pascal_case(value),
                item.name,
                value
            ));
        }
        out.push_str("        }\n    }\n}\n\n");
        out.push_str(&format!("impl ::dinoco::rusqlite::types::FromSql for {} {{\n", item.name));
        out.push_str("    fn column_result(value: ::dinoco::rusqlite::types::ValueRef<'_>) -> ::dinoco::rusqlite::types::FromSqlResult<Self> {\n");
        out.push_str("        let value = <String as ::dinoco::rusqlite::types::FromSql>::column_result(value)?;\n");
        out.push_str("        match value.as_str() {\n");
        for value in &item.values {
            out.push_str(&format!("            \"{}\" => Ok(Self::{}),\n", value, to_pascal_case(value)));
        }
        out.push_str("            _ => Err(::dinoco::rusqlite::types::FromSqlError::InvalidType),\n");
        out.push_str("        }\n    }\n}\n\n");
        out.push_str(&format!("impl<'a> ::dinoco::tokio_postgres::types::FromSql<'a> for {} {{\n", item.name));
        out.push_str("    fn from_sql(ty: &::dinoco::tokio_postgres::types::Type, raw: &'a [u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {\n");
        out.push_str("        let value = <String as ::dinoco::tokio_postgres::types::FromSql>::from_sql(ty, raw)?;\n");
        out.push_str("        match value.as_str() {\n");
        for value in &item.values {
            out.push_str(&format!("            \"{}\" => Ok(Self::{}),\n", value, to_pascal_case(value)));
        }
        out.push_str("            _ => Err(format!(\"unknown enum value `{}`\", value).into()),\n");
        out.push_str("        }\n    }\n");
        out.push_str("    fn accepts(ty: &::dinoco::tokio_postgres::types::Type) -> bool { <String as ::dinoco::tokio_postgres::types::FromSql>::accepts(ty) }\n");
        out.push_str("}\n\n");
        out.push_str(&format!("impl ::dinoco::mysql_common::prelude::FromValue for {} {{\n", item.name));
        out.push_str("    type Intermediate = Self;\n");
        out.push_str("}\n\n");
        out.push_str(&format!("impl ::core::convert::TryFrom<::dinoco::mysql_async::Value> for {} {{\n", item.name));
        out.push_str("    type Error = ::dinoco::mysql_common::value::convert::FromValueError;\n");
        out.push_str("    fn try_from(value: ::dinoco::mysql_async::Value) -> Result<Self, ::dinoco::mysql_common::value::convert::FromValueError> {\n");
        out.push_str("        let raw = value.clone();\n");
        out.push_str(
            "        let value = <String as ::dinoco::mysql_common::prelude::FromValue>::from_value_opt(value)?;\n",
        );
        out.push_str("        match value.as_str() {\n");
        for value in &item.values {
            out.push_str(&format!("            \"{}\" => Ok(Self::{}),\n", value, to_pascal_case(value)));
        }
        out.push_str("            _ => Err(::dinoco::mysql_common::value::convert::FromValueError(raw)),\n");
        out.push_str("        }\n    }\n}\n\n");
    }

    for model in schema.models() {
        let module = to_snake_case(&model.name);
        out.push_str(&format!("mod {module};\n"));
        out.push_str(&format!("pub use {module}::*;\n"));
    }

    out
}

pub fn render_model_file(model: &Model, schema: &Schema) -> String {
    let mut out = String::new();
    out.push_str("use super::*;\n");
    out.push_str("use dinoco::Entity;\n\n");
    out.push_str("#[derive(Debug, Entity)]\n");
    out.push_str(&format!("#[dinoco(table_name = \"{}\")]\n", to_snake_case(&model.name)));
    out.push_str(&format!("pub struct {} {{\n", model.name));
    for field in &model.fields {
        for attr in field_attributes(model, field, schema) {
            out.push_str("    ");
            out.push_str(&attr);
            out.push('\n');
        }
        out.push_str("    pub ");
        out.push_str(&field.name);
        out.push_str(": ");
        out.push_str(&rust_type(field, schema));
        out.push_str(",\n\n");
    }
    out.push_str("}\n");
    out
}

pub fn render_dinoco_mod(schema: &Schema) -> String {
    let config = schema.config();
    let database = config
        .and_then(|config| config.entries.iter().find(|entry| entry.key == "database"))
        .and_then(|entry| match &entry.value {
            ConfigValue::String(value) | ConfigValue::Ident(value) => Some(value.as_str()),
            _ => None,
        })
        .unwrap_or("sqlite");
    let connection = config
        .and_then(|config| config.entries.iter().find(|entry| entry.key == "connection"))
        .and_then(|entry| match &entry.value {
            ConfigValue::String(value) | ConfigValue::Ident(value) => Some(value.as_str()),
            _ => None,
        })
        .unwrap_or("direct");
    let database_url_env = config
        .and_then(|config| config.entries.iter().find(|entry| entry.key == "database_url"))
        .and_then(|entry| match &entry.value {
            ConfigValue::Env(value) => Some(value.as_str()),
            _ => None,
        })
        .unwrap_or("DATABASE_URL");

    let mut out = String::new();
    out.push_str("pub mod models;\n\n");
    out.push_str("pub use models::*;\n\n");
    out.push_str("pub async fn connect() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {\n");
    out.push_str(&format!("    let database_url = std::env::var(\"{database_url_env}\")?;\n"));
    match database {
        "postgresql" | "postgres" if connection == "pgbouncer" => out.push_str(
            "    let adapter = ::dinoco::PgBouncerAdapter::new(database_url).await?;\n    Ok(::dinoco::DinocoClient::new(::dinoco::Backend::PgBouncer(adapter)))\n",
        ),
        "postgresql" | "postgres" => out.push_str(
            "    let adapter = ::dinoco::PostgresAdapter::direct(database_url).await?;\n    Ok(::dinoco::DinocoClient::new(::dinoco::Backend::Postgres(adapter)))\n",
        ),
        "mysql" => out.push_str(
            "    let adapter = ::dinoco::MySqlAdapter::new(database_url);\n    Ok(::dinoco::DinocoClient::new(::dinoco::Backend::Mysql(adapter)))\n",
        ),
        _ => out.push_str(
            "    let adapter = <::dinoco::SqliteAdapter as ::dinoco::DinocoAdapter>::new(database_url).await.map_err(::dinoco::anyhow::Error::msg)?;\n    Ok(::dinoco::DinocoClient::new(::dinoco::Backend::Sqlite(adapter)))\n",
        ),
    }
    out.push_str("}\n");
    out
}

pub fn render_schema_snapshot(schema: &Schema) -> String {
    dinoco_formatter_like(schema)
}

fn rust_type(field: &ModelField, schema: &Schema) -> String {
    let base = if has_default_call(field, "uuid") {
        "::dinoco::Uuid".to_string()
    } else if has_default_call(field, "snowflake") {
        "::dinoco::Snowflake".to_string()
    } else {
        match field.ty.name.as_str() {
            "String" => "String".to_string(),
            "DateTime" => "::dinoco::chrono::DateTime<::dinoco::chrono::Utc>".to_string(),
            "Date" => "::dinoco::chrono::NaiveDate".to_string(),
            "Json" => "::dinoco::serde_json::Value".to_string(),
            "Boolean" => "bool".to_string(),
            "Integer" => "i64".to_string(),
            "Float" => "f64".to_string(),
            other => other.to_string(),
        }
    };

    if field.ty.list {
        format!("Vec<{base}>")
    } else if field.ty.optional {
        format!("Option<{base}>")
    } else if schema.models().any(|model| model.name == field.ty.name) {
        base
    } else {
        base
    }
}

fn has_default_call(field: &ModelField, name: &str) -> bool {
    field.attributes.iter().find(|attr| attr.name == "default").and_then(|attr| attr.arguments.first()).is_some_and(
        |argument| {
            matches!(
                argument,
                dinoco_compiler::AttributeArgument::Value(AttributeValue::Call { name: call_name, .. })
                    if call_name == name
            )
        },
    )
}

fn field_attributes(model: &Model, field: &ModelField, schema: &Schema) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut dinoco_attrs = Vec::new();

    if field.attributes.iter().any(|attr| attr.name == "id") {
        dinoco_attrs.push("primary_key".to_string());
    }

    if let Some(default) = field.attributes.iter().find(|attr| attr.name == "default") {
        if let Some(value) = default.arguments.first() {
            match value {
                dinoco_compiler::AttributeArgument::Value(AttributeValue::Call { name, .. }) if name == "uuid" => {
                    dinoco_attrs.push("auto_generate = uuid".to_string());
                }
                dinoco_compiler::AttributeArgument::Value(AttributeValue::Call { name, .. }) if name == "snowflake" => {
                    dinoco_attrs.push("auto_generate = snowflake".to_string());
                }
                dinoco_compiler::AttributeArgument::Value(AttributeValue::Call { name, .. })
                    if name == "autoincrement" =>
                {
                    dinoco_attrs.push("auto_generate = autoincrement".to_string());
                }
                dinoco_compiler::AttributeArgument::Value(AttributeValue::Call { name, .. }) if name == "now" => {
                    if field.ty.name == "Date" {
                        dinoco_attrs.push("default = ::dinoco::chrono::Utc::now().date_naive()".to_string());
                    } else {
                        dinoco_attrs.push("default = ::dinoco::chrono::Utc::now()".to_string());
                    }
                }
                dinoco_compiler::AttributeArgument::Value(AttributeValue::Ident(value))
                    if value == "true" || value == "false" =>
                {
                    dinoco_attrs.push(format!("default = {value}"));
                }
                dinoco_compiler::AttributeArgument::Value(AttributeValue::Ident(value)) if field.is_enum(schema) => {
                    dinoco_attrs.push(format!("default = {}::{}", field.ty.name, to_pascal_case(value)));
                }
                dinoco_compiler::AttributeArgument::Value(AttributeValue::Ident(value)) => {
                    dinoco_attrs.push(format!("default = {value}"));
                }
                dinoco_compiler::AttributeArgument::Value(AttributeValue::String(value)) => {
                    dinoco_attrs
                        .push(format!("default = ::std::string::String::from(\"{}\")", escape_rust_string(value)));
                }
                _ => {}
            }
        }
    }

    if field.is_relation(schema) {
        let relation = field.attributes.iter().find(|attr| attr.name == "relation");
        let fields = relation.and_then(|relation| relation.argument("fields")).and_then(first_array_ident);
        let references = relation.and_then(|relation| relation.argument("references")).and_then(first_array_ident);
        let (relation_kind, foreign_key, relation_reference) = if field.ty.list {
            if let (Some(parent_field), Some(child_field)) = (fields, references) {
                ("one_to_many", Some(child_field), Some(parent_field))
            } else if let Some((child_field, parent_field)) = inverse_relation_fields(model, field, schema) {
                ("one_to_many", Some(child_field), Some(parent_field))
            } else {
                ("many_to_many", None, None)
            }
        } else if relation_is_unique(model, field, fields.as_deref()) {
            ("one_to_one", fields, references)
        } else {
            ("many_to_one", fields, references)
        };

        dinoco_attrs.push(relation_kind.to_string());
        if let Some(name) = relation.and_then(relation_name) {
            dinoco_attrs.push(format!("relation_name = \"{name}\""));
        }
        if let (Some(foreign_key), Some(references)) = (foreign_key, relation_reference) {
            dinoco_attrs.push(format!("foreign_key = \"{foreign_key}\""));
            dinoco_attrs.push(format!("references = \"{references}\""));
        }
    }

    if !dinoco_attrs.is_empty() {
        attrs.push(format!("#[dinoco({})]", dinoco_attrs.join(", ")));
    }

    attrs
}

fn inverse_relation_fields(model: &Model, field: &ModelField, schema: &Schema) -> Option<(String, String)> {
    let target = schema.models().find(|candidate| candidate.name == field.ty.name)?;
    let expected_name = field.attributes.iter().find(|attr| attr.name == "relation").and_then(relation_name);
    let mut candidates = target.fields.iter().filter_map(|candidate| {
        if candidate.ty.list || candidate.ty.name != model.name {
            return None;
        }
        let relation = candidate.attributes.iter().find(|attr| attr.name == "relation")?;
        if relation_name(relation) != expected_name {
            return None;
        }
        let child_field = relation.argument("fields").and_then(first_array_ident)?;
        let parent_field = relation.argument("references").and_then(first_array_ident)?;
        Some((child_field, parent_field))
    });

    let candidate = candidates.next()?;
    candidates.next().is_none().then_some(candidate)
}

fn relation_is_unique(model: &Model, field: &ModelField, foreign_key: Option<&str>) -> bool {
    field.attributes.iter().any(|attr| attr.name == "unique")
        || foreign_key.is_some_and(|foreign_key| {
            model
                .fields
                .iter()
                .find(|candidate| candidate.name == foreign_key)
                .is_some_and(|candidate| candidate.attributes.iter().any(|attr| attr.name == "unique"))
        })
}

fn escape_rust_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn first_array_ident(value: &AttributeValue) -> Option<String> {
    let AttributeValue::Array(values) = value else {
        return None;
    };

    values.first().and_then(|value| match value {
        AttributeValue::Ident(value) => Some(value.clone()),
        _ => None,
    })
}

fn relation_name(attribute: &dinoco_compiler::Attribute) -> Option<String> {
    attribute.argument("name").and_then(string_or_ident).or_else(|| {
        attribute.arguments.iter().find_map(|argument| match argument {
            dinoco_compiler::AttributeArgument::Value(value) => string_or_ident(value),
            _ => None,
        })
    })
}

fn string_or_ident(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String(value) | AttributeValue::Ident(value) => Some(value.clone()),
        _ => None,
    }
}

fn dinoco_formatter_like(schema: &Schema) -> String {
    let mut out = String::new();
    if let Some(config) = schema.config() {
        out.push_str("config {\n");
        for entry in &config.entries {
            out.push_str("    ");
            out.push_str(&entry.key);
            out.push_str(" = ");
            out.push_str(match &entry.value {
                ConfigValue::String(value) | ConfigValue::Ident(value) => value,
                ConfigValue::Env(value) => value,
                ConfigValue::Array(_) => "[]",
            });
            out.push('\n');
        }
        out.push_str("}\n\n");
    }
    out
}

fn to_pascal_case(value: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in value.chars() {
        if ch == '_' || ch == '-' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_snake_case(value: &str) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[allow(dead_code)]
fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}
