use std::collections::BTreeSet;

use dinoco_compiler::{ParsedField, ParsedFieldDefault, ParsedFieldType, ParsedRelation, ParsedSchema, ParsedTable};

pub(crate) struct ResolvedRelation<'a> {
    pub(crate) target_table_name: &'a str,
    pub(crate) local_key_field: &'a ParsedField,
    pub(crate) remote_key_field: &'a ParsedField,
    pub(crate) cardinality: RelationCardinality,
}

pub(crate) enum RelationCardinality {
    Many,
    OptionalOne,
    ManyToMany { join_table_name: String, current_join_column: String, target_join_column: String },
}

pub(crate) struct JoinTableData {
    pub(crate) model_name: String,
    pub(crate) table: ParsedTable,
}

pub(crate) fn resolve_relation<'a>(
    table: &'a ParsedTable,
    field: &'a ParsedField,
    schema: &'a ParsedSchema,
) -> Option<ResolvedRelation<'a>> {
    let ParsedFieldType::Relation(target_model_name) = &field.field_type else {
        return None;
    };
    let target_table = schema.tables.iter().find(|item| item.name == *target_model_name)?;
    if table.primary_key_fields.len() != 1 || target_table.primary_key_fields.len() != 1 {
        return None;
    }

    match &field.relation {
        ParsedRelation::OneToMany(_) | ParsedRelation::OneToOneInverse(_) => {
            let owner_field = target_table.fields.iter().find(|candidate| {
                if !matches!(candidate.field_type, ParsedFieldType::Relation(ref model_name) if model_name == &table.name)
                {
                    return false;
                }

                match (&field.relation, &candidate.relation) {
                    (
                        ParsedRelation::OneToMany(current_name),
                        ParsedRelation::ManyToOne(target_name, _, references, _, _),
                    )
                    | (
                        ParsedRelation::OneToOneInverse(current_name),
                        ParsedRelation::OneToOneOwner(target_name, _, references, _, _),
                    ) => {
                        current_name == target_name
                            && references.first().is_some_and(|value| {
                                table.primary_key_fields.first().is_some_and(|primary_key| value == primary_key)
                            })
                    }
                    _ => false,
                }
            })?;

            let (remote_key_name, local_key_name, cardinality) = match &owner_field.relation {
                ParsedRelation::ManyToOne(_, fields, references, _, _) => {
                    (fields.first()?.as_str(), references.first()?.as_str(), RelationCardinality::Many)
                }
                ParsedRelation::OneToOneOwner(_, fields, references, _, _) => {
                    (fields.first()?.as_str(), references.first()?.as_str(), RelationCardinality::OptionalOne)
                }
                _ => return None,
            };

            let local_key_field = table.fields.iter().find(|item| item.name == local_key_name)?;
            let remote_key_field = target_table.fields.iter().find(|item| item.name == remote_key_name)?;

            Some(ResolvedRelation {
                target_table_name: &target_table.database_name,
                local_key_field,
                remote_key_field,
                cardinality,
            })
        }
        ParsedRelation::ManyToOne(_, fields, references, _, _)
        | ParsedRelation::OneToOneOwner(_, fields, references, _, _) => {
            let local_key_name = fields.first()?;
            let remote_key_name = references.first()?;
            let local_key_field = table.fields.iter().find(|item| item.name == *local_key_name)?;
            let remote_key_field = target_table.fields.iter().find(|item| item.name == *remote_key_name)?;

            Some(ResolvedRelation {
                target_table_name: &target_table.database_name,
                local_key_field,
                remote_key_field,
                cardinality: RelationCardinality::OptionalOne,
            })
        }
        ParsedRelation::ManyToMany(relation_name) => {
            let (join_table_name, current_join_column, target_join_column) =
                many_to_many_join_data(table, field, target_table, relation_name.as_deref())?;
            let local_key_field = table
                .fields
                .iter()
                .find(|item| table.primary_key_fields.first().is_some_and(|primary_key| &item.name == primary_key))?;
            let remote_key_field = target_table.fields.iter().find(|item| {
                target_table.primary_key_fields.first().is_some_and(|primary_key| &item.name == primary_key)
            })?;

            Some(ResolvedRelation {
                target_table_name: &target_table.database_name,
                local_key_field,
                remote_key_field,
                cardinality: RelationCardinality::ManyToMany {
                    join_table_name,
                    current_join_column,
                    target_join_column,
                },
            })
        }
        _ => None,
    }
}

