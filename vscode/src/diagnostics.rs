use std::collections::{HashMap, HashSet};

use dinoco_compiler::{Attribute, AttributeArgument, AttributeValue, ConfigValue, Model, ModelField, Schema};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::document::{BlockInfo, BlockKind, DocumentIndex, FieldInfo, scalar_types};

pub const CODE_UNKNOWN_TYPE: &str = "dinoco.unknownType";
pub const CODE_MISSING_CONFIG: &str = "dinoco.missingConfig";
pub const CODE_MISSING_DATABASE_URL: &str = "dinoco.missingDatabaseUrl";
pub const CODE_MISSING_SNOWFLAKE_NODE_ID: &str = "dinoco.missingSnowflakeNodeId";

pub fn analyze(source: &str, index: &DocumentIndex) -> Vec<Diagnostic> {
    let schema = match dinoco_compiler::parse(source) {
        Ok(schema) => schema,
        Err(error) => {
            let mut diagnostics = Vec::new();
            validate_ambiguous_relations(index, &mut diagnostics);
            let start = compiler_position(source, error.line, error.column);
            let end = Position::new(start.line, start.character.saturating_add(1));
            diagnostics.push(diagnostic(
                Range::new(start, end),
                DiagnosticSeverity::ERROR,
                "dinoco.syntax",
                error.message,
            ));
            return diagnostics;
        }
    };

    let mut diagnostics = Vec::new();
    validate_top_level(index, &mut diagnostics);
    validate_config(&schema, index, &mut diagnostics);
    validate_models(&schema, index, &mut diagnostics);
    validate_relation_pairs(&schema, index, &mut diagnostics);

    if let Err(error) = dinoco_compiler::compile(source)
        && !diagnostics.iter().any(|item| item.message == error.message)
    {
        diagnostics.push(diagnostic(
            compiler_semantic_range(index, &error.message),
            DiagnosticSeverity::ERROR,
            "dinoco.schema",
            error.message,
        ));
    }

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
    let snowflake_range = first_snowflake_range(schema, index);
    let Some(config) = schema.config() else {
        diagnostics.push(diagnostic(
            Range::new(Position::new(0, 0), Position::new(0, 0)),
            DiagnosticSeverity::ERROR,
            CODE_MISSING_CONFIG,
            "A `config` block is required to connect Dinoco to a database.",
        ));
        if let Some(range) = snowflake_range {
            diagnostics.push(diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                CODE_MISSING_SNOWFLAKE_NODE_ID,
                "snowflake() requires config.snowflake_node_id = env(\"...\")",
            ));
        }
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
        match (entry.key.as_str(), &entry.value) {
            ("database_url", ConfigValue::Env(name)) if !name.trim().is_empty() => {}
            ("database_url", _) => diagnostics.push(diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "dinoco.invalidDatabaseUrl",
                "`database_url` must use a non-empty env(\"...\") value.",
            )),
            ("read_replicas", ConfigValue::Array(values))
                if values.iter().all(|value| matches!(value, ConfigValue::Env(name) if !name.trim().is_empty())) => {}
            ("read_replicas", _) => diagnostics.push(diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "dinoco.invalidReadReplicas",
                "`read_replicas` must be an array containing only non-empty env(\"...\") values.",
            )),
            ("snowflake_node_id", ConfigValue::Env(name)) if !name.trim().is_empty() => {}
            ("snowflake_node_id", _) => diagnostics.push(diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "dinoco.invalidSnowflakeNodeId",
                "`snowflake_node_id` must use a non-empty env(\"...\") value.",
            )),
            _ => {}
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

    if let Some(range) = snowflake_range
        && !config.entries.iter().any(|entry| {
            entry.key == "snowflake_node_id"
                && matches!(&entry.value, ConfigValue::Env(name) if !name.trim().is_empty())
        })
    {
        diagnostics.push(diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            CODE_MISSING_SNOWFLAKE_NODE_ID,
            "snowflake() requires config.snowflake_node_id = env(\"...\")",
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
        let primary_key_declarations =
            model.fields.iter().filter(|field| field.attributes.iter().any(|attribute| attribute.name == "id")).count()
                + model.attributes("ids").count();
        if primary_key_declarations == 0 {
            diagnostics.push(diagnostic(
                model_index.body_range,
                DiagnosticSeverity::ERROR,
                "dinoco.missingPrimaryKey",
                format!("Model `{}` must declare exactly one primary key using @id or @@ids([...])", model.name),
            ));
        } else if primary_key_declarations > 1 {
            diagnostics.push(diagnostic(
                model_index.body_range,
                DiagnosticSeverity::ERROR,
                "dinoco.multiplePrimaryKeys",
                format!(
                    "Model `{}` declares multiple primary keys; use exactly one @id or one @@ids([...])",
                    model.name
                ),
            ));
        }
        let mut seen = HashSet::new();

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

            let relations =
                field.attributes.iter().filter(|attribute| attribute.name == "relation").collect::<Vec<_>>();
            if relations.len() > 1 {
                diagnostics.push(diagnostic(
                    field_index.name.range,
                    DiagnosticSeverity::ERROR,
                    "dinoco.duplicateRelationAttribute",
                    format!("Relation field `{}.{}` declares @relation more than once.", model.name, field.name),
                ));
            }
            if let Some(relation) = relations.first() {
                validate_relation(schema, model, field, relation, field_index, diagnostics);
            }
        }
    }
}

