use pest::Span;
use std::collections::{HashMap, HashSet};

use crate::{
    ConnectionUrl, Database, ParsedConfig, ParsedEnum, ParsedField, ParsedFieldDefault, ParsedFieldType, ParsedRelation, ParsedSchema, ParsedTable, ReferentialAction,
    ast::*,
    parser::{format_span_error, format_span_errors},
};

pub fn validate_schema(schema: &Schema) -> DinocoCompilerResult<ParsedSchema> {
    let mut names = HashSet::new();

    let config = validate_configs(&schema.configs, schema.span)?;
    let enums = validate_enums(&schema.enums, &mut names)?;
    let mut tables = validate_tables(&schema.tables, &enums, &mut names)?;

    validate_relations(&mut tables, &schema.tables)?;

    Ok(ParsedSchema { enums, config, tables })
}

fn validate_configs(configs: &Vec<Config>, schema_span: Span) -> DinocoCompilerResult<ParsedConfig> {
    if configs.is_empty() {
        return Err(format_span_error("Your schema must define a 'config { ... }' block.".to_string(), schema_span));
    }

    if configs.len() > 1 {
        return Err(format_span_error("Your schema must define only one 'config { ... }' block.".to_string(), schema_span));
    }

    fn parse_function(x: &Vec<ConfigValue<'_>>, span: Span) -> DinocoCompilerResult<ConnectionUrl> {
        let var = x
            .first()
            .and_then(|v| if let ConfigValue::String(s, _) = v { Some(s.clone()) } else { None })
            .ok_or_else(|| format_span_error("env() inside replicas expects a string.".to_string(), span))?;

        Ok(ConnectionUrl::Env(var))
    }

    let config = &configs[0];

    let mut database = None;
    let mut database_url = None;
    let mut read_replicas = vec![];

    let mut keys = HashSet::new();

    for field in &config.fields {
        let value = field
            .value
            .as_ref()
            .ok_or_else(|| format_span_error(format!("The config field '{}' is missing a value.", field.name), field.span))?;

        match field.name.as_str() {
            "database" => {
                if !keys.insert("database") {
                    return Err(format_span_error("Duplicate 'database'.".to_string(), field.span));
                }

                if let ConfigValue::String(db, _) = value {
                    match db.as_str() {
                        "mysql" => database = Some(Database::Mysql),
                        "postgresql" => database = Some(Database::Postgresql),
                        "sqlite" => database = Some(Database::Sqlite),
                        _ => return Err(format_span_error(format!("Unsupported database '{}'.", db), field.span)),
                    }
                } else {
                    return Err(format_span_error("'database' must be a string.".to_string(), field.span));
                }
            }
            "database_url" => {
                if !keys.insert("database_url") {
                    return Err(format_span_error("Duplicate 'database_url'.".to_string(), field.span));
                }

                match value {
                    ConfigValue::String(url_str, _) => {
                        let conn = ConnectionUrl::Literal(url_str.clone());
                        if !conn.is_valid() {
                            return Err(format_span_error("Database connection URL must start with a valid protocol.".to_string(), field.span));
                        }

                        database_url = Some(conn);
                    }

                    ConfigValue::Function { name, args, .. } if name == "env" => {
                        database_url = Some(parse_function(args, field.span)?);
                    }

                    _ => return Err(format_span_error("'database_url' must be string or env().".to_string(), field.span)),
                }
            }
            "read_replicas" => {
                if !keys.insert("read_replicas") {
                    return Err(format_span_error("Duplicate 'read_replicas'.".to_string(), field.span));
                }

                if let ConfigValue::Array(items, _) = value {
                    for item in items {
                        match item {
                            ConfigValue::String(s, _) => {
                                let conn = ConnectionUrl::Literal(s.clone());
                                if !conn.is_valid() {
                                    return Err(format_span_error("Replica connection URLS must start with a valid protocol.".to_string(), field.span));
                                }

                                read_replicas.push(conn)
                            }
                            ConfigValue::Function { name, args, .. } if name == "env" => {
                                read_replicas.push(parse_function(args, field.span)?);
                            }
                            _ => return Err(format_span_error("Replicas must be an array of strings or env().".to_string(), field.span)),
                        }
                    }
                } else {
                    return Err(format_span_error("'read_replicas' must be an array of connection URLs.".to_string(), field.span));
                }
            }
            _ => return Err(format_span_error(format!("'{}' is not a valid configuration key.", field.name), field.span)),
        }
    }

    let database = database.ok_or_else(|| format_span_error("'database' is required.".to_string(), config.span))?;
    let database_url = database_url.ok_or_else(|| format_span_error("'database_url' is required.".to_string(), config.span))?;

    Ok(ParsedConfig {
        database,
        database_url,
        read_replicas,
    })
}

