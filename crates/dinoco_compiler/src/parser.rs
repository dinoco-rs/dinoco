use std::collections::{HashMap, HashSet};

use pest::Parser;
use pest::iterators::Pair;
use pest_derive::Parser;

use crate::ast::{
    Attribute, AttributeArgument, AttributeValue, ConfigBlock, ConfigEntry, ConfigImport, ConfigValue, CustomDerive,
    EnumDef, FieldType, Import, Model, ModelField, Schema, SchemaItem, SourceOrigin, WorkspaceConfig,
};
use crate::error::CompileError;

pub type CompileResult<T> = Result<T, CompileError>;

#[derive(Parser)]
#[grammar = "schema.pest"]
struct DinocoPestParser;

pub fn parse_schema(source: &str) -> CompileResult<Schema> {
    parse_schema_with_file(source, "schema.dinoco")
}

pub(crate) fn parse_schema_with_file(source: &str, file: &str) -> CompileResult<Schema> {
    let mut pairs =
        DinocoPestParser::parse(Rule::schema, source).map_err(|error| CompileError::from(error).with_file(file))?;
    let schema = pairs.next().expect("schema pair");
    let mut items = Vec::new();

    for pair in schema.into_inner() {
        match pair.as_rule() {
            Rule::import_statement => items.push(SchemaItem::Import(parse_import(pair, file)?)),
            Rule::config_block => items.push(SchemaItem::Config(parse_config(pair, file)?)),
            Rule::enum_block => items.push(SchemaItem::Enum(parse_enum(pair, file)?)),
            Rule::model_block => items.push(SchemaItem::Model(parse_model(pair, file)?)),
            Rule::EOI => {}
            _ => return Err(pair_error(&pair, "unexpected schema item")),
        }
    }

    Ok(Schema { items })
}

fn parse_import(pair: Pair<'_, Rule>, file: &str) -> CompileResult<Import> {
    let origin = pair_origin(&pair, file);
    let mut symbols = Vec::new();
    let mut path = None;
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::ident => symbols.push(item.as_str().to_string()),
            Rule::string_literal => path = Some(unquote(item.as_str())),
            _ => return Err(pair_error(&item, "unexpected import token").with_file(file)),
        }
    }
    Ok(Import { symbols, path: path.ok_or_else(|| CompileError::at("expected import path", &origin))?, origin })
}

fn parse_config(pair: Pair<'_, Rule>, file: &str) -> CompileResult<ConfigBlock> {
    let origin = pair_origin(&pair, file);
    let mut entries = Vec::new();
    let mut workspaces = Vec::new();

    for pair in pair.into_inner() {
        match pair.as_rule() {
            Rule::config_entry => entries.push(parse_config_entry(pair, file)?),
            Rule::workspace_block => {
                for workspace in pair.into_inner() {
                    if workspace.as_rule() == Rule::workspace_entry {
                        workspaces.push(parse_workspace(workspace, file)?);
                    }
                }
            }
            _ => return Err(pair_error(&pair, "unexpected config item")),
        }
    }

    let custom_derives = parse_custom_derives(&entries)?;
    let imports = parse_config_imports(&entries)?;
    Ok(ConfigBlock { entries, workspaces, custom_derives, imports, origin })
}

fn parse_workspace(pair: Pair<'_, Rule>, file: &str) -> CompileResult<WorkspaceConfig> {
    let origin = pair_origin(&pair, file);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected workspace name")?.as_str().to_string();
    let entries = inner
        .filter(|pair| pair.as_rule() == Rule::config_entry)
        .map(|entry| parse_config_entry(entry, file))
        .collect::<CompileResult<Vec<_>>>()?;
    Ok(WorkspaceConfig { name, entries, origin })
}

fn parse_config_entry(pair: Pair<'_, Rule>, file: &str) -> CompileResult<ConfigEntry> {
    let origin = pair_origin(&pair, file);
    let mut inner = pair.into_inner();
    let key = expect_rule(&mut inner, Rule::ident, "expected config key")?.as_str().to_string();
    let value = parse_config_value(expect_rule(&mut inner, Rule::config_value, "expected config value")?, file)?;

    Ok(ConfigEntry { key, value, origin })
}

fn parse_custom_derives(entries: &[ConfigEntry]) -> CompileResult<Vec<CustomDerive>> {
    let Some(entry) = entries.iter().find(|entry| entry.key == "custom_derives") else {
        return Ok(Vec::new());
    };
    let ConfigValue::Array(values) = &entry.value else {
        return Err(CompileError::at("`config.custom_derives` must be an array of objects", &entry.origin));
    };

    values
        .iter()
        .map(|value| {
            let ConfigValue::Object(properties) = value else {
                return Err(CompileError::at("Every `config.custom_derives` item must be an object", &entry.origin));
            };
            let mut keys = HashSet::new();
            for property in properties {
                if !keys.insert(property.key.as_str()) {
                    return Err(CompileError::at(
                        format!("Custom derive key `{}` is declared more than once", property.key),
                        &property.origin,
                    ));
                }
                if !matches!(property.key.as_str(), "into" | "derive" | "import") {
                    return Err(CompileError::at(
                        format!("Unknown custom derive key `{}`", property.key),
                        &property.origin,
                    ));
                }
            }
            let missing = ["into", "derive", "import"]
                .into_iter()
                .filter(|key| !keys.contains(key))
                .map(|key| format!("`{key}`"))
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(CompileError::at(
                    format!(
                        "Custom derive requires all three keys: `into`, `derive`, and `import`; missing {}",
                        missing.join(", ")
                    ),
                    properties.first().map_or(&entry.origin, |property| &property.origin),
                ));
            }
            let string_property = |key: &str| -> CompileResult<&str> {
                let property = properties.iter().find(|property| property.key == key).ok_or_else(|| {
                    CompileError::at(format!("Custom derive requires `{key} = \"...\"`"), &entry.origin)
                })?;
                match &property.value {
                    ConfigValue::String(value) if !value.trim().is_empty() => Ok(value),
                    _ => Err(CompileError::at(
                        format!("Custom derive `{key}` must be a non-empty string"),
                        &property.origin,
                    )),
                }
            };
            let into = string_property("into")?;
            if !matches!(into, "enum" | "struct") {
                let origin = &properties.iter().find(|property| property.key == "into").unwrap().origin;
                return Err(CompileError::at("Custom derive `into` must be `enum` or `struct`", origin));
            }
            let derive = string_property("derive")?;
            let derive_origin = &properties.iter().find(|property| property.key == "derive").unwrap().origin;
            if !valid_derive_path(derive) {
                return Err(CompileError::at(
                    "Custom derive `derive` must be a valid Rust derive path such as `ZodSchema` or `crate::ZodSchema`",
                    derive_origin,
                ));
            }
            let import = string_property("import")?;
            let import_origin = &properties.iter().find(|property| property.key == "import").unwrap().origin;
            let import_statement = import.trim().trim_end_matches(';').trim();
            if import.contains('\n')
                || import.contains('\r')
                || !import_statement.strip_prefix("use ").is_some_and(|path| !path.trim().is_empty())
            {
                return Err(CompileError::at(
                    "Custom derive `import` must be a single Rust `use ...` statement",
                    import_origin,
                ));
            }
            Ok(CustomDerive {
                into: into.to_string(),
                derive: derive.to_string(),
                import: import.to_string(),
                origin: properties
                    .first()
                    .map(|property| property.origin.clone())
                    .unwrap_or_else(|| entry.origin.clone()),
            })
        })
        .collect()
}