pub(crate) fn collect_join_tables(schema: &ParsedSchema) -> Vec<JoinTableData> {
    let mut join_tables = Vec::new();
    let mut seen_join_tables = BTreeSet::new();

    for table in &schema.tables {
        for field in &table.fields {
            let ParsedRelation::ManyToMany(relation_name) = &field.relation else {
                continue;
            };
            let ParsedFieldType::Relation(target_model_name) = &field.field_type else {
                continue;
            };
            let Some(target_table) = schema.tables.iter().find(|candidate| candidate.name == *target_model_name) else {
                continue;
            };
            let Some((join_table_name, current_join_column, target_join_column)) =
                many_to_many_join_data(table, field, target_table, relation_name.as_deref())
            else {
                continue;
            };

            if !seen_join_tables.insert(join_table_name.clone()) {
                continue;
            }

            let Some(current_primary_key) = table.primary_key_fields.first() else {
                continue;
            };
            let Some(target_primary_key) = target_table.primary_key_fields.first() else {
                continue;
            };
            let Some(current_primary_key_field) =
                table.fields.iter().find(|candidate| candidate.name == *current_primary_key)
            else {
                continue;
            };
            let Some(target_primary_key_field) =
                target_table.fields.iter().find(|candidate| candidate.name == *target_primary_key)
            else {
                continue;
            };

            join_tables.push(JoinTableData {
                model_name: join_table_name.trim_start_matches('_').to_string(),
                table: ParsedTable {
                    name: join_table_name.clone(),
                    database_name: join_table_name.clone(),
                    primary_key_fields: vec![current_join_column.clone(), target_join_column.clone()],
                    fields: vec![
                        ParsedField {
                            name: current_join_column,
                            field_type: current_primary_key_field.field_type.clone(),
                            is_primary_key: true,
                            is_optional: false,
                            is_unique: false,
                            is_list: false,
                            relation: ParsedRelation::NotDefined,
                            default_value: ParsedFieldDefault::NotDefined,
                        },
                        ParsedField {
                            name: target_join_column,
                            field_type: target_primary_key_field.field_type.clone(),
                            is_primary_key: true,
                            is_optional: false,
                            is_unique: false,
                            is_list: false,
                            relation: ParsedRelation::NotDefined,
                            default_value: ParsedFieldDefault::NotDefined,
                        },
                    ],
                },
            });
        }
    }

    join_tables
}

fn many_to_many_join_data(
    current_table: &ParsedTable,
    current_field: &ParsedField,
    target_table: &ParsedTable,
    relation_name: Option<&str>,
) -> Option<(String, String, String)> {
    let join_table_name =
        build_many_to_many_join_table_name(current_table, current_field, target_table, relation_name)?;
    let current_primary_key = current_table.primary_key_fields.first()?;
    let target_primary_key = target_table.primary_key_fields.first()?;
    let current_clean = current_table.database_name.replace('"', "").to_lowercase();
    let target_clean = target_table.database_name.replace('"', "").to_lowercase();

    if current_table.name == target_table.name {
        let left_column = format!("{}_A_{}", current_clean, current_primary_key);
        let right_column = format!("{}_B_{}", current_clean, target_primary_key);

        return if should_swap_self_many_to_many_columns(current_table, current_field, relation_name) {
            Some((join_table_name, right_column, left_column))
        } else {
            Some((join_table_name, left_column, right_column))
        };
    }

    Some((
        join_table_name,
        format!("{}_{}", current_clean, current_primary_key),
        format!("{}_{}", target_clean, target_primary_key),
    ))
}

fn should_swap_self_many_to_many_columns(
    table: &ParsedTable,
    field: &ParsedField,
    relation_name: Option<&str>,
) -> bool {
    let counterpart = table
        .fields
        .iter()
        .filter(|candidate| candidate.name != field.name)
        .filter(
            |candidate| matches!(candidate.field_type, ParsedFieldType::Relation(ref target) if target == &table.name),
        )
        .filter_map(|candidate| match &candidate.relation {
            ParsedRelation::ManyToMany(candidate_relation_name)
                if candidate_relation_name.as_deref() == relation_name =>
            {
                Some(candidate)
            }
            _ => None,
        })
        .min_by(|left, right| left.name.cmp(&right.name));

    counterpart.is_some_and(|candidate| field.name > candidate.name)
}

fn build_many_to_many_join_table_name(
    current_table: &ParsedTable,
    current_field: &ParsedField,
    target_table: &ParsedTable,
    relation_name: Option<&str>,
) -> Option<String> {
    if let Some(relation_name) = relation_name {
        return Some(format!("_{}", relation_name.replace('"', "")));
    }

    if current_table.name == target_table.name {
        let anchor_field = find_many_to_many_counterpart(current_table, current_field, target_table, relation_name)
            .map(|field| if current_field.name <= field.name { current_field } else { field })
            .unwrap_or(current_field);

        return Some(format!("_{}{}", current_table.name, pascal_case(&anchor_field.name)));
    }

    if current_table.name < target_table.name {
        return Some(format!("_{}{}", current_table.name, pascal_case(&current_field.name)));
    }

    let counterpart = find_many_to_many_counterpart(current_table, current_field, target_table, relation_name)?;

    Some(format!("_{}{}", target_table.name, pascal_case(&counterpart.name)))
}

fn find_many_to_many_counterpart<'a>(
    current_table: &ParsedTable,
    current_field: &ParsedField,
    target_table: &'a ParsedTable,
    relation_name: Option<&str>,
) -> Option<&'a ParsedField> {
    target_table.fields.iter().find(|candidate| {
        if !matches!(candidate.field_type, ParsedFieldType::Relation(ref target) if target == &current_table.name) {
            return false;
        }

        if current_table.name == target_table.name && candidate.name == current_field.name {
            return false;
        }

        matches!(
            &candidate.relation,
            ParsedRelation::ManyToMany(candidate_relation_name)
                if candidate_relation_name.as_deref() == relation_name
        )
    })
}

fn pascal_case(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = true;

    for ch in value.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            uppercase_next = true;
            continue;
        }

        if uppercase_next {
            output.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            output.push(ch);
        }
    }

    output
}
