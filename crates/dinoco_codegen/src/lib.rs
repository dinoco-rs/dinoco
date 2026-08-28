use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use dinoco_compiler::{AttributeValue, ConfigValue, Model, ModelField, Schema};

pub fn generate_models(schema: &Schema) -> anyhow::Result<()> {
    generate_models_for_workspace(schema, None)
}

pub fn generate_models_for_workspace(schema: &Schema, workspace: Option<&str>) -> anyhow::Result<()> {
    let marker = Path::new("dinoco/.models-workspace");
    let generated_workspace = fs::read_to_string(marker).ok();
    let requested_workspace = workspace.unwrap_or("");
    let workspace_changed = generated_workspace
        .as_deref()
        .map(|generated| generated.trim() != requested_workspace)
        .unwrap_or(workspace.is_some());
    if workspace_changed {
        let models = Path::new("dinoco/models");
        if models.exists() {
            fs::remove_dir_all(models)?;
        }
        let generated_mod = Path::new("dinoco/mod.rs");
        if generated_mod.exists() {
            fs::remove_file(generated_mod)?;
        }
        let stale_flat_file = Path::new("dinoco/models.rs");
        if stale_flat_file.exists() {
            fs::remove_file(stale_flat_file)?;
        }
    }

    fs::create_dir_all("dinoco/models")?;
    let stale_flat_file = Path::new("dinoco/models.rs");
    if stale_flat_file.exists() {
        fs::remove_file(stale_flat_file)?;
    }

    fs::write("dinoco/models/mod.rs", render_models_mod(schema))?;
    for model in schema.models() {
        fs::write(format!("dinoco/models/{}.rs", to_snake_case(&model.name)), render_model_file(model, schema))?;
    }
    for join in implicit_many_to_many_joins(schema) {
        let legacy_join_file = format!("dinoco/models/{}.rs", to_snake_case(&join.rust_name));
        if Path::new(&legacy_join_file).exists() {
            fs::remove_file(legacy_join_file)?;
        }
    }
    let migrations = runtime_migrations(workspace)?;
    fs::write("dinoco/mod.rs", render_dinoco_mod_with_migrations(schema, &migrations))?;
    fs::write(marker, requested_workspace)?;
    Ok(())
}

pub fn render_models(schema: &Schema) -> String {
    let mut out = String::new();
    out.push_str(&render_models_mod(schema));
    for model in schema.models() {
        out.push('\n');
        out.push_str(&render_model_file(model, schema));
    }
    out
}

pub fn render_models_mod(schema: &Schema) -> String {
    let mut out = String::new();
    push_custom_imports(&mut out, schema, "enum");
    for item in schema.enums() {
        let derives = merged_derives(
            &[
                "Debug",
                "Clone",
                "Copy",
                "PartialEq",
                "Eq",
                "Default",
                "::dinoco::serde::Serialize",
                "::dinoco::serde::Deserialize",
                "::dinoco::DinocoEnum",
            ],
            schema,
            "enum",
        );
        out.push_str(&format!("#[derive({})]\n", derives.join(", ")));
        out.push_str("#[serde(crate = \"::dinoco::serde\")]\n");
        out.push_str(&format!("pub enum {} {{\n", item.name));
        for (index, value) in item.values.iter().enumerate() {
            if index == 0 {
                out.push_str("    #[default]\n");
            }
            out.push_str(&format!("    #[dinoco(value = \"{}\")]\n", escape_rust_string(value)));
            out.push_str(&format!("    #[serde(rename = \"{}\")]\n", escape_rust_string(value)));
            out.push_str("    ");
            out.push_str(&to_pascal_case(value));
            out.push_str(",\n");
        }
        out.push_str("}\n\n");
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
    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use super::*;\n");
    out.push_str("use dinoco::Entity;\n");
    push_custom_imports(&mut out, schema, "struct");
    out.push('\n');
    let mut base = vec!["Debug", "Clone"];
    if model_supports_copy(model, schema) {
        base.push("Copy");
    }
    base.extend(["Entity", "::dinoco::serde::Serialize", "::dinoco::serde::Deserialize"]);
    let derives = merged_derives(&base, schema, "struct");
    out.push_str(&format!("#[derive({})]\n", derives.join(", ")));
    out.push_str("#[serde(crate = \"::dinoco::serde\")]\n");
    out.push_str(&format!("#[dinoco(table_name = \"{}\")]\n", escape_rust_string(&model_table_name(model))));
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
        out.push_str(&rust_type(model, field, schema));
        out.push_str(",\n\n");
    }
    for field in many_to_many_virtual_fields(model, schema) {
        out.push_str(&format!(
            "    #[dinoco(many_to_many_key, join_table = \"{}\", parent_field = \"{}\", join_parent_field = \"{}\", join_child_field = \"{}\")]\n",
            escape_rust_string(&field.join_table),
            escape_rust_string(&field.parent_field),
            escape_rust_string(&field.join_parent_field),
            escape_rust_string(&field.join_child_field),
        ));
        out.push_str(&format!("    pub {}: Option<{}>,\n\n", field.name, field.ty));
    }
    out.push_str("}\n");
    out
}

