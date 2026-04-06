use dinoco_compiler::{ParsedField, ParsedFieldType, ParsedRelation, ParsedSchema, ParsedTable};

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
        ParsedRelation::ManyToMany(Some(relation_name)) => {
            let (join_table_name, current_join_column, target_join_column) =
                many_to_many_join_data(&table.name, target_model_name, relation_name);
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

fn many_to_many_join_data(current: &str, target: &str, relation_name: &str) -> (String, String, String) {
    let join_table_name = format!("_{}", relation_name.replace('"', ""));
    let current_clean = current.replace('"', "").to_lowercase();
    let target_clean = target.replace('"', "").to_lowercase();

    let (left_name, right_name, current_is_left) = if current <= target {
        (current_clean.clone(), target_clean.clone(), true)
    } else {
        (target_clean.clone(), current_clean.clone(), false)
    };

    let mut left_column = format!("{}_id", left_name);
    let mut right_column = format!("{}_id", right_name);

    if left_column == right_column {
        left_column = format!("{}_A_id", left_name);
        right_column = format!("{}_B_id", left_name);
    }

    if current_is_left {
        (join_table_name, left_column, right_column)
    } else {
        (join_table_name, right_column, left_column)
    }
}