fn validate_enums<'a>(enums: &'a Vec<Enum>, names: &mut HashSet<&'a str>) -> DinocoCompilerResult<Vec<ParsedEnum>> {
    let mut parsed_enums = vec![];

    for _enum in enums {
        if !names.insert(_enum.name.as_str()) {
            return Err(format_span_error(format!("Name '{}' is already in use", _enum.name), _enum.span));
        }

        let mut enum_values = HashSet::new();
        let mut parsed_values = Vec::new();

        for (_, v) in &_enum.values {
            let value = v.as_str();

            if !enum_values.insert(value) {
                return Err(format_span_error(format!("Duplicate value '{}' in enum '{}'.", value, _enum.name), _enum.span));
            }

            parsed_values.push(value.to_string());
        }

        parsed_enums.push(ParsedEnum {
            name: _enum.name.clone(),
            values: parsed_values,
        });
    }

    Ok(parsed_enums)
}

fn validate_tables<'a>(tables: &'a Vec<Table>, enums: &'a Vec<ParsedEnum>, names: &mut HashSet<&'a str>) -> DinocoCompilerResult<Vec<ParsedTable>> {
    let mut parsed_tables = vec![];

    for table in tables {
        if !names.insert(table.name.as_str()) {
            return Err(format_span_error(format!("Name '{}' is already in use", table.name), table.span));
        }

        let mut field_names = HashSet::new();
        let mut has_primary_key = false;
        let mut parsed_fields = vec![];

        for field in &table.fields {
            if !field_names.insert(&field.name) {
                return Err(format_span_error(format!("Duplicate field name '{}'", field.name), field.span));
            }

            if field.is_primary_key {
                if has_primary_key {
                    return Err(format_span_error("Multiple primary keys (@id) found.".to_string(), field.span));
                }

                has_primary_key = true;
            }

            let mut parsed_field = ParsedField {
                name: field.name.clone(),
                field_type: ParsedFieldType::String,
                default_value: ParsedFieldDefault::NotDefined,
                relation: ParsedRelation::NotDefined,

                is_primary_key: field.is_primary_key,
                is_optional: field.is_optional,
                is_unique: field.is_unique,
                is_list: field.is_list,
            };

            match &field.field_type {
                FieldType::Custom(name) => {
                    let is_enum = enums.iter().any(|e| &e.name == name);
                    let is_table = tables.iter().any(|t| &t.name == name);

                    if !is_enum && !is_table {
                        return Err(format_span_error(format!("Type '{}' does not exist.", name), field.span));
                    }

                    if is_enum {
                        parsed_field.field_type = ParsedFieldType::Enum(name.clone());
                    }

                    if is_table {
                        parsed_field.field_type = ParsedFieldType::Relation(name.clone());
                    }
                }
                FieldType::String => parsed_field.field_type = ParsedFieldType::String,
                FieldType::Integer => parsed_field.field_type = ParsedFieldType::Integer,
                FieldType::Boolean => parsed_field.field_type = ParsedFieldType::Boolean,
                FieldType::Float => parsed_field.field_type = ParsedFieldType::Float,
                FieldType::DateTime => parsed_field.field_type = ParsedFieldType::DateTime,
                FieldType::Json => parsed_field.field_type = ParsedFieldType::Json,
            }

            match field.default_value.clone() {
                FieldDefaultValue::NotDefined => {}
                FieldDefaultValue::Boolean(v) => {
                    if !matches!(parsed_field.field_type, ParsedFieldType::Boolean) {
                        return Err(format_span_error(
                            format!(
                                "Invalid default value for field '{}'. Expected type '{}', but got incompatible value.",
                                field.name,
                                parsed_field.field_type.to_string()
                            ),
                            field.span,
                        ));
                    }

                    parsed_field.default_value = ParsedFieldDefault::Boolean(v);
                }
                FieldDefaultValue::Integer(v) => {
                    if !matches!(parsed_field.field_type, ParsedFieldType::Integer) {
                        return Err(format_span_error(
                            format!(
                                "Invalid default value for field '{}'. Expected type '{}', but got incompatible value.",
                                field.name,
                                parsed_field.field_type.to_string()
                            ),
                            field.span,
                        ));
                    }

                    parsed_field.default_value = ParsedFieldDefault::Integer(v);
                }
                FieldDefaultValue::Float(v) => {
                    if !matches!(parsed_field.field_type, ParsedFieldType::Float) {
                        return Err(format_span_error(
                            format!(
                                "Invalid default value for field '{}'. Expected type '{}', but got incompatible value.",
                                field.name,
                                parsed_field.field_type.to_string()
                            ),
                            field.span,
                        ));
                    }

                    parsed_field.default_value = ParsedFieldDefault::Float(v);
                }
                FieldDefaultValue::String(v) => {
                    if !matches!(parsed_field.field_type, ParsedFieldType::String) {
                        return Err(format_span_error(
                            format!(
                                "Invalid default value for field '{}'. Expected type '{}', but got incompatible value.",
                                field.name,
                                parsed_field.field_type.to_string()
                            ),
                            field.span,
                        ));
                    }

                    parsed_field.default_value = ParsedFieldDefault::String(v);
                }
                FieldDefaultValue::Function(function) => {
                    match function {
                        FunctionCall::Snowflake | FunctionCall::AutoIncrement => {
                            if !matches!(field.field_type, FieldType::Integer) {
                                return Err(format_span_error(
                                    "Snowflake and autoincrement is only supported for Integer fields.".to_string(),
                                    field.span,
                                ));
                            }

                            if matches!(function, FunctionCall::AutoIncrement) && !field.is_primary_key {
                                return Err(format_span_error("Autoincrement is only supported on primary key fields (@id).".to_string(), field.span));
                            }
                        }

                        FunctionCall::Uuid => {
                            if !matches!(field.field_type, FieldType::String) {
                                return Err(format_span_error("UUID is only supported for String fields.".to_string(), field.span));
                            }
                        }
                        FunctionCall::Now => {
                            if !matches!(field.field_type, FieldType::DateTime) {
                                return Err(format_span_error("now() is only supported for DateTime fields.".to_string(), field.span));
                            }
                        }

                        FunctionCall::Env(..) => {
                            return Err(format_span_error(
                                "Unsupported @default() function. Supported: snowflake(), uuid(), autoincrement().".to_string(),
                                field.span,
                            ));
                        }
                    }

                    parsed_field.default_value = ParsedFieldDefault::Function(function);
                }
                FieldDefaultValue::Custom(val) => {
                    if let ParsedFieldType::Enum(name) = &parsed_field.field_type {
                        let _enum = enums.iter().find(|e| e.name == *name).unwrap();
                        if !_enum.values.contains(&val) {
                            return Err(format_span_error(format!("Invalid default value '{}' for enum '{}'", val, name), field.span));
                        }

                        parsed_field.default_value = ParsedFieldDefault::EnumValue(val.to_string())
                    } else {
                        return Err(format_span_error(format!("Invalid default value '{}'", val), field.span));
                    }
                }
            }

            parsed_fields.push(parsed_field);
        }

        if !has_primary_key {
            return Err(format_span_error("This table must have a primary key (@id).".to_string(), table.span));
        }

        parsed_tables.push(ParsedTable {
            name: table.name.clone(),
            fields: parsed_fields,
        })
    }

    Ok(parsed_tables)
}