pub fn render_dinoco_mod(schema: &Schema) -> String {
    render_dinoco_mod_with_migrations(schema, &[])
}

fn render_dinoco_mod_with_migrations(schema: &Schema, migrations: &[(String, String)]) -> String {
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
    let with_logger = config
        .and_then(|config| config.entries.iter().find(|entry| entry.key == "with_logger"))
        .and_then(|entry| match &entry.value {
            ConfigValue::Boolean(value) => Some(*value),
            _ => None,
        })
        .unwrap_or(false);
    let min_connection = config_integer(config, "min_connection").unwrap_or(2);
    let max_connection = config_integer(config, "max_connection").unwrap_or(10);
    let read_replica_envs = config_env_array(config, "read_replicas");

    let mut out = String::from("#![allow(dead_code)]\n\n");
    out.push_str("pub mod models;\n\n");
    out.push_str("pub use models::*;\n\n");
    out.push_str("pub async fn connect() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {\n");
    out.push_str(&format!("    let database_url = std::env::var(\"{database_url_env}\")?;\n"));
    match database {
        "postgresql" | "postgres" if connection == "pgbouncer" => out.push_str(
            "    let adapter = ::dinoco::PgBouncerAdapter::new(database_url).await?;\n    let client = ::dinoco::DinocoClient::new(::dinoco::Backend::PgBouncer(adapter));\n",
        ),
        "postgresql" | "postgres" => out.push_str(&format!(
            "    let adapter = ::dinoco::PostgresAdapter::direct_with_pool(database_url, {min_connection}, {max_connection}).await?;\n    let client = ::dinoco::DinocoClient::new(::dinoco::Backend::Postgres(adapter));\n"
        )),
        "mysql" => out.push_str(
            "    let adapter = ::dinoco::MySqlAdapter::new(database_url);\n    let client = ::dinoco::DinocoClient::new(::dinoco::Backend::Mysql(adapter));\n",
        ),
        _ => out.push_str(
            "    let adapter = <::dinoco::SqliteAdapter as ::dinoco::DinocoAdapter>::new(database_url).await.map_err(::dinoco::anyhow::Error::msg)?;\n    let client = ::dinoco::DinocoClient::new(::dinoco::Backend::Sqlite(adapter));\n",
        ),
    }
    out.push_str("    let read_replicas = vec![\n");
    for env_name in read_replica_envs {
        let env_name = escape_rust_string(env_name);
        match database {
            "postgresql" | "postgres" if connection == "pgbouncer" => out.push_str(&format!(
                "        ::dinoco::Backend::PgBouncer(::dinoco::PgBouncerAdapter::new(std::env::var(\"{env_name}\")?).await?),\n"
            )),
            "postgresql" | "postgres" => out.push_str(&format!(
                "        ::dinoco::Backend::Postgres(::dinoco::PostgresAdapter::direct_with_pool(std::env::var(\"{env_name}\")?, {min_connection}, {max_connection}).await?),\n"
            )),
            "mysql" => out.push_str(&format!(
                "        ::dinoco::Backend::Mysql(::dinoco::MySqlAdapter::new(std::env::var(\"{env_name}\")?)),\n"
            )),
            _ => out.push_str(&format!(
                "        ::dinoco::Backend::Sqlite(<::dinoco::SqliteAdapter as ::dinoco::DinocoAdapter>::new(std::env::var(\"{env_name}\")?).await.map_err(::dinoco::anyhow::Error::msg)?),\n"
            )),
        }
    }
    out.push_str("    ];\n");
    out.push_str(&format!("    Ok(client.with_read_replicas(read_replicas).with_logger({with_logger}))\n"));
    out.push_str("}\n");
    out.push_str("\npub const MIGRATIONS: &[::dinoco::runtime::Migration<'static>] = &[\n");
    for (name, include_path) in migrations {
        out.push_str(&format!(
            "    ::dinoco::runtime::Migration::new(\"{}\", include_str!(\"{}\")),\n",
            escape_rust_string(name),
            escape_rust_string(include_path)
        ));
    }
    out.push_str(
        "];

pub async fn migrate(
    client: &::dinoco::DinocoClient,
) -> ::dinoco::anyhow::Result<::dinoco::runtime::MigrationReport> {
    ::dinoco::runtime::run_migrations(client, MIGRATIONS).await
}
",
    );
    out
}

