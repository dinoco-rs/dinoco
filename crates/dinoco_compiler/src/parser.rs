use std::collections::HashSet;

use pest::Parser;
use pest::iterators::Pair;
use pest_derive::Parser;

use crate::ast::{
    Attribute, AttributeArgument, AttributeValue, ConfigBlock, ConfigEntry, ConfigValue, EnumDef, FieldType, Model,
    ModelField, Schema, SchemaItem,
};
use crate::error::CompileError;

pub type CompileResult<T> = Result<T, CompileError>;

#[derive(Parser)]
#[grammar = "schema.pest"]
struct DinocoPestParser;

pub fn parse_schema(source: &str) -> CompileResult<Schema> {
    let mut pairs = DinocoPestParser::parse(Rule::schema, source).map_err(CompileError::from)?;
    let schema = pairs.next().expect("schema pair");
    let mut items = Vec::new();

    for pair in schema.into_inner() {
        match pair.as_rule() {
            Rule::config_block => items.push(SchemaItem::Config(parse_config(pair)?)),
            Rule::enum_block => items.push(SchemaItem::Enum(parse_enum(pair)?)),
            Rule::model_block => items.push(SchemaItem::Model(parse_model(pair)?)),
            Rule::EOI => {}
            _ => return Err(pair_error(&pair, "unexpected schema item")),
        }
    }

    Ok(Schema { items })
}

fn parse_config(pair: Pair<'_, Rule>) -> CompileResult<ConfigBlock> {
    let mut entries = Vec::new();

    for pair in pair.into_inner() {
        if pair.as_rule() == Rule::config_entry {
            entries.push(parse_config_entry(pair)?);
        }
    }

    Ok(ConfigBlock { entries })
}

fn parse_config_entry(pair: Pair<'_, Rule>) -> CompileResult<ConfigEntry> {
    let mut inner = pair.into_inner();
    let key = expect_rule(&mut inner, Rule::ident, "expected config key")?.as_str().to_string();
    let value = parse_config_value(expect_rule(&mut inner, Rule::config_value, "expected config value")?)?;

    Ok(ConfigEntry { key, value })
}

pub(crate) fn validate_schema(schema: &Schema) -> CompileResult<()> {
    validate_declarations(schema)?;
    validate_config_values(schema)?;
    validate_relation_attributes(schema)?;
    validate_relation_pairs(schema)?;

    let mut uses_snowflake = false;

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
                        return Err(CompileError::new("uuid() defaults are only supported for String fields", 1, 1));
                    }
                    "snowflake" if field.ty.name != "Integer" => {
                        return Err(CompileError::new(
                            "snowflake() defaults are only supported for Integer fields",
                            1,
                            1,
                        ));
                    }
                    "autoincrement" if field.ty.name != "Integer" => {
                        return Err(CompileError::new(
                            "autoincrement() defaults are only supported for Integer fields",
                            1,
                            1,
                        ));
                    }
                    "now" if field.ty.name != "DateTime" && field.ty.name != "Date" => {
                        return Err(CompileError::new(
                            "now() defaults are only supported for DateTime or Date fields",
                            1,
                            1,
                        ));
                    }
                    "snowflake" => uses_snowflake = true,
                    "uuid" | "autoincrement" | "now" => {}
                    _ => {
                        return Err(CompileError::new(
                            "unsupported @default() function. Supported: autoincrement(), uuid(), snowflake(), now()",
                            1,
                            1,
                        ));
                    }
                }
            }
        }
    }

    if uses_snowflake {
        let has_node_id = schema
            .config()
            .into_iter()
            .flat_map(|config| &config.entries)
            .any(|entry| entry.key == "snowflake_node_id" && matches!(entry.value, ConfigValue::Env(_)));
        if !has_node_id {
            return Err(CompileError::new("snowflake() requires config.snowflake_node_id = env(\"...\")", 1, 1));
        }
    }

    Ok(())
}