fn parse_config_imports(entries: &[ConfigEntry]) -> CompileResult<Vec<ConfigImport>> {
    let Some(entry) = entries.iter().find(|entry| entry.key == "imports") else {
        return Ok(Vec::new());
    };
    let ConfigValue::Array(values) = &entry.value else {
        return Err(CompileError::at("`config.imports` must be an array of non-empty file paths", &entry.origin));
    };

    let mut paths = HashSet::new();
    values
        .iter()
        .map(|value| {
            let ConfigValue::String(path) = value else {
                return Err(CompileError::at(
                    "Every `config.imports` item must be a non-empty string path",
                    &entry.origin,
                ));
            };
            if path.trim().is_empty() {
                return Err(CompileError::at("`config.imports` paths cannot be empty", &entry.origin));
            }
            if !paths.insert(path.as_str()) {
                return Err(CompileError::at(
                    format!("Schema path `{path}` is listed more than once in `config.imports`"),
                    &entry.origin,
                ));
            }
            Ok(ConfigImport { path: path.clone(), origin: entry.origin.clone() })
        })
        .collect()
}

fn valid_derive_path(value: &str) -> bool {
    let value = value.trim().strip_prefix("::").unwrap_or(value.trim());
    !value.is_empty()
        && value.split("::").all(|segment| {
            let mut characters = segment.chars();
            characters.next().is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
                && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
        })
}

pub(crate) fn validate_schema(schema: &Schema) -> CompileResult<()> {
    validate_declarations(schema)?;
    validate_config_values(schema)?;
    validate_model_attributes(schema)?;
    validate_index_attributes(schema)?;
    validate_relation_attributes(schema)?;
    validate_relation_pairs(schema)?;
    validate_primary_keys(schema)?;

    let mut snowflake_origin = None;

    for model in schema.models() {
        for field in &model.fields {
            let Some(default) = field.attributes.iter().find(|attribute| attribute.name == "default") else {
                continue;
            };
            let Some(AttributeArgument::Value(value)) = default.arguments.first() else {
                continue;
            };

            if let AttributeValue::Call { name, .. } = value {
                match name.as_str() {
                    "uuid" if field.ty.name != "String" => {
                        return Err(CompileError::at(
                            "uuid() defaults are only supported for String fields",
                            &default.origin,
                        ));
                    }
                    "snowflake" if field.ty.name != "Integer" => {
                        return Err(CompileError::at(
                            "snowflake() defaults are only supported for Integer fields",
                            &default.origin,
                        ));
                    }
                    "autoincrement" if field.ty.name != "Integer" => {
                        return Err(CompileError::at(
                            "autoincrement() defaults are only supported for Integer fields",
                            &default.origin,
                        ));
                    }
                    "now" if field.ty.name != "DateTime" && field.ty.name != "Date" => {
                        return Err(CompileError::at(
                            "now() defaults are only supported for DateTime or Date fields",
                            &default.origin,
                        ));
                    }
                    "snowflake" => snowflake_origin = Some(&default.origin),
                    "uuid" | "autoincrement" | "now" => {}
                    _ => {
                        return Err(CompileError::at(
                            "unsupported @default() function. Supported: autoincrement(), uuid(), snowflake(), now()",
                            &default.origin,
                        ));
                    }
                }
            }
        }
    }

    if let Some(origin) = snowflake_origin {
        let configs = schema.config().into_iter().flat_map(config_entry_scopes).collect::<Vec<_>>();
        if configs.is_empty() {
            return Err(CompileError::at("snowflake() requires config.snowflake_node_id = env(\"...\")", origin));
        }
        for (scope, entries) in configs {
            let has_node_id = entries
                .iter()
                .any(|entry| entry.key == "snowflake_node_id" && matches!(entry.value, ConfigValue::Env(_)));
            if !has_node_id {
                return Err(CompileError::at(
                    format!("snowflake() requires {scope}.snowflake_node_id = env(\"...\")"),
                    origin,
                ));
            }
        }
    }

    Ok(())
}

fn validate_declarations(schema: &Schema) -> CompileResult<()> {
    const SCALAR_TYPES: [&str; 7] = ["String", "Boolean", "Integer", "Float", "DateTime", "Date", "Json"];

    let configs = schema
        .items
        .iter()
        .filter_map(|item| match item {
            SchemaItem::Config(config) => Some(config),
            _ => None,
        })
        .collect::<Vec<_>>();
    if let [first, second, ..] = configs.as_slice() {
        return Err(CompileError::at("Only one `config` block is allowed", &second.origin)
            .with_related("first config block is here", &first.origin));
    }

    for config in configs {
        let mut keys = HashMap::new();
        for entry in &config.entries {
            if let Some(first) = keys.insert(entry.key.as_str(), &entry.origin) {
                return Err(CompileError::at(
                    format!("Config key `{}` is declared more than once", entry.key),
                    &entry.origin,
                )
                .with_related("first declaration is here", first));
            }
        }

        if config.entries.iter().any(|entry| !matches!(entry.key.as_str(), "custom_derives" | "imports"))
            && !config.workspaces.is_empty()
        {
            return source_error(
                &config.origin,
                "A `config` block cannot mix top-level database settings with `workspace` settings; remove the top-level `database` and `database_url` entries",
            );
        }

        let mut workspace_names = HashMap::new();
        for workspace in &config.workspaces {
            if let Some(first) = workspace_names.insert(workspace.name.as_str(), &workspace.origin) {
                return Err(CompileError::at(
                    format!("Workspace `{}` is declared more than once", workspace.name),
                    &workspace.origin,
                )
                .with_related("first declaration is here", first));
            }
            let mut keys = HashMap::new();
            for entry in &workspace.entries {
                if let Some(first) = keys.insert(entry.key.as_str(), &entry.origin) {
                    return Err(CompileError::at(
                        format!(
                            "Config key `{}` is declared more than once in workspace `{}`",
                            entry.key, workspace.name
                        ),
                        &entry.origin,
                    )
                    .with_related("first declaration is here", first));
                }
            }
        }
    }

    let mut type_names = HashMap::new();
    for item in &schema.items {
        let declaration = match item {
            SchemaItem::Enum(item) => Some((item.name.as_str(), &item.origin)),
            SchemaItem::Model(item) => Some((item.name.as_str(), &item.origin)),
            SchemaItem::Import(_) | SchemaItem::Config(_) => None,
        };
        if let Some((name, origin)) = declaration {
            if SCALAR_TYPES.contains(&name) {
                return source_error(origin, format!("Type `{name}` conflicts with a built-in scalar type"));
            }
            if let Some(first) = type_names.insert(name, origin) {
                return Err(CompileError::at(format!("Type `{name}` is declared more than once"), origin)
                    .with_related("first declaration is here", first));
            }
        }
    }

    for item in schema.enums() {
        if item.values.is_empty() {
            return source_error(&item.origin, format!("Enum `{}` must contain at least one value", item.name));
        }
        let mut values = HashSet::new();
        for value in &item.values {
            if !values.insert(value.as_str()) {
                return source_error(
                    &item.origin,
                    format!("Enum value `{}.{value}` is declared more than once", item.name),
                );
            }
        }
    }

    for model in schema.models() {
        if model.fields.is_empty() {
            return source_error(&model.origin, format!("Model `{}` must contain at least one field", model.name));
        }
        let mut field_names = HashMap::new();
        for field in &model.fields {
            if is_rust_keyword(&field.name) {
                return source_error(
                    &field.origin,
                    format!(
                        "Field `{}.{}` conflicts with a reserved Rust keyword; rename it (for example, use `_{}`)",
                        model.name, field.name, field.name
                    ),
                );
            }
            if let Some(first) = field_names.insert(field.name.as_str(), &field.origin) {
                return Err(CompileError::at(
                    format!("Field `{}.{}` is declared more than once", model.name, field.name),
                    &field.origin,
                )
                .with_related("first declaration is here", first));
            }

            let scalar = SCALAR_TYPES.contains(&field.ty.name.as_str());
            let enum_type = schema.enums().any(|item| item.name == field.ty.name);
            let model_type = schema.models().any(|item| item.name == field.ty.name);
            if !scalar && !enum_type && !model_type {
                return Err(CompileError::at(
                    format!("Field `{}.{}` uses unknown type `{}`", model.name, field.name, field.ty.name),
                    &field.origin,
                ));
            }
            if field.ty.list && !model_type {
                return source_error(
                    &field.origin,
                    format!(
                        "Field `{}.{}` is a list of `{}`, but only model relation fields may be lists",
                        model.name, field.name, field.ty.name
                    ),
                );
            }
            if field.ty.list && field.ty.optional {
                return source_error(
                    &field.origin,
                    format!(
                        "Relation list `{}.{}` cannot be optional; remove `?` from the field type",
                        model.name, field.name
                    ),
                );
            }

            let mut attributes = HashMap::new();
            for attribute in &field.attributes {
                if matches!(attribute.name.as_str(), "id" | "unique" | "default" | "relation" | "index" | "fulltext")
                    && let Some(first) = attributes.insert(attribute.name.as_str(), &attribute.origin)
                {
                    return Err(CompileError::at(
                        format!("Field `{}.{}` declares @{} more than once", model.name, field.name, attribute.name),
                        &attribute.origin,
                    )
                    .with_related("first declaration is here", first));
                }
            }
        }
    }

    Ok(())
}