fn validate_relations(parsed_tables: &mut Vec<ParsedTable>, schema_tables: &[Table]) -> DinocoCompilerResult<()> {
    fn get_relation_name(field: &Field<'_>) -> Option<String> {
        if let Some(rel) = &field.relation {
            if let Some(v) = rel.named_params.get("name") {
                return v.first().map(|x| x.to_string());
            }
        }
        None
    }

    fn get_relation_fields_and_references(field: &Field<'_>) -> (Vec<String>, Vec<String>) {
        if let Some(rel) = &field.relation {
            let fields = rel.named_params.get("fields").cloned().unwrap_or_default();
            let references = rel.named_params.get("references").cloned().unwrap_or_default();
            (fields, references)
        } else {
            (vec![], vec![])
        }
    }

    fn has_fields_or_references(field: &Field<'_>) -> bool {
        let (f, r) = get_relation_fields_and_references(field);
        !f.is_empty() || !r.is_empty()
    }

    fn validate_types_and_keys(ast_field: &Field<'_>, ast_table: &Table, target_ast_table: &Table) -> DinocoCompilerResult<()> {
        let (fields, references) = get_relation_fields_and_references(ast_field);

        if fields.len() != references.len() {
            return Err(format_span_error("fields and references arrays must have the same length.".into(), ast_field.span));
        }

        for (local_f, remote_f) in fields.iter().zip(references.iter()) {
            let local_ast_field = ast_table
                .fields
                .iter()
                .find(|f| &f.name == local_f)
                .ok_or_else(|| format_span_error(format!("Field '{}' not found in model '{}'.", local_f, ast_table.name), ast_field.span))?;

            let remote_ast_field = target_ast_table
                .fields
                .iter()
                .find(|f| &f.name == remote_f)
                .ok_or_else(|| format_span_error(format!("Field '{}' not found in model '{}'.", remote_f, target_ast_table.name), ast_field.span))?;

            if local_ast_field.field_type != remote_ast_field.field_type {
                return Err(format_span_error(
                    format!(
                        "Type mismatch: relation field '{}' is of type '{:?}' but references '{}' which is '{:?}'.",
                        local_f, local_ast_field.field_type, remote_f, remote_ast_field.field_type
                    ),
                    ast_field.span,
                ));
            }

            if !remote_ast_field.is_unique && !remote_ast_field.is_primary_key {
                return Err(format_span_error(
                    format!(
                        "The `references` field '{}' on model '{}' must be marked as @unique or @id.",
                        remote_f, target_ast_table.name
                    ),
                    ast_field.span,
                ));
            }
        }
        Ok(())
    }

    fn get_referential_actions(field: &Field<'_>) -> DinocoCompilerResult<(Option<ReferentialAction>, Option<ReferentialAction>)> {
        fn parse_action(action_value: Option<&String>, action_name: &str, span: pest::Span) -> DinocoCompilerResult<Option<ReferentialAction>> {
            match action_value {
                Some(val) => match val.as_str() {
                    "Cascade" => Ok(Some(ReferentialAction::Cascade)),
                    "SetNull" => Ok(Some(ReferentialAction::SetNull)),
                    "SetDefault" => Ok(Some(ReferentialAction::SetDefault)),
                    _ => Err(format_span_error(
                        format!(
                            "Valor inválido para {}: '{}'. As únicas opções permitidas são: Cascade, SetNull ou SetDefault.",
                            action_name, val
                        ),
                        span,
                    )),
                },
                None => Ok(None),
            }
        }

        if let Some(rel) = &field.relation {
            let on_update_str = rel.named_params.get("onUpdate").and_then(|v| v.first());
            let on_delete_str = rel.named_params.get("onDelete").and_then(|v| v.first());

            let on_update = parse_action(on_update_str, "onUpdate", field.span)?;
            let on_delete = parse_action(on_delete_str, "onDelete", field.span)?;

            Ok((on_delete, on_update))
        } else {
            Ok((None, None))
        }
    }

    for i in 0..parsed_tables.len() {
        let current_table_name = parsed_tables[i].name.clone();
        let ast_table = schema_tables.iter().find(|t| t.name == current_table_name).unwrap();

        let mut used_foreign_keys: HashSet<String> = HashSet::new();
        let mut used_relations_name: HashMap<String, Vec<String>> = HashMap::new();

        for j in 0..parsed_tables[i].fields.len() {
            let parsed_field_name = parsed_tables[i].fields[j].name.clone();
            let field_type = parsed_tables[i].fields[j].field_type.clone();

            if let ParsedFieldType::Relation(target_model_name) = &field_type {
                let ast_field = ast_table.fields.iter().find(|x| x.name == parsed_field_name).unwrap();
                let target_ast_table = schema_tables.iter().find(|t| t.name == *target_model_name).unwrap();

                let (fields, _) = get_relation_fields_and_references(ast_field);
                let (on_delete, on_update) = get_referential_actions(ast_field)?;

                for local_f in &fields {
                    if !used_foreign_keys.insert(local_f.clone()) {
                        return Err(format_span_error(
                            format!(
                                "The local field '{}' is already being used as a foreign key in another relation. A field can only be assigned to one @relation.",
                                local_f
                            ),
                            ast_field.span,
                        ));
                    }
                }

                let current_rel_name = get_relation_name(ast_field);

                let back_relation_fields: Vec<_> = target_ast_table
                    .fields
                    .iter()
                    .filter(|f| if let FieldType::Custom(m) = &f.field_type { m == &current_table_name } else { false })
                    .collect();

                if back_relation_fields.len() > 1 && current_rel_name.is_none() {
                    return Err(format_span_error(
                        format!(
                            "Ambiguous relation between '{}' and '{}'. There are multiple relations, so you must use @relation(name: \"...\") to disambiguate.",
                            current_table_name, target_model_name
                        ),
                        ast_field.span,
                    ));
                }

                let target_ast_field = if let Some(ref name) = current_rel_name {
                    let entry = used_relations_name.entry(name.to_string()).or_default();
                    entry.push(ast_field.name.clone());

                    if entry.len() > 1 {
                        let fields_with_same_name: Vec<&Field> = ast_table.fields.iter().filter(|f| entry.contains(&f.name)).collect();

                        let is_self_relation = fields_with_same_name.iter().all(|f| {
                            if let FieldType::Custom(target) = &f.field_type {
                                target == &current_table_name
                            } else {
                                false
                            }
                        });

                        if !is_self_relation || entry.len() > 2 {
                            let errors = fields_with_same_name
								.iter()
								.map(|f| {
									(
										format!(
											"The relation name '{}' is used multiple times (conflict on field '{}'). Each @relation must have a unique name unless it's a self-relation.",
											name, f.name
										),
										f.span,
									)
								})
								.collect::<Vec<(String, Span)>>();

                            return Err(format_span_errors(errors));
                        }
                    }

                    match back_relation_fields.iter().find(|f| get_relation_name(f).as_ref() == Some(name)) {
                        Some(f) => *f,
                        None => {
                            let candidate_field = if back_relation_fields.len() == 1 {
                                Some(back_relation_fields[0])
                            } else {
                                back_relation_fields.iter().find(|f| get_relation_name(f).is_none()).copied()
                            };

                            if let Some(candidate) = candidate_field {
                                return Err(format_span_errors(vec![
                                    (
                                        format!(
                                            "Incomplete relation: Missing the opposite field in model '{}' with @relation(name: {}).",
                                            target_model_name, name
                                        ),
                                        ast_field.span,
                                    ),
                                    (format!("You need to define or fix the relation name here to match {}.", name), candidate.span),
                                ]));
                            } else {
                                return Err(format_span_errors(vec![
                                    (
                                        format!(
                                            "Missing relation: The opposite field is not defined in model '{}' for relation {}.",
                                            target_model_name, name
                                        ),
                                        ast_field.span,
                                    ),
                                    (
                                        format!("Add a field here pointing to '{}' with @relation(name: {}).", current_table_name, name),
                                        target_ast_table.span,
                                    ),
                                ]));
                            }
                        }
                    }
                } else {
                    if back_relation_fields.is_empty() {
                        return Err(format_span_error(
                            format!("Missing back-relation field on model '{}' pointing back to '{}'.", target_model_name, current_table_name),
                            ast_field.span,
                        ));
                    }

                    back_relation_fields[0]
                };

                let is_local_list = ast_field.is_list;
                let is_remote_list = target_ast_field.is_list;

                let (fields, references) = get_relation_fields_and_references(ast_field);
                let mut parsed_rel = ParsedRelation::NotDefined;

                if is_local_list && is_remote_list {
                    if has_fields_or_references(ast_field) {
                        return Err(format_span_error("Many-to-Many relations cannot define 'fields' or 'references'.".into(), ast_field.span));
                    }

                    parsed_rel = ParsedRelation::ManyToMany(current_rel_name);
                } else if !is_local_list && is_remote_list {
                    validate_types_and_keys(ast_field, ast_table, target_ast_table)?;

                    parsed_rel = ParsedRelation::ManyToOne(current_rel_name, fields, references, on_delete, on_update);
                } else if is_local_list && !is_remote_list {
                    if has_fields_or_references(ast_field) {
                        return Err(format_span_error(
                            "The list side of a 1:N relation cannot define 'fields' or 'references'.".into(),
                            ast_field.span,
                        ));
                    }

                    parsed_rel = ParsedRelation::OneToMany(current_rel_name);
                } else {
                    let local_has_fk = has_fields_or_references(ast_field);
                    let remote_has_fk = has_fields_or_references(target_ast_field);

                    if local_has_fk && remote_has_fk {
                        return Err(format_span_error(
                            "In a 1:1 relation, only ONE side can define 'fields' and 'references'.".into(),
                            ast_field.span,
                        ));
                    } else if !local_has_fk && !remote_has_fk {
                        return Err(format_span_error(
                            "In a 1:1 relation, exactly ONE side must define 'fields' and 'references'.".into(),
                            ast_field.span,
                        ));
                    }

                    if local_has_fk {
                        validate_types_and_keys(ast_field, ast_table, target_ast_table)?;

                        let (fields, references) = get_relation_fields_and_references(ast_field);

                        for local_f in &fields {
                            let lf = ast_table.fields.iter().find(|f| &f.name == local_f).unwrap();

                            if !lf.is_unique && !lf.is_primary_key {
                                return Err(format_span_error(
                                    format!("Field '{}' must have a @unique constraint to form a valid One-to-One relation.", local_f),
                                    ast_field.span,
                                ));
                            }
                        }

                        parsed_rel = ParsedRelation::OneToOneOwner(current_rel_name, fields, references, on_delete, on_update);
                    } else {
                        parsed_rel = ParsedRelation::OneToOneInverse(current_rel_name);
                    }
                }

                parsed_tables[i].fields[j].relation = parsed_rel;
            }
        }
    }

    Ok(())
}