fn validate_declarations(schema: &Schema) -> CompileResult<()> {
    const SCALAR_TYPES: [&str; 7] = ["String", "Boolean", "Integer", "Float", "DateTime", "Date", "Json"];

    if schema.items.iter().filter(|item| matches!(item, SchemaItem::Config(_))).count() > 1 {
        return schema_error("Only one `config` block is allowed");
    }

    for config in schema.items.iter().filter_map(|item| match item {
        SchemaItem::Config(config) => Some(config),
        _ => None,
    }) {
        let mut keys = HashSet::new();
        for entry in &config.entries {
            if !keys.insert(entry.key.as_str()) {
                return schema_error(format!("Config key `{}` is declared more than once", entry.key));
            }
        }
    }

    let mut type_names = HashSet::new();
    for item in &schema.items {
        let name = match item {
            SchemaItem::Enum(item) => Some(item.name.as_str()),
            SchemaItem::Model(item) => Some(item.name.as_str()),
            SchemaItem::Config(_) => None,
        };
        if let Some(name) = name {
            if SCALAR_TYPES.contains(&name) {
                return schema_error(format!("Type `{name}` conflicts with a built-in scalar type"));
            }
            if !type_names.insert(name) {
                return schema_error(format!("Type `{name}` is declared more than once"));
            }
        }
    }

    for item in schema.enums() {
        if item.values.is_empty() {
            return schema_error(format!("Enum `{}` must contain at least one value", item.name));
        }
        let mut values = HashSet::new();
        for value in &item.values {
            if !values.insert(value.as_str()) {
                return schema_error(format!("Enum value `{}.{value}` is declared more than once", item.name));
            }
        }
    }

    for model in schema.models() {
        if model.fields.is_empty() {
            return schema_error(format!("Model `{}` must contain at least one field", model.name));
        }
        let mut field_names = HashSet::new();
        for field in &model.fields {
            if !field_names.insert(field.name.as_str()) {
                return schema_error(format!("Field `{}.{}` is declared more than once", model.name, field.name));
            }

            let scalar = SCALAR_TYPES.contains(&field.ty.name.as_str());
            let enum_type = schema.enums().any(|item| item.name == field.ty.name);
            let model_type = schema.models().any(|item| item.name == field.ty.name);
            if !scalar && !enum_type && !model_type {
                return schema_error(format!(
                    "Field `{}.{}` uses unknown type `{}`",
                    model.name, field.name, field.ty.name
                ));
            }
            if field.ty.list && !model_type {
                return schema_error(format!(
                    "Field `{}.{}` is a list of `{}`, but only model relation fields may be lists",
                    model.name, field.name, field.ty.name
                ));
            }
            if field.ty.list && field.ty.optional {
                return schema_error(format!(
                    "Relation list `{}.{}` cannot be optional; remove `?` from the field type",
                    model.name, field.name
                ));
            }

            let mut attributes = HashSet::new();
            for attribute in &field.attributes {
                if matches!(attribute.name.as_str(), "id" | "unique" | "default" | "relation")
                    && !attributes.insert(attribute.name.as_str())
                {
                    return schema_error(format!(
                        "Field `{}.{}` declares @{} more than once",
                        model.name, field.name, attribute.name
                    ));
                }
            }
        }
    }

    Ok(())
}