fn validate_index_attributes(schema: &Schema) -> CompileResult<()> {
    let mut mapped_names = HashMap::new();

    for model in schema.models() {
        let composite_indexes = model
            .attributes("indexes")
            .flat_map(|attribute| attribute.field_names().unwrap_or_default())
            .collect::<HashSet<_>>();
        let composite_fulltexts = model
            .attributes("fulltexts")
            .flat_map(|attribute| attribute.field_names().unwrap_or_default())
            .collect::<HashSet<_>>();
        if let Some(field) = composite_indexes.intersection(&composite_fulltexts).next() {
            return source_error(
                &model.origin,
                format!(
                    "Field `{}.{field}` cannot combine @index and @fulltext (including @@indexes/@@fulltexts) because both \
                     declare an index",
                    model.name
                ),
            );
        }

        for field in &model.fields {
            if let Some(fulltext) = field.attributes.iter().find(|attribute| attribute.name == "fulltext") {
                if field.ty.name != "String" || field.ty.list || field.is_relation(schema) {
                    return source_error(
                        &fulltext.origin,
                        format!(
                            "Field `{}.{}` uses @fulltext, but full-text search is only supported on String fields",
                            model.name, field.name
                        ),
                    );
                }
                if !fulltext.arguments.is_empty() {
                    return source_error(
                        &fulltext.origin,
                        format!("@fulltext on `{}.{}` does not accept arguments", model.name, field.name),
                    );
                }
                if field.attributes.iter().any(|attribute| attribute.name == "index")
                    || composite_indexes.contains(field.name.as_str())
                {
                    return source_error(
                        &field.origin,
                        format!(
                            "Field `{}.{}` cannot combine @index and @fulltext (including @@indexes/@@fulltexts) because \
                             both declare an index",
                            model.name, field.name
                        ),
                    );
                }
            }

            if composite_fulltexts.contains(field.name.as_str())
                && field.attributes.iter().any(|attribute| attribute.name == "index")
            {
                return source_error(
                    &field.origin,
                    format!(
                        "Field `{}.{}` cannot combine @index and @fulltext (including @@indexes/@@fulltexts) because both \
                         declare an index",
                        model.name, field.name
                    ),
                );
            }

            let Some(index) = field.attributes.iter().find(|attribute| attribute.name == "index") else {
                continue;
            };

            if field.is_relation(schema) || field.ty.list {
                return source_error(
                    &index.origin,
                    format!(
                        "Field `{}.{}` uses @index, but indexes must be declared on scalar or enum fields",
                        model.name, field.name
                    ),
                );
            }

            if index.arguments.len() > 1 {
                return source_error(
                    &index.origin,
                    format!("@index on `{}.{}` accepts only the optional `map` argument", model.name, field.name),
                );
            }

            let Some(argument) = index.arguments.first() else {
                continue;
            };
            let AttributeArgument::Named { key, value } = argument else {
                return source_error(
                    &index.origin,
                    format!("@index on `{}.{}` accepts only `map: \"index_name\"`", model.name, field.name),
                );
            };
            if key != "map" {
                return source_error(
                    &index.origin,
                    format!(
                        "@index on `{}.{}` does not support `{key}`; use `map: \"index_name\"`",
                        model.name, field.name
                    ),
                );
            }
            let Some(name) = attribute_string_or_ident(value) else {
                return source_error(
                    &index.origin,
                    format!("@index map on `{}.{}` must be a string or identifier", model.name, field.name),
                );
            };
            if name.trim().is_empty() {
                return source_error(
                    &index.origin,
                    format!("@index map on `{}.{}` cannot be empty", model.name, field.name),
                );
            }
            if let Some(first) = mapped_names.insert(name, &index.origin) {
                return Err(CompileError::at(format!("Index name `{name}` is declared more than once"), &index.origin)
                    .with_related("first declaration is here", first));
            }
        }
    }

    Ok(())
}