fn validate_ambiguous_relations(index: &DocumentIndex, diagnostics: &mut Vec<Diagnostic>) {
    for model in index.blocks.iter().filter(|block| block.kind == BlockKind::Model) {
        let Some(model_name) = model.name.as_ref().map(|name| name.name.as_str()) else {
            continue;
        };
        let mut relation_targets: HashMap<&str, Vec<&FieldInfo>> = HashMap::new();
        for field in &model.fields {
            if field.attribute("relation").is_some() {
                relation_targets.entry(&field.ty.name).or_default().push(field);
            }
        }

        for (target, relations) in relation_targets {
            if relations.len() < 2 {
                continue;
            }
            for field_index in relations {
                let relation = field_index.attribute("relation").expect("indexed relation attribute");
                if relation.argument("name").is_none() {
                    diagnostics.push(diagnostic(
                        relation.name.range,
                        DiagnosticSeverity::ERROR,
                        "dinoco.ambiguousRelation",
                        format!(
                            "Multiple relations from `{}` to `{target}` require a unique `name` argument.",
                            model_name
                        ),
                    ));
                }
            }
        }
    }
}

#[derive(Debug)]
enum RelationMapping<'a> {
    None,
    Valid { fields: Vec<&'a str>, references: Vec<&'a str> },
    Invalid,
}

fn validate_relation_pairs(schema: &Schema, index: &DocumentIndex, diagnostics: &mut Vec<Diagnostic>) {
    let mut checked = HashSet::new();

    for model in schema.models() {
        for (field_position, field) in model.fields.iter().enumerate() {
            if !field.is_relation(schema) {
                continue;
            }
            let Some(target) = schema.models().find(|candidate| candidate.name == field.ty.name) else {
                continue;
            };
            let relation_name = field_relation_name(field);
            let candidates = target
                .fields
                .iter()
                .enumerate()
                .filter(|(candidate_position, candidate)| {
                    candidate.ty.name == model.name
                        && (model.name != target.name || *candidate_position != field_position)
                        && field_relation_name(candidate) == relation_name
                })
                .map(|(_, candidate)| candidate)
                .collect::<Vec<_>>();
            let field_index = indexed_field(index, model, field);

            let opposite = match candidates.as_slice() {
                [opposite] => *opposite,
                [] => {
                    let suffix =
                        relation_name.map(|name| format!(" using @relation(name: \"{name}\")")).unwrap_or_default();
                    diagnostics.push(diagnostic(
                        field_index.map_or(default_range(), |item| item.range),
                        DiagnosticSeverity::ERROR,
                        "dinoco.missingOppositeRelation",
                        format!(
                            "Relation field `{}.{}` targets model `{}`{suffix}, but `{}` has no compatible opposite \
                             relation field pointing back to `{}`",
                            model.name, field.name, target.name, target.name, model.name
                        ),
                    ));
                    continue;
                }
                _ => {
                    diagnostics.push(diagnostic(
                        field_index.map_or(default_range(), |item| item.range),
                        DiagnosticSeverity::ERROR,
                        "dinoco.ambiguousRelation",
                        format!(
                            "Ambiguous relation field `{}.{}`: model `{}` has multiple possible opposite fields \
                             pointing back to `{}`; add matching @relation(name: \"...\") attributes to both sides",
                            model.name, field.name, target.name, model.name
                        ),
                    ));
                    continue;
                }
            };

            let mut endpoints =
                [format!("{}.{}", model.name, field.name), format!("{}.{}", target.name, opposite.name)];
            endpoints.sort();
            if !checked.insert(endpoints.join("|")) {
                continue;
            }

            validate_relation_pair_shape(schema, index, model, field, target, opposite, diagnostics);
        }
    }
}