fn validate_config_values(schema: &Schema) -> CompileResult<()> {
    for config in schema.items.iter().filter_map(|item| match item {
        SchemaItem::Config(config) => Some(config),
        _ => None,
    }) {
        for entry in &config.entries {
            match (entry.key.as_str(), &entry.value) {
                ("database_url", ConfigValue::Env(name)) if !name.trim().is_empty() => {}
                ("database_url", ConfigValue::Env(_)) => {
                    return schema_error("`database_url` env name cannot be empty");
                }
                ("database_url", _) => {
                    return schema_error("`database_url` only accepts env(\"DATABASE_URL\")");
                }
                ("read_replicas", ConfigValue::Array(values))
                    if values
                        .iter()
                        .all(|value| matches!(value, ConfigValue::Env(name) if !name.trim().is_empty())) => {}
                ("read_replicas", ConfigValue::Array(_)) => {
                    return schema_error("`read_replicas` only accepts non-empty env(...) values");
                }
                ("read_replicas", _) => {
                    return schema_error("`read_replicas` must be an array of env(...) values");
                }
                ("snowflake_node_id", ConfigValue::Env(name)) if !name.trim().is_empty() => {}
                ("snowflake_node_id", ConfigValue::Env(_)) => {
                    return schema_error("`snowflake_node_id` env name cannot be empty");
                }
                ("snowflake_node_id", _) => {
                    return schema_error("`snowflake_node_id` only accepts env(...)");
                }
                _ => {}
            }
        }
    }

    Ok(())
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
                return schema_error(format!(
                    "Relation field `{}.{}` declares @relation more than once",
                    model.name, field.name
                ));
            }
            if relation_count == 0 {
                continue;
            }
            if !field.is_relation(schema) {
                return schema_error(format!(
                    "Field `{}.{}` uses @relation, but `{}` is not a model",
                    model.name, field.name, field.ty.name
                ));
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
                    return schema_error(format!(
                        "Relation field `{}.{}` targets model `{}`{relation_suffix}, but `{}` has no compatible \
                         opposite relation field pointing back to `{}`",
                        model.name, field.name, target.name, target.name, model.name
                    ));
                }
                _ => {
                    return schema_error(format!(
                        "Ambiguous relation field `{}.{}`: model `{}` has multiple possible opposite fields \
                         pointing back to `{}`; add matching @relation(name: \"...\") attributes to both sides",
                        model.name, field.name, target.name, model.name
                    ));
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
        return schema_error(format!(
            "Self relation `{}.{}` must declare the same non-empty @relation(name: \"...\") on both sides",
            model.name, field.name
        ));
    }

    match (field.ty.list, opposite.ty.list) {
        (true, true) => {
            if definition.has_keys() || opposite_definition.has_keys() {
                return schema_error(format!(
                    "Many-to-many relation `{}.{}` <-> `{}.{}` cannot declare fields/references; implicit join \
                     relations require two unmapped list fields",
                    model.name, field.name, target.name, opposite.name
                ));
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
        return schema_error(format!(
            "One-to-many relation `{}.{}` <-> `{}.{}` requires fields/references on the singular FK-owning side \
             `{}.{}`",
            list_model.name, list_field.name, owner_model.name, owner_field.name, owner_model.name, owner_field.name
        ));
    }

    validate_owner_keys(schema, owner_model, owner_field, list_model, owner_definition)?;
    if relation_is_unique(owner_model, owner_field, owner_definition) {
        return schema_error(format!(
            "Relation `{}.{}` is unique, so its opposite `{}.{}` must be singular instead of a list",
            owner_model.name, owner_field.name, list_model.name, list_field.name
        ));
    }

    reject_non_owning_options(list_model, list_field, list_definition)?;
    if list_definition.has_keys() {
        let list_fields = list_definition.fields.as_deref().expect("checked relation fields");
        let list_references = list_definition.references.as_deref().expect("checked relation references");
        let owner_fields = owner_definition.fields.as_deref().expect("checked owner fields");
        let owner_references = owner_definition.references.as_deref().expect("checked owner references");
        if list_fields != owner_references || list_references != owner_fields {
            return schema_error(format!(
                "List relation `{}.{}` must mirror the owning side: expected fields: [{}], references: [{}]",
                list_model.name,
                list_field.name,
                owner_references.join(", "),
                owner_fields.join(", ")
            ));
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
    let (owner_model, owner_field, owner_definition, inverse_model, inverse_field, inverse_definition) =
        match (definition.has_keys(), opposite_definition.has_keys()) {
            (true, false) => (model, field, definition, target, opposite, opposite_definition),
            (false, true) => (target, opposite, opposite_definition, model, field, definition),
            (false, false) => {
                return schema_error(format!(
                    "One-to-one relation `{}.{}` <-> `{}.{}` requires fields/references on exactly one FK-owning side",
                    model.name, field.name, target.name, opposite.name
                ));
            }
            (true, true) => {
                return schema_error(format!(
                    "One-to-one relation `{}.{}` <-> `{}.{}` declares fields/references on both sides; only one side \
                     may own the foreign key",
                    model.name, field.name, target.name, opposite.name
                ));
            }
        };

    validate_owner_keys(schema, owner_model, owner_field, inverse_model, owner_definition)?;
    reject_non_owning_options(inverse_model, inverse_field, inverse_definition)?;
    if owner_field.attributes.iter().any(|attribute| attribute.name == "unique")
        && owner_definition.fields.as_ref().is_some_and(|fields| fields.len() != 1)
    {
        return schema_error(format!(
            "Composite one-to-one relation `{}.{}` cannot place @unique on the relation field; declare @unique on \
             each local foreign-key field so the database constraint can be generated explicitly",
            owner_model.name, owner_field.name
        ));
    }
    if !relation_is_unique(owner_model, owner_field, owner_definition) {
        return schema_error(format!(
            "One-to-one relation `{}.{}` requires @unique on the relation field or its local foreign-key field",
            owner_model.name, owner_field.name
        ));
    }
    if !inverse_field.ty.optional {
        return schema_error(format!(
            "The non-owning side `{}.{}` of a one-to-one relation must be optional because it has no local foreign key",
            inverse_model.name, inverse_field.name
        ));
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
    let relation_is_optional = local_fields.iter().any(|local| local.ty.optional);
    if field.ty.optional != relation_is_optional {
        return schema_error(format!(
            "Relation `{}.{}` optionality must match its local foreign key: use `{}` because fields [{}] are {}",
            model.name,
            field.name,
            if relation_is_optional { format!("{}?", field.ty.name) } else { field.ty.name.clone() },
            definition.fields.as_deref().unwrap_or_default().join(", "),
            if relation_is_optional { "nullable" } else { "required" }
        ));
    }

    for (action_name, action) in &definition.actions {
        if action == "SetNull" && local_fields.iter().any(|local| !local.ty.optional) {
            return schema_error(format!(
                "{action_name}: SetNull on `{}.{}` requires every local foreign-key field to be optional",
                model.name, field.name
            ));
        }
        if action == "SetDefault"
            && local_fields.iter().any(|local| !local.attributes.iter().any(|attribute| attribute.name == "default"))
        {
            return schema_error(format!(
                "{action_name}: SetDefault on `{}.{}` requires every local foreign-key field to define @default(...)",
                model.name, field.name
            ));
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
        return schema_error(format!(
            "Relation `{}.{}` must have the same number of fields ({}) and references ({})",
            model.name,
            field.name,
            fields.len(),
            references.len()
        ));
    }

    let mut local_fields = Vec::with_capacity(fields.len());
    for (local_name, reference_name) in fields.iter().zip(references) {
        let Some(local) = model.fields.iter().find(|candidate| candidate.name == *local_name) else {
            return schema_error(format!(
                "Relation `{}.{}` refers to missing local field `{}.{local_name}`",
                model.name, field.name, model.name
            ));
        };
        let Some(reference) = target.fields.iter().find(|candidate| candidate.name == *reference_name) else {
            return schema_error(format!(
                "Relation `{}.{}` refers to missing target field `{}.{reference_name}`",
                model.name, field.name, target.name
            ));
        };
        if local.ty.list || local.is_relation(schema) {
            return schema_error(format!(
                "Relation key `{}.{}` must be a scalar or enum field, not a relation/list field",
                model.name, local.name
            ));
        }
        if reference.ty.list || reference.is_relation(schema) {
            return schema_error(format!(
                "Referenced key `{}.{}` must be a scalar or enum field, not a relation/list field",
                target.name, reference.name
            ));
        }
        if local.ty.name != reference.ty.name {
            return schema_error(format!(
                "Relation `{}.{}` has incompatible key types: `{}.{}` is `{}`, but `{}.{}` is `{}`",
                model.name,
                field.name,
                model.name,
                local.name,
                local.ty.name,
                target.name,
                reference.name,
                reference.ty.name
            ));
        }
        if require_unique_reference
            && !reference.attributes.iter().any(|attribute| matches!(attribute.name.as_str(), "id" | "unique"))
        {
            return schema_error(format!(
                "Relation `{}.{}` references `{}.{}`, which must declare @id or @unique",
                model.name, field.name, target.name, reference.name
            ));
        }
        local_fields.push(local);
    }

    Ok(local_fields)
}

fn reject_non_owning_options(model: &Model, field: &ModelField, definition: &RelationDefinition) -> CompileResult<()> {
    if definition.has_map || !definition.actions.is_empty() {
        return schema_error(format!(
            "Relation `{}.{}` does not own a foreign key, so map/onDelete/onUpdate must be declared on the singular \
             side with fields/references",
            model.name, field.name
        ));
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
        return schema_error(format!(
            "Implicit many-to-many relations require model `{}` to have exactly one scalar @id field",
            model.name
        ));
    }
    Ok(())
}

fn relation_is_unique(model: &Model, field: &ModelField, definition: &RelationDefinition) -> bool {
    field.attributes.iter().any(|attribute| attribute.name == "unique")
        || definition.fields.as_ref().is_some_and(|fields| {
            fields.iter().all(|name| {
                model.fields.iter().find(|candidate| candidate.name == *name).is_some_and(|candidate| {
                    candidate.attributes.iter().any(|attribute| matches!(attribute.name.as_str(), "id" | "unique"))
                })
            })
        })
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
                    return schema_error(format!(
                        "Unknown @relation argument `{key}` on `{}.{}`",
                        model.name, field.name
                    ));
                }
                if !named.insert(key.as_str()) {
                    return schema_error(format!(
                        "Duplicate @relation argument `{key}` on `{}.{}`",
                        model.name, field.name
                    ));
                }
            }
            AttributeArgument::Value(value) => {
                if positional_name.is_some() {
                    return schema_error(format!(
                        "Relation `{}.{}` accepts at most one positional name",
                        model.name, field.name
                    ));
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
        return schema_error(format!(
            "Relation `{}.{}` declares its name both positionally and with `name:`",
            model.name, field.name
        ));
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
        return schema_error(format!(
            "Relation `{}.{}` must declare `fields` and `references` together",
            model.name, field.name
        ));
    }
    if fields.as_ref().is_some_and(|fields| fields.len() != references.as_ref().map_or(0, Vec::len)) {
        return schema_error(format!(
            "Relation `{}.{}` must have the same number of fields and references",
            model.name, field.name
        ));
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
            return schema_error(format!(
                "{action_name} on `{}.{}` must be one of: Cascade, Restrict, NoAction, SetNull, SetDefault",
                model.name, field.name
            ));
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
        return schema_error(format!(
            "@relation({argument}: ...) on `{}.{}` must be a non-empty array of field identifiers",
            model.name, field.name
        ));
    };
    if values.is_empty() {
        return schema_error(format!("@relation({argument}: ...) on `{}.{}` cannot be empty", model.name, field.name));
    }

    let mut seen = HashSet::new();
    let mut names = Vec::with_capacity(values.len());
    for value in values {
        let AttributeValue::Ident(name) = value else {
            return schema_error(format!(
                "@relation({argument}: ...) on `{}.{}` only accepts field identifiers",
                model.name, field.name
            ));
        };
        if !seen.insert(name.as_str()) {
            return schema_error(format!(
                "@relation({argument}: ...) on `{}.{}` contains duplicate field `{name}`",
                model.name, field.name
            ));
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
        return schema_error(format!("{label} on `{}.{}` must be a string or identifier", model.name, field.name));
    };
    if value.trim().is_empty() {
        return schema_error(format!("{label} on `{}.{}` cannot be empty", model.name, field.name));
    }
    Ok(value.to_string())
}

fn schema_error<T>(message: impl Into<String>) -> CompileResult<T> {
    Err(CompileError::new(message, 1, 1))
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

fn parse_config_value(pair: Pair<'_, Rule>) -> CompileResult<ConfigValue> {
    let error = pair_position_error(&pair, "expected config value");
    let value = pair.into_inner().next().ok_or(error)?;

    match value.as_rule() {
        Rule::config_array => {
            let values = value.into_inner().map(parse_config_value).collect::<CompileResult<Vec<_>>>()?;
            Ok(ConfigValue::Array(values))
        }
        Rule::env_call => {
            let error = pair_position_error(&value, "expected env name");
            let raw = value.into_inner().find(|pair| pair.as_rule() == Rule::string_literal).ok_or(error)?;
            Ok(ConfigValue::Env(unquote(raw.as_str())))
        }
        Rule::string_literal => Ok(ConfigValue::String(unquote(value.as_str()))),
        Rule::ident => Ok(ConfigValue::Ident(value.as_str().to_string())),
        _ => Err(pair_error(&value, "unexpected config value")),
    }
}

fn parse_enum(pair: Pair<'_, Rule>) -> CompileResult<EnumDef> {
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

    Ok(EnumDef { name, values })
}

fn parse_model(pair: Pair<'_, Rule>) -> CompileResult<Model> {
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected model name")?.as_str().to_string();
    let fields = inner
        .filter(|pair| pair.as_rule() == Rule::model_field)
        .map(parse_model_field)
        .collect::<CompileResult<Vec<_>>>()?;

    Ok(Model { name, fields })
}

fn parse_model_field(pair: Pair<'_, Rule>) -> CompileResult<ModelField> {
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
            Rule::attribute => attributes.push(parse_attribute(pair)?),
            _ => return Err(pair_error(&pair, "unexpected field token")),
        }
    }

    Ok(ModelField { name, ty: FieldType { name: ty_name, optional, list }, attributes })
}

fn parse_attribute(pair: Pair<'_, Rule>) -> CompileResult<Attribute> {
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected attribute name")?.as_str().to_string();
    let arguments = inner
        .find(|pair| pair.as_rule() == Rule::attribute_arguments)
        .map(parse_attribute_arguments)
        .transpose()?
        .unwrap_or_default();

    Ok(Attribute { name, arguments })
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