fn validate_model_attributes(schema: &Schema) -> CompileResult<()> {
    for model in schema.models() {
        let known = ["ids", "uniques", "indexes", "fulltexts", "table_name"];
        for attribute in &model.attributes {
            if !known.contains(&attribute.name.as_str()) {
                return source_error(
                    &attribute.origin,
                    format!("Unknown model attribute `@@{}` on model `{}`", attribute.name, model.name),
                );
            }
        }

        let ids = model.attributes("ids").collect::<Vec<_>>();
        if ids.len() > 1 {
            return Err(CompileError::at(
                format!("Model `{}` declares @@ids more than once; exactly one primary key is allowed", model.name),
                &ids[1].origin,
            )
            .with_related("first declaration is here", &ids[0].origin));
        }
        let table_names = model.attributes("table_name").collect::<Vec<_>>();
        if table_names.len() > 1 {
            return Err(CompileError::at(
                format!("Model `{}` declares @@table_name more than once", model.name),
                &table_names[1].origin,
            )
            .with_related("first declaration is here", &table_names[0].origin));
        }

        if let Some(table_name) = model.attribute("table_name") {
            let [AttributeArgument::Value(AttributeValue::String(name))] = table_name.arguments.as_slice() else {
                return source_error(
                    &table_name.origin,
                    format!("@@table_name on model `{}` requires exactly one non-empty string", model.name),
                );
            };
            if name.trim().is_empty() {
                return source_error(
                    &table_name.origin,
                    format!("@@table_name on model `{}` cannot be empty", model.name),
                );
            }
        }

        let mut fulltext_fields = model
            .fields
            .iter()
            .filter(|field| field.attributes.iter().any(|attribute| attribute.name == "fulltext"))
            .map(|field| field.name.as_str())
            .collect::<HashSet<_>>();
        let mut declarations = HashSet::new();

        for attribute in &model.attributes {
            if !matches!(attribute.name.as_str(), "ids" | "uniques" | "indexes" | "fulltexts") {
                continue;
            }

            let Some(fields) = attribute.field_names() else {
                return source_error(
                    &attribute.origin,
                    format!(
                        "@@{} on model `{}` requires exactly one array of field identifiers",
                        attribute.name, model.name
                    ),
                );
            };
            if fields.is_empty() {
                return source_error(
                    &attribute.origin,
                    format!("@@{} on model `{}` cannot be empty", attribute.name, model.name),
                );
            }

            let mut seen = HashSet::new();
            for name in &fields {
                if !seen.insert(*name) {
                    return source_error(
                        &attribute.origin,
                        format!("@@{} on model `{}` contains duplicate field `{name}`", attribute.name, model.name),
                    );
                }
                let Some(field) = model.fields.iter().find(|field| field.name == *name) else {
                    return source_error(
                        &attribute.origin,
                        format!("@@{} on model `{}` refers to missing field `{name}`", attribute.name, model.name),
                    );
                };
                if field.ty.list || field.is_relation(schema) {
                    return source_error(
                        &attribute.origin,
                        format!(
                            "@@{} on model `{}` may only contain scalar or enum fields; `{name}` is a relation/list",
                            attribute.name, model.name
                        ),
                    );
                }
                if attribute.name == "ids" && field.ty.optional {
                    return source_error(
                        &attribute.origin,
                        format!("Composite primary key field `{}.{name}` must be required", model.name),
                    );
                }
                if attribute.name == "fulltexts" && field.ty.name != "String" {
                    return source_error(
                        &attribute.origin,
                        format!(
                            "@@fulltexts on model `{}` may only contain String fields; `{name}` is `{}`",
                            model.name, field.ty.name
                        ),
                    );
                }
            }

            let signature = format!("{}:{}", attribute.name, fields.join(","));
            if !declarations.insert(signature) {
                return source_error(
                    &attribute.origin,
                    format!(
                        "Model `{}` declares @@{}([{}]) more than once",
                        model.name,
                        attribute.name,
                        fields.join(", ")
                    ),
                );
            }

            if attribute.name == "fulltexts" {
                for name in fields {
                    if !fulltext_fields.insert(name) {
                        return source_error(
                            &attribute.origin,
                            format!("Field `{}.{name}` belongs to more than one full-text index", model.name),
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_primary_keys(schema: &Schema) -> CompileResult<()> {
    for model in schema.models() {
        let field_ids = model
            .fields
            .iter()
            .filter(|field| field.attributes.iter().any(|attribute| attribute.name == "id"))
            .collect::<Vec<_>>();
        let primary_key_declarations = field_ids.len() + model.attributes("ids").count();

        match primary_key_declarations {
            0 => {
                return source_error(
                    &model.origin,
                    format!("Model `{}` must declare exactly one primary key using @id or @@ids([...])", model.name),
                );
            }
            1 => {}
            _ => {
                return source_error(
                    &model.origin,
                    format!(
                        "Model `{}` declares multiple primary keys; use exactly one @id or one @@ids([...])",
                        model.name
                    ),
                );
            }
        }

        if let [field] = field_ids.as_slice()
            && (field.ty.optional || field.ty.list || field.is_relation(schema))
        {
            return source_error(
                &field.origin,
                format!("Primary key `{}.{}` must be a required scalar or enum field", model.name, field.name),
            );
        }
    }

    Ok(())
}

fn validate_config_values(schema: &Schema) -> CompileResult<()> {
    for config in schema.items.iter().filter_map(|item| match item {
        SchemaItem::Config(config) => Some(config),
        _ => None,
    }) {
        let database_entries = config
            .entries
            .iter()
            .filter(|entry| !matches!(entry.key.as_str(), "custom_derives" | "imports"))
            .cloned()
            .collect::<Vec<_>>();
        if !database_entries.is_empty() {
            validate_config_scope("config", &database_entries)?;
        }
        for workspace in &config.workspaces {
            if let Some(entry) =
                workspace.entries.iter().find(|entry| matches!(entry.key.as_str(), "custom_derives" | "imports"))
            {
                return Err(CompileError::at(
                    format!("`{}` must be declared at the top level of `config`", entry.key),
                    &entry.origin,
                ));
            }
            validate_config_scope(&format!("config.workspace.{}", workspace.name), &workspace.entries)?;
        }
    }

    Ok(())
}

fn validate_config_scope(scope: &str, entries: &[ConfigEntry]) -> CompileResult<()> {
    for entry in entries {
        match (entry.key.as_str(), &entry.value) {
            ("database_url", ConfigValue::Env(name)) if !name.trim().is_empty() => {}
            ("database_url", ConfigValue::Env(_)) => {
                return schema_error(format!("`{scope}.database_url` env name cannot be empty"));
            }
            ("database_url", _) => {
                return schema_error(format!("`{scope}.database_url` only accepts env(\"DATABASE_URL\")"));
            }
            ("read_replicas", ConfigValue::Array(values))
                if values.iter().all(|value| matches!(value, ConfigValue::Env(name) if !name.trim().is_empty())) => {}
            ("read_replicas", ConfigValue::Array(_)) => {
                return schema_error(format!("`{scope}.read_replicas` only accepts non-empty env(...) values"));
            }
            ("read_replicas", _) => {
                return schema_error(format!("`{scope}.read_replicas` must be an array of env(...) values"));
            }
            ("snowflake_node_id", ConfigValue::Env(name)) if !name.trim().is_empty() => {}
            ("snowflake_node_id", ConfigValue::Env(_)) => {
                return schema_error(format!("`{scope}.snowflake_node_id` env name cannot be empty"));
            }
            ("snowflake_node_id", _) => {
                return schema_error(format!("`{scope}.snowflake_node_id` only accepts env(...)"));
            }
            ("with_logger", ConfigValue::Boolean(_)) => {}
            ("with_logger", _) => {
                return schema_error(format!("`{scope}.with_logger` must be `true` or `false`"));
            }
            ("min_connection" | "max_connection", ConfigValue::Integer(value)) if *value > 0 => {}
            ("min_connection" | "max_connection", _) => {
                return schema_error(format!("`{scope}.{}` must be a positive integer", entry.key));
            }
            _ => {}
        }
    }

    let database = entries
        .iter()
        .find(|entry| entry.key == "database")
        .ok_or_else(|| CompileError::new(format!("`{scope}.database` is required"), 1, 1))?;
    let database = match &database.value {
        ConfigValue::String(value) | ConfigValue::Ident(value)
            if matches!(value.as_str(), "postgresql" | "postgres" | "mysql" | "sqlite") =>
        {
            value.as_str()
        }
        _ => {
            return schema_error(format!("`{scope}.database` must be `postgresql`, `mysql`, or `sqlite`"));
        }
    };

    if !entries.iter().any(|entry| entry.key == "database_url") {
        return schema_error(format!("`{scope}.database_url = env(\"DATABASE_URL\")` is required"));
    }

    let connection = entries.iter().find(|entry| entry.key == "connection");
    let connection = match connection.map(|entry| &entry.value) {
        None => "direct",
        Some(ConfigValue::String(value) | ConfigValue::Ident(value))
            if matches!(value.as_str(), "direct" | "pgbouncer") =>
        {
            value
        }
        Some(_) => {
            return schema_error(format!("`{scope}.connection` must be `direct` or `pgbouncer`"));
        }
    };

    let min_connection = config_positive_integer(entries, "min_connection").unwrap_or(2);
    let max_connection = config_positive_integer(entries, "max_connection").unwrap_or(10);
    let configures_pool = entries.iter().any(|entry| matches!(entry.key.as_str(), "min_connection" | "max_connection"));
    if configures_pool && !(matches!(database, "postgresql" | "postgres") && connection == "direct") {
        return schema_error(format!(
            "`{scope}.min_connection` and `{scope}.max_connection` are supported only for PostgreSQL with `connection = \"direct\"`"
        ));
    }
    if min_connection > max_connection {
        return schema_error(format!(
            "`{scope}.min_connection` ({min_connection}) cannot be greater than `{scope}.max_connection` ({max_connection})"
        ));
    }

    Ok(())
}

fn config_positive_integer(entries: &[ConfigEntry], key: &str) -> Option<usize> {
    entries.iter().find(|entry| entry.key == key).and_then(|entry| match &entry.value {
        ConfigValue::Integer(value) if *value > 0 => usize::try_from(*value).ok(),
        _ => None,
    })
}

fn config_entry_scopes(config: &ConfigBlock) -> Vec<(String, &[ConfigEntry])> {
    if config.workspaces.is_empty() {
        vec![("config".to_string(), config.entries.as_slice())]
    } else {
        config
            .workspaces
            .iter()
            .map(|workspace| (format!("config.workspace.{}", workspace.name), workspace.entries.as_slice()))
            .collect()
    }
}

#[derive(Debug, Clone)]
struct RelationDefinition {
    name: Option<String>,
    fields: Option<Vec<String>>,
    references: Option<Vec<String>>,
    has_map: bool,
    actions: Vec<(String, String)>,
}

impl RelationDefinition {
    fn has_keys(&self) -> bool {
        self.fields.is_some()
    }
}

fn validate_relation_attributes(schema: &Schema) -> CompileResult<()> {
    for model in schema.models() {
        for field in &model.fields {
            let relation_count = field.attributes.iter().filter(|attribute| attribute.name == "relation").count();
            if relation_count > 1 {
                return source_error(
                    &field.origin,
                    format!("Relation field `{}.{}` declares @relation more than once", model.name, field.name),
                );
            }
            if field.is_relation(schema) && !field.ty.list && !field.ty.optional {
                return source_error(
                    &field.origin,
                    format!(
                        "Singular relation `{}.{}` must be optional; use `{}?` because relation fields are unloaded by default",
                        model.name, field.name, field.ty.name
                    ),
                );
            }
            if relation_count == 0 {
                continue;
            }
            if !field.is_relation(schema) {
                return source_error(
                    &field.origin,
                    format!(
                        "Field `{}.{}` uses @relation, but `{}` is not a model",
                        model.name, field.name, field.ty.name
                    ),
                );
            }

            relation_definition(model, field)?;
        }
    }

    Ok(())
}

fn validate_relation_pairs(schema: &Schema) -> CompileResult<()> {
    for model in schema.models() {
        for (field_index, field) in model.fields.iter().enumerate() {
            if !field.is_relation(schema) {
                continue;
            }

            let target =
                schema.models().find(|candidate| candidate.name == field.ty.name).expect("relation targets are models");
            let definition = relation_definition(model, field)?;
            let relation_name = definition.name.as_deref();
            let candidates = target
                .fields
                .iter()
                .enumerate()
                .filter(|(candidate_index, candidate)| {
                    candidate.ty.name == model.name
                        && (model.name != target.name || *candidate_index != field_index)
                        && field_relation_name(candidate).as_deref() == relation_name
                })
                .map(|(_, candidate)| candidate)
                .collect::<Vec<_>>();

            match candidates.as_slice() {
                [opposite] => {
                    let opposite_definition = relation_definition(target, opposite)?;
                    validate_relation_cardinality(
                        schema,
                        model,
                        field,
                        &definition,
                        target,
                        opposite,
                        &opposite_definition,
                    )?;
                }
                [] => {
                    let relation_suffix =
                        relation_name.map(|name| format!(" using @relation(name: \"{name}\")")).unwrap_or_default();
                    return source_error(
                        &field.origin,
                        format!(
                            "Relation field `{}.{}` targets model `{}`{relation_suffix}, but `{}` has no compatible \
                             opposite relation field pointing back to `{}`",
                            model.name, field.name, target.name, target.name, model.name
                        ),
                    );
                }
                _ => {
                    return source_error(
                        &field.origin,
                        format!(
                            "Ambiguous relation field `{}.{}`: model `{}` has multiple possible opposite fields \
                             pointing back to `{}`; add matching @relation(name: \"...\") attributes to both sides",
                            model.name, field.name, target.name, model.name
                        ),
                    );
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_relation_cardinality(
    schema: &Schema,
    model: &Model,
    field: &ModelField,
    definition: &RelationDefinition,
    target: &Model,
    opposite: &ModelField,
    opposite_definition: &RelationDefinition,
) -> CompileResult<()> {
    if model.name == target.name && definition.name.is_none() {
        return source_error(
            &field.origin,
            format!(
                "Self relation `{}.{}` must declare the same non-empty @relation(name: \"...\") on both sides",
                model.name, field.name
            ),
        );
    }

    match (field.ty.list, opposite.ty.list) {
        (true, true) => {
            if definition.has_keys() || opposite_definition.has_keys() {
                return source_error(
                    &field.origin,
                    format!(
                        "Many-to-many relation `{}.{}` <-> `{}.{}` cannot declare fields/references; implicit join \
                         relations require two unmapped list fields",
                        model.name, field.name, target.name, opposite.name
                    ),
                );
            }
            reject_non_owning_options(model, field, definition)?;
            reject_non_owning_options(target, opposite, opposite_definition)?;
            validate_many_to_many_id(schema, model)?;
            if model.name != target.name {
                validate_many_to_many_id(schema, target)?;
            }
        }
        (true, false) => {
            validate_one_to_many_pair(schema, model, field, definition, target, opposite, opposite_definition)?
        }
        (false, true) => {
            validate_one_to_many_pair(schema, target, opposite, opposite_definition, model, field, definition)?
        }
        (false, false) => {
            validate_one_to_one_pair(schema, model, field, definition, target, opposite, opposite_definition)?
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_one_to_many_pair(
    schema: &Schema,
    list_model: &Model,
    list_field: &ModelField,
    list_definition: &RelationDefinition,
    owner_model: &Model,
    owner_field: &ModelField,
    owner_definition: &RelationDefinition,
) -> CompileResult<()> {
    if !owner_definition.has_keys() {
        return source_error(
            &owner_field.origin,
            format!(
                "One-to-many relation `{}.{}` <-> `{}.{}` requires fields/references on the singular FK-owning side \
                 `{}.{}`",
                list_model.name,
                list_field.name,
                owner_model.name,
                owner_field.name,
                owner_model.name,
                owner_field.name
            ),
        );
    }

    validate_owner_keys(schema, owner_model, owner_field, list_model, owner_definition)?;
    if relation_is_unique(owner_model, owner_field, owner_definition) {
        return source_error(
            &owner_field.origin,
            format!(
                "Relation `{}.{}` is unique, so its opposite `{}.{}` must be singular instead of a list",
                owner_model.name, owner_field.name, list_model.name, list_field.name
            ),
        );
    }

    reject_non_owning_options(list_model, list_field, list_definition)?;
    if list_definition.has_keys() {
        let list_fields = list_definition.fields.as_deref().expect("checked relation fields");
        let list_references = list_definition.references.as_deref().expect("checked relation references");
        let owner_fields = owner_definition.fields.as_deref().expect("checked owner fields");
        let owner_references = owner_definition.references.as_deref().expect("checked owner references");
        if list_fields != owner_references || list_references != owner_fields {
            return source_error(
                &list_field.origin,
                format!(
                    "List relation `{}.{}` must mirror the owning side: expected fields: [{}], references: [{}]",
                    list_model.name,
                    list_field.name,
                    owner_references.join(", "),
                    owner_fields.join(", ")
                ),
            );
        }
        validate_key_field_names(schema, list_model, list_field, owner_model, list_definition, false)?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_one_to_one_pair(
    schema: &Schema,
    model: &Model,
    field: &ModelField,
    definition: &RelationDefinition,
    target: &Model,
    opposite: &ModelField,
    opposite_definition: &RelationDefinition,
) -> CompileResult<()> {
    let (owner_model, owner_field, owner_definition, inverse_model, inverse_field, inverse_definition) = match (
        definition.has_keys(),
        opposite_definition.has_keys(),
    ) {
        (true, false) => (model, field, definition, target, opposite, opposite_definition),
        (false, true) => (target, opposite, opposite_definition, model, field, definition),
        (false, false) => {
            return source_error(
                &field.origin,
                format!(
                    "One-to-one relation `{}.{}` <-> `{}.{}` requires fields/references on exactly one FK-owning side",
                    model.name, field.name, target.name, opposite.name
                ),
            );
        }
        (true, true) => {
            return source_error(
                &field.origin,
                format!(
                    "One-to-one relation `{}.{}` <-> `{}.{}` declares fields/references on both sides; only one side \
                         may own the foreign key",
                    model.name, field.name, target.name, opposite.name
                ),
            );
        }
    };

    validate_owner_keys(schema, owner_model, owner_field, inverse_model, owner_definition)?;
    reject_non_owning_options(inverse_model, inverse_field, inverse_definition)?;
    if owner_field.attributes.iter().any(|attribute| attribute.name == "unique")
        && owner_definition.fields.as_ref().is_some_and(|fields| fields.len() != 1)
    {
        return source_error(
            &owner_field.origin,
            format!(
                "Composite one-to-one relation `{}.{}` cannot place @unique on the relation field; declare \
                 @@uniques([...]) for the complete local foreign-key tuple",
                owner_model.name, owner_field.name
            ),
        );
    }
    if !relation_is_unique(owner_model, owner_field, owner_definition) {
        return source_error(
            &owner_field.origin,
            format!(
                "One-to-one relation `{}.{}` requires @unique on the relation field or its local foreign-key field",
                owner_model.name, owner_field.name
            ),
        );
    }
    Ok(())
}

fn validate_owner_keys(
    schema: &Schema,
    model: &Model,
    field: &ModelField,
    target: &Model,
    definition: &RelationDefinition,
) -> CompileResult<()> {
    let local_fields = validate_key_field_names(schema, model, field, target, definition, true)?;
    for (action_name, action) in &definition.actions {
        if action == "SetNull" && local_fields.iter().any(|local| !local.ty.optional) {
            return source_error(
                &field.origin,
                format!(
                    "{action_name}: SetNull on relation `{}.{}` requires every local foreign-key field to be optional",
                    model.name, field.name
                ),
            );
        }
        if action == "SetDefault"
            && local_fields.iter().any(|local| !local.attributes.iter().any(|attribute| attribute.name == "default"))
        {
            return source_error(
                &field.origin,
                format!(
                    "{action_name}: SetDefault on `{}.{}` requires every local foreign-key field to define @default(...)",
                    model.name, field.name
                ),
            );
        }
    }

    Ok(())
}

fn validate_key_field_names<'a>(
    schema: &Schema,
    model: &'a Model,
    field: &ModelField,
    target: &Model,
    definition: &RelationDefinition,
    require_unique_reference: bool,
) -> CompileResult<Vec<&'a ModelField>> {
    let fields = definition.fields.as_deref().expect("relation key fields were checked");
    let references = definition.references.as_deref().expect("relation key references were checked");
    if fields.len() != references.len() {
        return source_error(
            &field.origin,
            format!(
                "Relation `{}.{}` must have the same number of fields ({}) and references ({})",
                model.name,
                field.name,
                fields.len(),
                references.len()
            ),
        );
    }

    let mut local_fields = Vec::with_capacity(fields.len());
    for (local_name, reference_name) in fields.iter().zip(references) {
        let Some(local) = model.fields.iter().find(|candidate| candidate.name == *local_name) else {
            return source_error(
                &field.origin,
                format!(
                    "Relation `{}.{}` refers to missing local field `{}.{local_name}`",
                    model.name, field.name, model.name
                ),
            );
        };
        let Some(reference) = target.fields.iter().find(|candidate| candidate.name == *reference_name) else {
            return source_error(
                &field.origin,
                format!(
                    "Relation `{}.{}` refers to missing target field `{}.{reference_name}`",
                    model.name, field.name, target.name
                ),
            );
        };
        if local.ty.list || local.is_relation(schema) {
            return source_error(
                &local.origin,
                format!(
                    "Relation key `{}.{}` must be a scalar or enum field, not a relation/list field",
                    model.name, local.name
                ),
            );
        }
        if reference.ty.list || reference.is_relation(schema) {
            return source_error(
                &reference.origin,
                format!(
                    "Referenced key `{}.{}` must be a scalar or enum field, not a relation/list field",
                    target.name, reference.name
                ),
            );
        }
        if local.ty.name != reference.ty.name {
            return source_error(
                &field.origin,
                format!(
                    "Relation `{}.{}` has incompatible key types: `{}.{}` is `{}`, but `{}.{}` is `{}`",
                    model.name,
                    field.name,
                    model.name,
                    local.name,
                    local.ty.name,
                    target.name,
                    reference.name,
                    reference.ty.name
                ),
            );
        }
        local_fields.push(local);
    }

    if require_unique_reference && !model_has_unique_key(target, references) {
        return source_error(
            &field.origin,
            format!(
                "Relation `{}.{}` references `{}.[{}]`, which must declare @id or @unique, or match @@ids([...]) or \
                 @@uniques([...])",
                model.name,
                field.name,
                target.name,
                references.join(", ")
            ),
        );
    }

    Ok(local_fields)
}

fn reject_non_owning_options(model: &Model, field: &ModelField, definition: &RelationDefinition) -> CompileResult<()> {
    if definition.has_map || !definition.actions.is_empty() {
        return source_error(
            &field.origin,
            format!(
                "Relation `{}.{}` does not own a foreign key, so map/onDelete/onUpdate must be declared on the singular \
                 side with fields/references",
                model.name, field.name
            ),
        );
    }
    Ok(())
}

fn validate_many_to_many_id(schema: &Schema, model: &Model) -> CompileResult<()> {
    let ids = model
        .fields
        .iter()
        .filter(|field| field.attributes.iter().any(|attribute| attribute.name == "id"))
        .collect::<Vec<_>>();
    if ids.len() != 1 || ids[0].ty.list || ids[0].ty.optional || ids[0].is_relation(schema) {
        return source_error(
            &model.origin,
            format!(
                "Implicit many-to-many relations require model `{}` to have exactly one scalar @id field",
                model.name
            ),
        );
    }
    Ok(())
}

fn relation_is_unique(model: &Model, field: &ModelField, definition: &RelationDefinition) -> bool {
    field.attributes.iter().any(|attribute| attribute.name == "unique")
        || definition.fields.as_ref().is_some_and(|fields| model_has_unique_key(model, fields))
}

fn model_has_unique_key(model: &Model, fields: &[String]) -> bool {
    if fields.len() == 1
        && model.fields.iter().find(|field| field.name == fields[0]).is_some_and(|field| {
            field.attributes.iter().any(|attribute| matches!(attribute.name.as_str(), "id" | "unique"))
        })
    {
        return true;
    }
    if fields.iter().all(|name| {
        model.fields.iter().find(|field| field.name == *name).is_some_and(|field| {
            field.attributes.iter().any(|attribute| matches!(attribute.name.as_str(), "id" | "unique"))
        })
    }) {
        return true;
    }

    model
        .attributes
        .iter()
        .filter(|attribute| matches!(attribute.name.as_str(), "ids" | "uniques"))
        .filter_map(Attribute::field_names)
        .any(|candidate| candidate.iter().copied().eq(fields.iter().map(String::as_str)))
}

fn relation_definition(model: &Model, field: &ModelField) -> CompileResult<RelationDefinition> {
    let Some(relation) = field.attributes.iter().find(|attribute| attribute.name == "relation") else {
        return Ok(RelationDefinition {
            name: None,
            fields: None,
            references: None,
            has_map: false,
            actions: Vec::new(),
        });
    };

    let mut named = HashSet::new();
    let mut positional_name = None;
    for argument in &relation.arguments {
        match argument {
            AttributeArgument::Named { key, .. } => {
                if !matches!(key.as_str(), "name" | "fields" | "references" | "onDelete" | "onUpdate" | "map") {
                    return source_error(
                        &relation.origin,
                        format!("Unknown @relation argument `{key}` on `{}.{}`", model.name, field.name),
                    );
                }
                if !named.insert(key.as_str()) {
                    return source_error(
                        &relation.origin,
                        format!("Duplicate @relation argument `{key}` on `{}.{}`", model.name, field.name),
                    );
                }
            }
            AttributeArgument::Value(value) => {
                if positional_name.is_some() {
                    return source_error(
                        &relation.origin,
                        format!("Relation `{}.{}` accepts at most one positional name", model.name, field.name),
                    );
                }
                positional_name = Some(non_empty_string_or_ident(value, "relation name", model, field)?);
            }
        }
    }

    let named_name = relation
        .argument("name")
        .map(|value| non_empty_string_or_ident(value, "relation name", model, field))
        .transpose()?;
    if positional_name.is_some() && named_name.is_some() {
        return source_error(
            &relation.origin,
            format!("Relation `{}.{}` declares its name both positionally and with `name:`", model.name, field.name),
        );
    }
    let fields = relation
        .argument("fields")
        .map(|value| relation_identifier_array(value, "fields", model, field))
        .transpose()?;
    let references = relation
        .argument("references")
        .map(|value| relation_identifier_array(value, "references", model, field))
        .transpose()?;
    if fields.is_some() != references.is_some() {
        return source_error(
            &relation.origin,
            format!("Relation `{}.{}` must declare `fields` and `references` together", model.name, field.name),
        );
    }
    if fields.as_ref().is_some_and(|fields| fields.len() != references.as_ref().map_or(0, Vec::len)) {
        return source_error(
            &relation.origin,
            format!("Relation `{}.{}` must have the same number of fields and references", model.name, field.name),
        );
    }

    if let Some(value) = relation.argument("map") {
        non_empty_string_or_ident(value, "constraint map", model, field)?;
    }

    let mut actions = Vec::new();
    for action_name in ["onDelete", "onUpdate"] {
        let Some(value) = relation.argument(action_name) else {
            continue;
        };
        let action = non_empty_string_or_ident(value, action_name, model, field)?;
        if !matches!(action.as_str(), "Cascade" | "Restrict" | "NoAction" | "SetNull" | "SetDefault") {
            return source_error(
                &relation.origin,
                format!(
                    "{action_name} on `{}.{}` must be one of: Cascade, Restrict, NoAction, SetNull, SetDefault",
                    model.name, field.name
                ),
            );
        }
        actions.push((action_name.to_string(), action));
    }

    Ok(RelationDefinition {
        name: named_name.or(positional_name),
        fields,
        references,
        has_map: relation.argument("map").is_some(),
        actions,
    })
}

fn relation_identifier_array(
    value: &AttributeValue,
    argument: &str,
    model: &Model,
    field: &ModelField,
) -> CompileResult<Vec<String>> {
    let AttributeValue::Array(values) = value else {
        return source_error(
            &field.origin,
            format!(
                "@relation({argument}: ...) on `{}.{}` must be a non-empty array of field identifiers",
                model.name, field.name
            ),
        );
    };
    if values.is_empty() {
        return source_error(
            &field.origin,
            format!("@relation({argument}: ...) on `{}.{}` cannot be empty", model.name, field.name),
        );
    }

    let mut seen = HashSet::new();
    let mut names = Vec::with_capacity(values.len());
    for value in values {
        let AttributeValue::Ident(name) = value else {
            return source_error(
                &field.origin,
                format!("@relation({argument}: ...) on `{}.{}` only accepts field identifiers", model.name, field.name),
            );
        };
        if !seen.insert(name.as_str()) {
            return source_error(
                &field.origin,
                format!(
                    "@relation({argument}: ...) on `{}.{}` contains duplicate field `{name}`",
                    model.name, field.name
                ),
            );
        }
        names.push(name.clone());
    }
    Ok(names)
}

fn non_empty_string_or_ident(
    value: &AttributeValue,
    label: &str,
    model: &Model,
    field: &ModelField,
) -> CompileResult<String> {
    let Some(value) = attribute_string_or_ident(value) else {
        return source_error(
            &field.origin,
            format!("{label} on `{}.{}` must be a string or identifier", model.name, field.name),
        );
    };
    if value.trim().is_empty() {
        return source_error(&field.origin, format!("{label} on `{}.{}` cannot be empty", model.name, field.name));
    }
    Ok(value.to_string())
}

fn schema_error<T>(message: impl Into<String>) -> CompileResult<T> {
    Err(CompileError::new(message, 1, 1))
}

fn source_error<T>(origin: &SourceOrigin, message: impl Into<String>) -> CompileResult<T> {
    Err(CompileError::at(message, origin))
}

fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "Self"
            | "abstract"
            | "as"
            | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

fn field_relation_name(field: &ModelField) -> Option<String> {
    let relation = field.attributes.iter().find(|attribute| attribute.name == "relation")?;
    relation.argument("name").and_then(attribute_string_or_ident).map(str::to_string).or_else(|| {
        relation.arguments.iter().find_map(|argument| match argument {
            AttributeArgument::Value(value) => attribute_string_or_ident(value).map(str::to_string),
            AttributeArgument::Named { .. } => None,
        })
    })
}

fn attribute_string_or_ident(value: &AttributeValue) -> Option<&str> {
    match value {
        AttributeValue::String(value) | AttributeValue::Ident(value) => Some(value),
        _ => None,
    }
}

fn parse_config_value(pair: Pair<'_, Rule>, file: &str) -> CompileResult<ConfigValue> {
    let error = pair_position_error(&pair, "expected config value");
    let value = pair.into_inner().next().ok_or(error)?;

    match value.as_rule() {
        Rule::config_array => {
            let values =
                value.into_inner().map(|value| parse_config_value(value, file)).collect::<CompileResult<Vec<_>>>()?;
            Ok(ConfigValue::Array(values))
        }
        Rule::config_object => {
            let entries = value
                .into_inner()
                .filter(|pair| pair.as_rule() == Rule::config_entry)
                .map(|entry| parse_config_entry(entry, file))
                .collect::<CompileResult<Vec<_>>>()?;
            Ok(ConfigValue::Object(entries))
        }
        Rule::env_call => {
            let error = pair_position_error(&value, "expected env name");
            let raw = value.into_inner().find(|pair| pair.as_rule() == Rule::string_literal).ok_or(error)?;
            Ok(ConfigValue::Env(unquote(raw.as_str())))
        }
        Rule::string_literal => Ok(ConfigValue::String(unquote(value.as_str()))),
        Rule::boolean_literal => Ok(ConfigValue::Boolean(value.as_str() == "true")),
        Rule::number_literal => value
            .as_str()
            .parse::<i64>()
            .map(ConfigValue::Integer)
            .map_err(|_| pair_error(&value, "config numbers must be integers")),
        Rule::ident => Ok(ConfigValue::Ident(value.as_str().to_string())),
        _ => Err(pair_error(&value, "unexpected config value")),
    }
}

fn parse_enum(pair: Pair<'_, Rule>, file: &str) -> CompileResult<EnumDef> {
    let origin = pair_origin(&pair, file);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected enum name")?.as_str().to_string();
    let values = inner
        .filter(|pair| pair.as_rule() == Rule::enum_value)
        .map(|pair| {
            pair.into_inner()
                .next()
                .map(|pair| pair.as_str().to_string())
                .ok_or_else(|| CompileError::new("expected enum value", 1, 1))
        })
        .collect::<CompileResult<Vec<_>>>()?;

    Ok(EnumDef { name, values, origin })
}

fn parse_model(pair: Pair<'_, Rule>, file: &str) -> CompileResult<Model> {
    let origin = pair_origin(&pair, file);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected model name")?.as_str().to_string();
    let mut fields = Vec::new();
    let mut attributes = Vec::new();

    for pair in inner {
        match pair.as_rule() {
            Rule::model_field => fields.push(parse_model_field(pair, file)?),
            Rule::model_attribute => attributes.push(parse_attribute(pair, file)?),
            _ => return Err(pair_error(&pair, "unexpected model token")),
        }
    }

    Ok(Model { name, fields, attributes, origin })
}

fn parse_model_field(pair: Pair<'_, Rule>, file: &str) -> CompileResult<ModelField> {
    let origin = pair_origin(&pair, file);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected field name")?.as_str().to_string();
    let ty_name = expect_rule(&mut inner, Rule::field_type, "expected field type")?
        .into_inner()
        .next()
        .ok_or_else(|| CompileError::new("expected field type", 1, 1))?
        .as_str()
        .to_string();
    let mut optional = false;
    let mut list = false;
    let mut attributes = Vec::new();

    for pair in inner {
        match pair.as_rule() {
            Rule::field_optional => optional = true,
            Rule::field_list => list = true,
            Rule::attribute => attributes.push(parse_attribute(pair, file)?),
            _ => return Err(pair_error(&pair, "unexpected field token")),
        }
    }

    Ok(ModelField { name, ty: FieldType { name: ty_name, optional, list }, attributes, origin })
}

fn parse_attribute(pair: Pair<'_, Rule>, file: &str) -> CompileResult<Attribute> {
    let origin = pair_origin(&pair, file);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected attribute name")?.as_str().to_string();
    let arguments = inner
        .find(|pair| pair.as_rule() == Rule::attribute_arguments)
        .map(parse_attribute_arguments)
        .transpose()?
        .unwrap_or_default();

    Ok(Attribute { name, arguments, origin })
}

fn parse_attribute_arguments(pair: Pair<'_, Rule>) -> CompileResult<Vec<AttributeArgument>> {
    pair.into_inner().filter(|pair| pair.as_rule() == Rule::attribute_argument).map(parse_attribute_argument).collect()
}

fn parse_attribute_argument(pair: Pair<'_, Rule>) -> CompileResult<AttributeArgument> {
    let error = pair_position_error(&pair, "expected attribute argument");
    let pair = pair.into_inner().next().ok_or(error)?;

    match pair.as_rule() {
        Rule::named_argument => {
            let mut inner = pair.into_inner();
            let key = expect_rule(&mut inner, Rule::ident, "expected argument name")?.as_str().to_string();
            let value =
                parse_attribute_value(expect_rule(&mut inner, Rule::attribute_value, "expected argument value")?)?;

            Ok(AttributeArgument::Named { key, value })
        }
        Rule::attribute_value => Ok(AttributeArgument::Value(parse_attribute_value(pair)?)),
        _ => Err(pair_error(&pair, "unexpected attribute argument")),
    }
}

fn parse_attribute_value(pair: Pair<'_, Rule>) -> CompileResult<AttributeValue> {
    let error = pair_position_error(&pair, "expected attribute value");
    let pair = pair.into_inner().next().ok_or(error)?;

    match pair.as_rule() {
        Rule::attribute_array => {
            let values = pair
                .into_inner()
                .filter(|pair| pair.as_rule() == Rule::attribute_value)
                .map(parse_attribute_value)
                .collect::<CompileResult<Vec<_>>>()?;

            Ok(AttributeValue::Array(values))
        }
        Rule::attribute_call => {
            let mut inner = pair.into_inner();
            let name = expect_rule(&mut inner, Rule::ident, "expected function name")?.as_str().to_string();
            let arguments = inner
                .find(|pair| pair.as_rule() == Rule::attribute_arguments)
                .map(parse_attribute_arguments)
                .transpose()?
                .unwrap_or_default();

            Ok(AttributeValue::Call { name, arguments })
        }
        Rule::string_literal => Ok(AttributeValue::String(unquote(pair.as_str()))),
        Rule::number_literal | Rule::boolean_literal | Rule::ident => {
            Ok(AttributeValue::Ident(pair.as_str().to_string()))
        }
        _ => Err(pair_error(&pair, "unexpected attribute value")),
    }
}

fn expect_rule<'a>(
    inner: &mut impl Iterator<Item = Pair<'a, Rule>>,
    rule: Rule,
    message: &'static str,
) -> CompileResult<Pair<'a, Rule>> {
    let pair = inner.next().ok_or_else(|| CompileError::new(message, 1, 1))?;

    if pair.as_rule() == rule { Ok(pair) } else { Err(pair_error(&pair, message)) }
}

fn pair_error(pair: &Pair<'_, Rule>, message: impl Into<String>) -> CompileError {
    let (line, column) = pair.as_span().start_pos().line_col();
    CompileError::new(message, line, column)
}

fn pair_position_error(pair: &Pair<'_, Rule>, message: impl Into<String>) -> CompileError {
    pair_error(pair, message)
}

fn pair_origin(pair: &Pair<'_, Rule>, file: &str) -> SourceOrigin {
    let (line, column) = pair.as_span().start_pos().line_col();
    SourceOrigin::new(file, line, column)
}

fn unquote(value: &str) -> String {
    let value = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')).unwrap_or(value);
    let mut output = String::new();
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        output.push(match chars.next() {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some(other) => other,
            None => '\\',
        });
    }

    output
}

#[cfg(test)]
mod tests {
    use super::parse_schema;

    #[test]
    fn compiles_schema_model() {
        let schema = parse_schema(
            r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
                read_replicas = [env("DATABASE_URL"), env("READ_DATABASE_URL")]
            }

            enum Status {
                Active
                Canceled
            }

            model User {
                id      String  @id @default(uuid())
                email   String
                status  Status
                tokens  Token[]
            }

            model Token {
                id       String  @id @default(uuid())
                user     User?   @relation(fields: [user_id], references: [id], onDelete: Cascade)
                user_id  String?
            }
            "#,
        )
        .expect("schema should compile");

        assert_eq!(schema.config().expect("config").entries.len(), 3);
        assert_eq!(schema.enums().count(), 1);
        assert_eq!(schema.models().count(), 2);

        let token = schema.models().find(|model| model.name == "Token").expect("token");
        let user = token.fields.iter().find(|field| field.name == "user").expect("user");
        assert!(user.ty.optional);
        assert!(user.attributes.iter().any(|attribute| attribute.name == "relation"));
    }
}