fn config_integer(config: Option<&dinoco_compiler::ConfigBlock>, key: &str) -> Option<i64> {
    config?.entries.iter().find(|entry| entry.key == key).and_then(|entry| match &entry.value {
        ConfigValue::Integer(value) => Some(*value),
        _ => None,
    })
}

fn config_env_array<'a>(config: Option<&'a dinoco_compiler::ConfigBlock>, key: &str) -> Vec<&'a str> {
    config
        .and_then(|config| config.entries.iter().find(|entry| entry.key == key))
        .and_then(|entry| match &entry.value {
            ConfigValue::Array(values) => Some(values),
            _ => None,
        })
        .into_iter()
        .flatten()
        .filter_map(|value| match value {
            ConfigValue::Env(name) => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

fn merged_derives(base: &[&str], schema: &Schema, target: &str) -> Vec<String> {
    let mut derives = base.iter().map(|derive| (*derive).to_string()).collect::<Vec<_>>();
    let mut names =
        derives.iter().map(|derive| derive.rsplit("::").next().unwrap_or(derive).to_string()).collect::<BTreeSet<_>>();
    for custom in schema.custom_derives().filter(|custom| custom.into == target) {
        let short = custom.derive.rsplit("::").next().unwrap_or(&custom.derive);
        if names.insert(short.to_string()) {
            derives.push(custom.derive.clone());
        }
    }
    derives
}

fn push_custom_imports(out: &mut String, schema: &Schema, target: &str) {
    let imports = schema
        .custom_derives()
        .filter(|custom| custom.into == target)
        .map(|custom| custom.import.trim().trim_end_matches(';'))
        .filter(|import| !import.is_empty())
        .collect::<BTreeSet<_>>();
    for import in &imports {
        out.push_str(import);
        out.push_str(";\n");
    }
    if !out.is_empty() && !imports.is_empty() {
        out.push('\n');
    }
}

fn runtime_migrations(workspace: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
    let relative_root = workspace
        .map_or_else(|| Path::new("migrations").to_path_buf(), |workspace| Path::new("migrations").join(workspace));
    let root = Path::new("dinoco").join(&relative_root);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut migrations = fs::read_dir(&root)?
        .map(|entry| -> anyhow::Result<_> {
            let path = entry?.path();
            if !path.is_dir() {
                return Ok(None);
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow::anyhow!("invalid runtime migration directory name"))?
                .to_string();
            let up = path.join("up.sql");
            if !up.is_file() {
                anyhow::bail!("runtime migration `{name}` is missing {}", up.display());
            }
            let include_path = relative_root.join(&name).join("up.sql").to_string_lossy().replace('\\', "/");
            Ok(Some((name, include_path)))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    migrations.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(migrations)
}

pub fn render_schema_snapshot(schema: &Schema) -> String {
    dinoco_formatter_like(schema)
}

pub fn render_many_to_many_join_file(join: &ManyToManyJoin, schema: &Schema) -> String {
    let mut out = String::new();
    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use super::*;\n");
    out.push_str("use dinoco::Entity;\n\n");
    if many_to_many_join_supports_copy(join, schema) {
        out.push_str(
            "#[derive(Debug, Clone, Copy, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]\n",
        );
    } else {
        out.push_str("#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]\n");
    }
    out.push_str("#[serde(crate = \"::dinoco::serde\")]\n");
    out.push_str(&format!("#[dinoco(table_name = \"{}\")]\n", escape_rust_string(&join.table_name)));
    out.push_str(&format!("pub struct {} {{\n", join.rust_name));
    out.push_str("    #[dinoco(primary_key)]\n");
    out.push_str(&format!("    pub {}: {},\n\n", join.left_column, model_primary_rust_type(schema, &join.left_model)));
    out.push_str("    #[dinoco(primary_key)]\n");
    out.push_str(&format!(
        "    pub {}: {},\n\n",
        join.right_column,
        model_primary_rust_type(schema, &join.right_model)
    ));
    out.push_str("}\n");
    out
}

fn rust_type(model: &Model, field: &ModelField, schema: &Schema) -> String {
    let relation_default = referenced_relation_default(model, field, schema);
    let base = if has_default_call(field, "uuid") || relation_default == Some("uuid") {
        "::dinoco::Uuid".to_string()
    } else if has_default_call(field, "snowflake") || relation_default == Some("snowflake") {
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
    } else {
        base
    }
}

fn referenced_relation_default<'a>(model: &'a Model, field: &ModelField, schema: &'a Schema) -> Option<&'a str> {
    for relation_field in &model.fields {
        if !relation_field.is_relation(schema) || relation_field.ty.list {
            continue;
        }
        let Some(relation) = relation_field.attributes.iter().find(|attr| attr.name == "relation") else {
            continue;
        };
        let Some(local_fields) = relation.argument("fields").and_then(array_idents) else {
            continue;
        };
        let Some(references) = relation.argument("references").and_then(array_idents) else {
            continue;
        };

        for (local, reference) in local_fields.iter().zip(references.iter()) {
            if local != &field.name {
                continue;
            }

            let Some(target) = schema.models().find(|candidate| candidate.name == relation_field.ty.name) else {
                continue;
            };
            let Some(referenced) = target.fields.iter().find(|candidate| candidate.name == *reference) else {
                continue;
            };

            if has_default_call(referenced, "uuid") {
                return Some("uuid");
            }
            if has_default_call(referenced, "snowflake") {
                return Some("snowflake");
            }
        }
    }

    None
}

fn model_primary_rust_type(schema: &Schema, model_name: &str) -> String {
    let Some(model) = schema.models().find(|model| model.name == model_name) else {
        return "String".to_string();
    };
    let Some(field) = model.fields.iter().find(|field| is_primary_key_field(model, field)) else {
        return "String".to_string();
    };

    rust_type(model, field, schema)
}

fn model_supports_copy(model: &Model, schema: &Schema) -> bool {
    model.fields.iter().all(|field| field_supports_copy(field, schema))
}

fn field_supports_copy(field: &ModelField, schema: &Schema) -> bool {
    if field.ty.list || field.is_relation(schema) {
        return false;
    }

    match field.ty.name.as_str() {
        // `::dinoco::Uuid` is currently an alias for `String`, so UUID-backed
        // fields are cloneable but not copyable as well.
        "String" => false,
        "Json" => false,
        "Boolean" | "Integer" | "Float" | "DateTime" | "Date" => true,
        _ => field.is_enum(schema),
    }
}

fn many_to_many_join_supports_copy(join: &ManyToManyJoin, schema: &Schema) -> bool {
    [&join.left_model, &join.right_model].into_iter().all(|model_name| {
        let Some(model) = schema.models().find(|model| model.name == *model_name) else {
            return false;
        };
        let Some(field) = model.fields.iter().find(|field| is_primary_key_field(model, field)) else {
            return false;
        };

        field_supports_copy(field, schema)
    })
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

    if is_primary_key_field(model, field) {
        dinoco_attrs.push("primary_key".to_string());
    }

    // The derive cannot discover whether an arbitrary Rust path names an enum.
    // Preserve that schema information explicitly so `Option<MyEnum>` is not
    // mistaken for an optional relation by the generated Entity implementation.
    if field.is_enum(schema) {
        dinoco_attrs.push("enum".to_string());
    }

    if field.attributes.iter().any(|attr| attr.name == "fulltext") {
        dinoco_attrs.push("fulltext".to_string());
    } else if let Some(fields) = model
        .attributes("fulltexts")
        .filter_map(|attribute| attribute.field_names())
        .find(|fields| fields.contains(&field.name.as_str()))
    {
        dinoco_attrs.push(format!("fulltext = \"{}\"", fields.join(",")));
    }

    if let Some(default) = field.attributes.iter().find(|attr| attr.name == "default")
        && let Some(value) = default.arguments.first()
    {
        match value {
            dinoco_compiler::AttributeArgument::Value(AttributeValue::Call { name, .. }) if name == "uuid" => {
                dinoco_attrs.push("auto_generate = uuid".to_string());
            }
            dinoco_compiler::AttributeArgument::Value(AttributeValue::Call { name, .. }) if name == "snowflake" => {
                dinoco_attrs.push("auto_generate = snowflake".to_string());
            }
            dinoco_compiler::AttributeArgument::Value(AttributeValue::Call { name, .. }) if name == "autoincrement" => {
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
                let value = format!("{}::{}", field.ty.name, to_pascal_case(value));
                if field.ty.optional {
                    dinoco_attrs.push(format!("default = ::core::option::Option::Some({value})"));
                } else {
                    dinoco_attrs.push(format!("default = {value}"));
                }
            }
            dinoco_compiler::AttributeArgument::Value(AttributeValue::Ident(value)) => {
                dinoco_attrs.push(format!("default = {value}"));
            }
            dinoco_compiler::AttributeArgument::Value(AttributeValue::String(value)) => {
                dinoco_attrs.push(format!("default = ::std::string::String::from(\"{}\")", escape_rust_string(value)));
            }
            _ => {}
        }
    }

    if field.is_relation(schema) {
        let relation = field.attributes.iter().find(|attr| attr.name == "relation");
        let fields = relation.and_then(|relation| relation.argument("fields")).and_then(first_array_ident);
        let references = relation.and_then(|relation| relation.argument("references")).and_then(first_array_ident);
        let many_to_many = implicit_many_to_many_field(model, field, schema);
        let (relation_kind, foreign_key, relation_reference) = if field.ty.list {
            if let (Some(parent_field), Some(child_field)) = (fields, references) {
                ("one_to_many", Some(child_field), Some(parent_field))
            } else if let Some((child_field, parent_field)) = inverse_relation_fields(model, field, schema) {
                ("one_to_many", Some(child_field), Some(parent_field))
            } else if let Some(many_to_many) = &many_to_many {
                ("many_to_many", Some(many_to_many.child_field.clone()), Some(many_to_many.parent_field.clone()))
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
        if let Some(many_to_many) = many_to_many {
            dinoco_attrs.push(format!("join_table = \"{}\"", many_to_many.join_table));
            dinoco_attrs.push(format!("parent_field = \"{}\"", many_to_many.parent_field));
            dinoco_attrs.push(format!("join_parent_field = \"{}\"", many_to_many.join_parent_field));
            dinoco_attrs.push(format!("join_child_field = \"{}\"", many_to_many.join_child_field));
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

fn is_primary_key_field(model: &Model, field: &ModelField) -> bool {
    field.attributes.iter().any(|attribute| attribute.name == "id")
        || model
            .attribute("ids")
            .and_then(|attribute| attribute.field_names())
            .is_some_and(|fields| fields.contains(&field.name.as_str()))
}

fn model_table_name(model: &Model) -> String {
    model
        .attribute("table_name")
        .and_then(|attribute| attribute.arguments.first())
        .and_then(|argument| match argument {
            dinoco_compiler::AttributeArgument::Value(AttributeValue::String(value)) => Some(value.clone()),
            _ => None,
        })
        .unwrap_or_else(|| to_snake_case(&model.name))
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

fn array_idents(value: &AttributeValue) -> Option<Vec<String>> {
    let AttributeValue::Array(values) = value else {
        return None;
    };

    values
        .iter()
        .map(|value| match value {
            AttributeValue::Ident(value) | AttributeValue::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
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
            let value = match &entry.value {
                ConfigValue::String(value) | ConfigValue::Ident(value) => value.clone(),
                ConfigValue::Env(value) => value.clone(),
                ConfigValue::Array(_) | ConfigValue::Object(_) => "[]".to_string(),
                ConfigValue::Boolean(value) => value.to_string(),
                ConfigValue::Integer(value) => value.to_string(),
            };
            out.push_str(&value);
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
    let chars = value.chars().collect::<Vec<_>>();
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_ascii_uppercase() {
            let previous_is_lowercase_or_digit =
                index > 0 && (chars[index - 1].is_ascii_lowercase() || chars[index - 1].is_ascii_digit());
            let starts_word_after_acronym = index > 0
                && chars[index - 1].is_ascii_uppercase()
                && chars.get(index + 1).is_some_and(|next| next.is_ascii_lowercase());
            if previous_is_lowercase_or_digit || starts_word_after_acronym {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManyToManyJoin {
    pub rust_name: String,
    pub table_name: String,
    pub left_model: String,
    pub right_model: String,
    pub left_column: String,
    pub right_column: String,
}

#[derive(Debug, Clone)]
struct ManyToManyField {
    join_table: String,
    parent_field: String,
    child_field: String,
    join_parent_field: String,
    join_child_field: String,
}

#[derive(Debug, Clone)]
struct ManyToManyVirtualField {
    name: String,
    ty: String,
    join_table: String,
    parent_field: String,
    join_parent_field: String,
    join_child_field: String,
}

fn many_to_many_virtual_fields(model: &Model, schema: &Schema) -> Vec<ManyToManyVirtualField> {
    let mut used = model.fields.iter().map(|field| field.name.clone()).collect::<BTreeSet<_>>();
    let mut fields = Vec::new();

    for relation in &model.fields {
        let Some(metadata) = implicit_many_to_many_field(model, relation, schema) else {
            continue;
        };
        let Some(target) = schema.models().find(|target| target.name == relation.ty.name) else {
            continue;
        };

        let mut name = metadata.join_child_field.clone();
        if !used.insert(name.clone()) {
            name = format!("{}_id", to_snake_case(&relation.name));
            let mut suffix = 2usize;
            while !used.insert(name.clone()) {
                name = format!("{}_id_{suffix}", to_snake_case(&relation.name));
                suffix += 1;
            }
        }

        fields.push(ManyToManyVirtualField {
            name,
            ty: model_primary_rust_type(schema, &target.name),
            join_table: metadata.join_table,
            parent_field: metadata.parent_field,
            join_parent_field: metadata.join_parent_field,
            join_child_field: metadata.join_child_field,
        });
    }

    fields
}

fn implicit_many_to_many_field(model: &Model, field: &ModelField, schema: &Schema) -> Option<ManyToManyField> {
    if !field.ty.list || !field.is_relation(schema) {
        return None;
    }
    let relation = field.attributes.iter().find(|attr| attr.name == "relation");
    if relation.and_then(|attr| attr.argument("fields")).is_some() {
        return None;
    }

    let target = schema.models().find(|target| target.name == field.ty.name)?;
    let relation_label = relation.and_then(relation_name);
    let mut opposites = target.fields.iter().filter(|candidate| {
        (model.name != target.name || candidate.name != field.name)
            && candidate.ty.list
            && candidate.ty.name == model.name
            && candidate.attributes.iter().find(|attr| attr.name == "relation").and_then(relation_name)
                == relation_label
    });
    let opposite = opposites.next()?;
    if opposites.next().is_some() {
        return None;
    }

    let mut names = [model.name.as_str(), target.name.as_str()];
    names.sort();
    let join_table = many_to_many_table_name(names[0], names[1], relation_label.as_deref());
    let (join_parent_field, join_child_field) = if model.name != target.name {
        (format!("{}_id", to_snake_case(&model.name)), format!("{}_id", to_snake_case(&target.name)))
    } else if field.name.as_str() <= opposite.name.as_str() {
        ("a_id".to_string(), "b_id".to_string())
    } else {
        ("b_id".to_string(), "a_id".to_string())
    };

    Some(ManyToManyField {
        join_table,
        parent_field: model.fields.iter().find(|field| is_primary_key_field(model, field))?.name.clone(),
        child_field: target.fields.iter().find(|field| is_primary_key_field(target, field))?.name.clone(),
        join_parent_field,
        join_child_field,
    })
}

fn implicit_many_to_many_joins(schema: &Schema) -> Vec<ManyToManyJoin> {
    let mut seen = BTreeSet::new();
    let mut joins = Vec::new();
    let model_names = schema.models().map(|model| model.name.as_str()).collect::<BTreeSet<_>>();

    for model in schema.models() {
        for field in &model.fields {
            if !field.ty.list {
                continue;
            }
            let Some(target) = schema.models().find(|target| target.name == field.ty.name) else {
                continue;
            };
            if field
                .attributes
                .iter()
                .find(|attr| attr.name == "relation")
                .and_then(|attr| attr.argument("fields"))
                .is_some()
            {
                continue;
            }

            let relation_label = field.attributes.iter().find(|attr| attr.name == "relation").and_then(relation_name);
            let has_opposite = target.fields.iter().any(|candidate| {
                (model.name != target.name || candidate.name != field.name)
                    && candidate.ty.list
                    && candidate.ty.name == model.name
                    && candidate.attributes.iter().find(|attr| attr.name == "relation").and_then(relation_name)
                        == relation_label
            });
            if !has_opposite {
                continue;
            }

            let mut names = [model.name.as_str(), target.name.as_str()];
            names.sort();
            let key = relation_label
                .as_deref()
                .map(|name| format!("{}:{}:{name}", names[0], names[1]))
                .unwrap_or_else(|| format!("{}:{}", names[0], names[1]));
            if !seen.insert(key) {
                continue;
            }

            let table_name = many_to_many_table_name(names[0], names[1], relation_label.as_deref());
            let mut rust_name = format!("{}{}", names[0], names[1]);
            if let Some(label) = relation_label.as_deref() {
                rust_name.push_str(&to_pascal_case(label));
            }
            if model_names.contains(rust_name.as_str()) {
                rust_name.push_str("Relation");
            }

            joins.push(ManyToManyJoin {
                rust_name,
                table_name,
                left_model: names[0].to_string(),
                right_model: names[1].to_string(),
                left_column: if names[0] == names[1] {
                    "a_id".to_string()
                } else {
                    format!("{}_id", to_snake_case(names[0]))
                },
                right_column: if names[0] == names[1] {
                    "b_id".to_string()
                } else {
                    format!("{}_id", to_snake_case(names[1]))
                },
            });
        }
    }

    joins
}

fn many_to_many_table_name(left: &str, right: &str, relation_name: Option<&str>) -> String {
    let base = format!("_{}_to_{}", to_snake_case(left), to_snake_case(right));
    relation_name.map(|name| format!("{base}_{}", to_snake_case(name))).unwrap_or(base)
}

#[allow(dead_code)]
fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}