fn validate_relation_pair_shape(
    schema: &Schema,
    index: &DocumentIndex,
    model: &Model,
    field: &ModelField,
    target: &Model,
    opposite: &ModelField,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let field_range = indexed_field(index, model, field).map_or(default_range(), |item| item.range);
    let opposite_range = indexed_field(index, target, opposite).map_or(default_range(), |item| item.range);
    if model.name == target.name && field_relation_name(field).is_none() {
        diagnostics.push(diagnostic(
            field_range,
            DiagnosticSeverity::ERROR,
            "dinoco.unnamedSelfRelation",
            format!(
                "Self relation `{}.{}` must declare the same non-empty @relation(name: \"...\") on both sides",
                model.name, field.name
            ),
        ));
        return;
    }

    let mapping = relation_mapping(field);
    let opposite_mapping = relation_mapping(opposite);
    if matches!(mapping, RelationMapping::Invalid) || matches!(opposite_mapping, RelationMapping::Invalid) {
        return;
    }

    match (field.ty.list, opposite.ty.list) {
        (true, true) => {
            if !matches!(mapping, RelationMapping::None) || !matches!(opposite_mapping, RelationMapping::None) {
                diagnostics.push(diagnostic(
                    field_range,
                    DiagnosticSeverity::ERROR,
                    "dinoco.mappedManyToMany",
                    format!(
                        "Many-to-many relation `{}.{}` <-> `{}.{}` cannot declare fields/references; implicit join \
                         relations require two unmapped list fields",
                        model.name, field.name, target.name, opposite.name
                    ),
                ));
            }
            validate_non_owning_options(index, model, field, diagnostics);
            validate_non_owning_options(index, target, opposite, diagnostics);
            validate_many_to_many_id(schema, index, model, field_range, diagnostics);
            if model.name != target.name {
                validate_many_to_many_id(schema, index, target, opposite_range, diagnostics);
            }
        }
        (true, false) => validate_one_to_many_diagnostics(
            schema,
            index,
            model,
            field,
            &mapping,
            target,
            opposite,
            &opposite_mapping,
            diagnostics,
        ),
        (false, true) => validate_one_to_many_diagnostics(
            schema,
            index,
            target,
            opposite,
            &opposite_mapping,
            model,
            field,
            &mapping,
            diagnostics,
        ),
        (false, false) => validate_one_to_one_diagnostics(
            schema,
            index,
            model,
            field,
            &mapping,
            target,
            opposite,
            &opposite_mapping,
            diagnostics,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_one_to_many_diagnostics(
    schema: &Schema,
    index: &DocumentIndex,
    list_model: &Model,
    list_field: &ModelField,
    list_mapping: &RelationMapping<'_>,
    owner_model: &Model,
    owner_field: &ModelField,
    owner_mapping: &RelationMapping<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let list_range = indexed_field(index, list_model, list_field).map_or(default_range(), |item| item.range);
    let owner_range = indexed_field(index, owner_model, owner_field).map_or(default_range(), |item| item.range);
    let RelationMapping::Valid { fields: owner_fields, references: owner_references } = owner_mapping else {
        diagnostics.push(diagnostic(
            owner_range,
            DiagnosticSeverity::ERROR,
            "dinoco.missingRelationKeys",
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
        ));
        return;
    };

    validate_owner_diagnostics(
        schema,
        index,
        owner_model,
        owner_field,
        list_model,
        owner_fields,
        owner_references,
        diagnostics,
    );
    if relation_field_is_unique(owner_model, owner_field, owner_fields) {
        diagnostics.push(diagnostic(
            owner_range,
            DiagnosticSeverity::ERROR,
            "dinoco.relationCardinalityMismatch",
            format!(
                "Relation `{}.{}` is unique, so its opposite `{}.{}` must be singular instead of a list",
                owner_model.name, owner_field.name, list_model.name, list_field.name
            ),
        ));
    }

    validate_non_owning_options(index, list_model, list_field, diagnostics);
    if let RelationMapping::Valid { fields, references } = list_mapping
        && (fields != owner_references || references != owner_fields)
    {
        diagnostics.push(diagnostic(
            list_range,
            DiagnosticSeverity::ERROR,
            "dinoco.invalidInverseRelationKeys",
            format!(
                "List relation `{}.{}` must mirror the owning side: expected fields: [{}], references: [{}]",
                list_model.name,
                list_field.name,
                owner_references.join(", "),
                owner_fields.join(", ")
            ),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_one_to_one_diagnostics(
    schema: &Schema,
    index: &DocumentIndex,
    model: &Model,
    field: &ModelField,
    mapping: &RelationMapping<'_>,
    target: &Model,
    opposite: &ModelField,
    opposite_mapping: &RelationMapping<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let pair_range = indexed_field(index, model, field).map_or(default_range(), |item| item.range);
    let (owner_model, owner_field, owner_fields, owner_references, inverse_model, inverse_field) =
        match (mapping, opposite_mapping) {
            (RelationMapping::Valid { fields, references }, RelationMapping::None) => {
                (model, field, fields, references, target, opposite)
            }
            (RelationMapping::None, RelationMapping::Valid { fields, references }) => {
                (target, opposite, fields, references, model, field)
            }
            (RelationMapping::None, RelationMapping::None) => {
                diagnostics.push(diagnostic(
                    pair_range,
                    DiagnosticSeverity::ERROR,
                    "dinoco.missingRelationKeys",
                    format!(
                        "One-to-one relation `{}.{}` <-> `{}.{}` requires fields/references on exactly one \
                         FK-owning side",
                        model.name, field.name, target.name, opposite.name
                    ),
                ));
                return;
            }
            (RelationMapping::Valid { .. }, RelationMapping::Valid { .. }) => {
                diagnostics.push(diagnostic(
                    pair_range,
                    DiagnosticSeverity::ERROR,
                    "dinoco.multipleRelationOwners",
                    format!(
                        "One-to-one relation `{}.{}` <-> `{}.{}` declares fields/references on both sides; only one \
                         side may own the foreign key",
                        model.name, field.name, target.name, opposite.name
                    ),
                ));
                return;
            }
            (RelationMapping::Invalid, _) | (_, RelationMapping::Invalid) => return,
        };

    validate_owner_diagnostics(
        schema,
        index,
        owner_model,
        owner_field,
        inverse_model,
        owner_fields,
        owner_references,
        diagnostics,
    );
    validate_non_owning_options(index, inverse_model, inverse_field, diagnostics);
    let owner_range = indexed_field(index, owner_model, owner_field).map_or(default_range(), |item| item.range);
    if owner_field.attributes.iter().any(|attribute| attribute.name == "unique") && owner_fields.len() != 1 {
        diagnostics.push(diagnostic(
            owner_range,
            DiagnosticSeverity::ERROR,
            "dinoco.compositeRelationUnique",
            format!(
                "Composite one-to-one relation `{}.{}` cannot place @unique on the relation field; declare \
                 @@uniques([...]) for the complete local foreign-key tuple",
                owner_model.name, owner_field.name
            ),
        ));
    }
    if !relation_field_is_unique(owner_model, owner_field, owner_fields) {
        diagnostics.push(diagnostic(
            owner_range,
            DiagnosticSeverity::ERROR,
            "dinoco.oneToOneRequiresUnique",
            format!(
                "One-to-one relation `{}.{}` requires @unique on the relation field or its local foreign-key field",
                owner_model.name, owner_field.name
            ),
        ));
    }
    if !inverse_field.ty.optional {
        diagnostics.push(diagnostic(
            indexed_field(index, inverse_model, inverse_field).map_or(default_range(), |item| item.ty.range),
            DiagnosticSeverity::ERROR,
            "dinoco.requiredInverseOneToOne",
            format!(
                "The non-owning side `{}.{}` of a one-to-one relation must be optional because it has no local \
                 foreign key",
                inverse_model.name, inverse_field.name
            ),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_owner_diagnostics(
    schema: &Schema,
    index: &DocumentIndex,
    model: &Model,
    field: &ModelField,
    target: &Model,
    fields: &[&str],
    references: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(field_index) = indexed_field(index, model, field) else {
        return;
    };
    let local_fields = fields
        .iter()
        .filter_map(|name| model.fields.iter().find(|candidate| candidate.name == **name))
        .collect::<Vec<_>>();
    let reference_fields = references
        .iter()
        .filter_map(|name| target.fields.iter().find(|candidate| candidate.name == **name))
        .collect::<Vec<_>>();
    if local_fields.len() != fields.len() || reference_fields.len() != references.len() {
        return;
    }

    if !model_has_unique_key(target, references) {
        diagnostics.push(diagnostic(
            references
                .first()
                .map_or(field_index.range, |name| relation_argument_value_range(field_index, "references", name)),
            DiagnosticSeverity::ERROR,
            "dinoco.referenceMustBeUnique",
            format!(
                "Relation `{}.{}` references `{}.[{}]`, which must declare @id or @unique, or match @@ids([...]) or \
                 @@uniques([...]).",
                model.name,
                field.name,
                target.name,
                references.join(", ")
            ),
        ));
    }

    let optional = local_fields.iter().any(|local| local.ty.optional);
    if field.ty.optional != optional {
        diagnostics.push(diagnostic(
            field_index.ty.range,
            DiagnosticSeverity::ERROR,
            "dinoco.relationOptionalityMismatch",
            format!(
                "Relation `{}.{}` optionality must match its local foreign key: use `{}` because fields [{}] are {}.",
                model.name,
                field.name,
                if optional { format!("{}?", field.ty.name) } else { field.ty.name.clone() },
                fields.join(", "),
                if optional { "nullable" } else { "required" }
            ),
        ));
    }

    let relation = field.attributes.iter().find(|attribute| attribute.name == "relation");
    for action in ["onDelete", "onUpdate"] {
        let Some(value) = relation.and_then(|relation| attribute_ident(relation, action)) else {
            continue;
        };
        if value == "SetNull" && local_fields.iter().any(|local| !local.ty.optional) {
            diagnostics.push(diagnostic(
                relation_argument_name_range(field_index, action),
                DiagnosticSeverity::ERROR,
                "dinoco.setNullRequiresOptionalField",
                format!("`{action}: SetNull` requires every local foreign-key field to be optional."),
            ));
        }
        if value == "SetDefault"
            && local_fields.iter().any(|local| !local.attributes.iter().any(|attribute| attribute.name == "default"))
        {
            diagnostics.push(diagnostic(
                relation_argument_name_range(field_index, action),
                DiagnosticSeverity::ERROR,
                "dinoco.setDefaultRequiresDefault",
                format!("`{action}: SetDefault` requires every local foreign-key field to define `@default(...)`."),
            ));
        }
    }

    let _ = schema;
}

fn validate_non_owning_options(
    index: &DocumentIndex,
    model: &Model,
    field: &ModelField,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(relation) = field.attributes.iter().find(|attribute| attribute.name == "relation") else {
        return;
    };
    let Some(field_index) = indexed_field(index, model, field) else {
        return;
    };
    for argument in ["map", "onDelete", "onUpdate"] {
        if relation.argument(argument).is_some() {
            diagnostics.push(diagnostic(
                relation_argument_name_range(field_index, argument),
                DiagnosticSeverity::ERROR,
                "dinoco.referentialOptionOnInverse",
                format!(
                    "Relation `{}.{}` does not own a foreign key, so `{argument}` must be declared on the singular \
                     side with fields/references.",
                    model.name, field.name
                ),
            ));
        }
    }
}

fn validate_many_to_many_id(
    schema: &Schema,
    index: &DocumentIndex,
    model: &Model,
    fallback_range: Range,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ids = model
        .fields
        .iter()
        .filter(|field| field.attributes.iter().any(|attribute| attribute.name == "id"))
        .collect::<Vec<_>>();
    if ids.len() != 1 || ids[0].ty.list || ids[0].ty.optional || ids[0].is_relation(schema) {
        diagnostics.push(diagnostic(
            index.model(&model.name).map_or(fallback_range, |item| item.body_range),
            DiagnosticSeverity::ERROR,
            "dinoco.manyToManyRequiresId",
            format!(
                "Implicit many-to-many relations require model `{}` to have exactly one scalar @id field.",
                model.name
            ),
        ));
    }
}

fn relation_mapping(field: &ModelField) -> RelationMapping<'_> {
    let Some(relation) = field.attributes.iter().find(|attribute| attribute.name == "relation") else {
        return RelationMapping::None;
    };
    match (relation.argument("fields"), relation.argument("references")) {
        (None, None) => RelationMapping::None,
        (Some(fields), Some(references)) => {
            let (Some(fields), Some(references)) =
                (relation_identifier_values(fields), relation_identifier_values(references))
            else {
                return RelationMapping::Invalid;
            };
            if fields.len() != references.len() {
                RelationMapping::Invalid
            } else {
                RelationMapping::Valid { fields, references }
            }
        }
        _ => RelationMapping::Invalid,
    }
}

fn relation_field_is_unique(model: &Model, field: &ModelField, fields: &[&str]) -> bool {
    field.attributes.iter().any(|attribute| attribute.name == "unique") || model_has_unique_key(model, fields)
}

fn model_has_unique_key(model: &Model, fields: &[&str]) -> bool {
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
        .any(|candidate| candidate.iter().copied().eq(fields.iter().copied()))
}

fn field_relation_name(field: &ModelField) -> Option<&str> {
    field.attributes.iter().find(|attribute| attribute.name == "relation").and_then(relation_name_value)
}

fn relation_name_value(relation: &Attribute) -> Option<&str> {
    relation.argument("name").and_then(attribute_value_ident).or_else(|| {
        relation.arguments.iter().find_map(|argument| match argument {
            AttributeArgument::Value(value) => attribute_value_ident(value),
            AttributeArgument::Named { .. } => None,
        })
    })
}

fn attribute_value_ident(value: &AttributeValue) -> Option<&str> {
    match value {
        AttributeValue::Ident(value) | AttributeValue::String(value) => Some(value),
        _ => None,
    }
}

fn relation_identifier_values(value: &AttributeValue) -> Option<Vec<&str>> {
    let AttributeValue::Array(values) = value else {
        return None;
    };
    if values.is_empty() {
        return None;
    }
    values
        .iter()
        .map(|value| match value {
            AttributeValue::Ident(value) => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

fn duplicate_names<'a>(names: &[&'a str]) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    for name in names {
        if !seen.insert(*name) {
            duplicates.insert(*name);
        }
    }
    duplicates.into_iter().collect()
}

fn indexed_field<'a>(index: &'a DocumentIndex, model: &Model, field: &ModelField) -> Option<&'a FieldInfo> {
    index.model(&model.name)?.field(&field.name)
}

fn validate_relation(
    schema: &Schema,
    model: &Model,
    field: &ModelField,
    relation: &Attribute,
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

    let relation_range = field_index.attribute("relation").map_or(field_index.range, |item| item.range);
    let mut named_arguments = HashSet::new();
    let mut positional_names = 0usize;
    for argument in &relation.arguments {
        match argument {
            AttributeArgument::Named { key, .. } => {
                if !matches!(key.as_str(), "name" | "fields" | "references" | "onDelete" | "onUpdate" | "map") {
                    diagnostics.push(diagnostic(
                        relation_argument_name_range(field_index, key),
                        DiagnosticSeverity::ERROR,
                        "dinoco.unknownRelationArgument",
                        format!("Unknown @relation argument `{key}`."),
                    ));
                }
                if !named_arguments.insert(key.as_str()) {
                    diagnostics.push(diagnostic(
                        relation_argument_name_range(field_index, key),
                        DiagnosticSeverity::ERROR,
                        "dinoco.duplicateRelationArgument",
                        format!("Duplicate @relation argument `{key}`."),
                    ));
                }
            }
            AttributeArgument::Value(_) => positional_names += 1,
        }
    }
    if positional_names > 1 || (positional_names == 1 && relation.argument("name").is_some()) {
        diagnostics.push(diagnostic(
            relation_range,
            DiagnosticSeverity::ERROR,
            "dinoco.invalidRelationName",
            "A relation accepts one name, either positionally or through `name:`, but not both.",
        ));
    }
    if relation_name_value(relation).is_some_and(str::is_empty) {
        diagnostics.push(diagnostic(
            relation_argument_name_range(field_index, "name"),
            DiagnosticSeverity::ERROR,
            "dinoco.invalidRelationName",
            "A relation name cannot be empty.",
        ));
    }

    let local_argument = relation.argument("fields");
    let reference_argument = relation.argument("references");
    if local_argument.is_some() != reference_argument.is_some() {
        diagnostics.push(diagnostic(
            relation_range,
            DiagnosticSeverity::ERROR,
            "dinoco.incompleteRelationKeys",
            "`fields` and `references` must be declared together.",
        ));
        return;
    }

    let (Some(local_argument), Some(reference_argument)) = (local_argument, reference_argument) else {
        return;
    };
    let Some(local_names) = relation_identifier_values(local_argument) else {
        diagnostics.push(diagnostic(
            relation_argument_name_range(field_index, "fields"),
            DiagnosticSeverity::ERROR,
            "dinoco.invalidRelationFields",
            "`fields` must be a non-empty array of field identifiers.",
        ));
        return;
    };
    let Some(reference_names) = relation_identifier_values(reference_argument) else {
        diagnostics.push(diagnostic(
            relation_argument_name_range(field_index, "references"),
            DiagnosticSeverity::ERROR,
            "dinoco.invalidRelationReferences",
            "`references` must be a non-empty array of field identifiers.",
        ));
        return;
    };
    if local_names.len() != reference_names.len() {
        diagnostics.push(diagnostic(
            relation_range,
            DiagnosticSeverity::ERROR,
            "dinoco.relationArity",
            "`fields` and `references` must contain the same number of fields.",
        ));
        return;
    }

    for duplicate in duplicate_names(&local_names) {
        diagnostics.push(diagnostic(
            relation_argument_value_range(field_index, "fields", duplicate),
            DiagnosticSeverity::ERROR,
            "dinoco.duplicateRelationField",
            format!("Field `{duplicate}` occurs more than once in `fields`."),
        ));
    }
    for duplicate in duplicate_names(&reference_names) {
        diagnostics.push(diagnostic(
            relation_argument_value_range(field_index, "references", duplicate),
            DiagnosticSeverity::ERROR,
            "dinoco.duplicateRelationReference",
            format!("Field `{duplicate}` occurs more than once in `references`."),
        ));
    }

    for (local_name, reference_name) in local_names.iter().zip(reference_names.iter()) {
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

        if local.ty.list || local.is_relation(schema) {
            diagnostics.push(diagnostic(
                relation_argument_value_range(field_index, "fields", local_name),
                DiagnosticSeverity::ERROR,
                "dinoco.invalidRelationKey",
                format!("Relation key `{}.{}` must be a scalar or enum field.", model.name, local.name),
            ));
        }
        if reference.ty.list || reference.is_relation(schema) {
            diagnostics.push(diagnostic(
                relation_argument_value_range(field_index, "references", reference_name),
                DiagnosticSeverity::ERROR,
                "dinoco.invalidReferenceKey",
                format!("Referenced key `{}.{}` must be a scalar or enum field.", target.name, reference.name),
            ));
        }
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
    }

    for action in ["onDelete", "onUpdate"] {
        let Some(value) = relation.argument(action) else {
            continue;
        };
        if !matches!(
            attribute_ident(relation, action),
            Some("Cascade" | "Restrict" | "NoAction" | "SetNull" | "SetDefault")
        ) {
            diagnostics.push(diagnostic(
                relation_argument_name_range(field_index, action),
                DiagnosticSeverity::ERROR,
                "dinoco.invalidReferentialAction",
                format!("`{action}` must be Cascade, Restrict, NoAction, SetNull, or SetDefault."),
            ));
        }
        if !matches!(value, AttributeValue::Ident(_) | AttributeValue::String(_)) {
            diagnostics.push(diagnostic(
                relation_argument_name_range(field_index, action),
                DiagnosticSeverity::ERROR,
                "dinoco.invalidReferentialAction",
                format!("`{action}` must be a referential-action identifier."),
            ));
        }
    }
}

fn attribute_ident<'a>(attribute: &'a Attribute, name: &str) -> Option<&'a str> {
    match attribute.argument(name) {
        Some(AttributeValue::Ident(value) | AttributeValue::String(value)) => Some(value),
        _ => None,
    }
}

fn first_snowflake_range(schema: &Schema, index: &DocumentIndex) -> Option<Range> {
    for model in schema.models() {
        for field in &model.fields {
            let uses_snowflake = field
                .attributes
                .iter()
                .filter(|attribute| attribute.name == "default")
                .flat_map(|attribute| &attribute.arguments)
                .any(|argument| {
                    matches!(
                        argument,
                        AttributeArgument::Value(AttributeValue::Call { name, .. })
                            | AttributeArgument::Named {
                                value: AttributeValue::Call { name, .. },
                                ..
                            } if name == "snowflake"
                    )
                });
            if uses_snowflake {
                let field = indexed_field(index, model, field)?;
                return Some(field.attribute("default").map_or(field.range, |attribute| attribute.range));
            }
        }
    }
    None
}

fn compiler_semantic_range(index: &DocumentIndex, message: &str) -> Range {
    for model in index.blocks.iter().filter(|block| block.kind == BlockKind::Model) {
        let Some(model_name) = model.name.as_ref().map(|name| name.name.as_str()) else {
            continue;
        };
        for field in &model.fields {
            if message.contains(&format!("`{model_name}.{}`", field.name.name)) {
                return field.range;
            }
        }
    }

    default_range()
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

    #[test]
    fn reports_relation_and_snowflake_errors_together_at_their_fields() {
        let source = r#"config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
        }
        model Account {
            id       Integer @id @default(snowflake())
            business Business[]
        }
        model Business {
            id Integer @id
        }"#;
        let diagnostics = analyze(source, &DocumentIndex::new(source));

        let snowflake = diagnostics
            .iter()
            .find(|item| item.code == Some(NumberOrString::String(CODE_MISSING_SNOWFLAKE_NODE_ID.into())))
            .expect("snowflake diagnostic");
        assert_eq!(snowflake.range.start.line, 5);

        let relation = diagnostics
            .iter()
            .find(|item| item.code == Some(NumberOrString::String("dinoco.missingOppositeRelation".into())))
            .expect("opposite relation diagnostic");
        assert_eq!(relation.range.start.line, 6);
        assert!(relation.message.contains("Account.business"));
    }

    #[test]
    fn reports_relation_key_ownership_and_cardinality_problems() {
        let source = r#"model User {
            id      Integer @id
            profile Profile?
            posts   Post[]
        }
        model Profile {
            id      Integer @id
            user_id Integer?
            user    User? @relation(fields: [user_id], references: [id])
        }
        model Post {
            id      Integer @id
            user_id Integer?
            user    User @relation(fields: [user_id], references: [id])
        }"#;
        let diagnostics = analyze(source, &DocumentIndex::new(source));
        let codes = diagnostics
            .iter()
            .filter_map(|item| match &item.code {
                Some(NumberOrString::String(code)) => Some(code.as_str()),
                _ => None,
            })
            .collect::<HashSet<_>>();

        assert!(codes.contains("dinoco.oneToOneRequiresUnique"), "{diagnostics:#?}");
        assert!(codes.contains("dinoco.relationOptionalityMismatch"), "{diagnostics:#?}");
    }

    #[test]
    fn reports_unmaterializable_composite_relation_uniqueness() {
        let source = r#"model User {
            tenant Integer @unique
            id     Integer @unique
            detail Detail?
        }
        model Detail {
            id        Integer @id
            tenant_id Integer
            user_id   Integer
            user      User @unique @relation(fields: [tenant_id, user_id], references: [tenant, id])
        }"#;
        let diagnostics = analyze(source, &DocumentIndex::new(source));
        let item = diagnostics
            .iter()
            .find(|item| item.code == Some(NumberOrString::String("dinoco.compositeRelationUnique".into())))
            .expect("composite uniqueness diagnostic");
        assert!(item.message.contains("Detail.user"));
    }

    #[test]
    fn reports_invalid_inverse_mapping_and_inverse_actions() {
        let source = r#"model User {
            id     Integer @id
            legacy Integer @unique
            posts  Post[] @relation(fields: [legacy], references: [user_id], onDelete: Cascade)
        }
        model Post {
            id      Integer @id
            user_id Integer
            user    User @relation(fields: [user_id], references: [id])
        }"#;
        let diagnostics = analyze(source, &DocumentIndex::new(source));

        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == Some(NumberOrString::String("dinoco.invalidInverseRelationKeys".into())))
        );
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == Some(NumberOrString::String("dinoco.referentialOptionOnInverse".into())))
        );
    }
}
