use dinoco_compiler::{
    ParsedField, ParsedFieldDefault, ParsedFieldType, ParsedRelation, ParsedSchema, ParsedTable, ReferentialAction,
};
use std::collections::{HashMap, HashSet};

use crate::{MigrationPlan, MigrationStep, SafetyLevel, is_destructive_cast};

fn primary_key_type(table: &ParsedTable, field_name: &str) -> Option<ParsedFieldType> {
    table.fields.iter().find(|field| field.name == field_name).map(|field| field.field_type.clone())
}

fn primary_key_constraint_name(table_name: &str) -> Option<String> {
    Some(format!("pk_{}", table_name))
}

fn push_safety_alert(alerts: &mut Vec<SafetyLevel>, alert: SafetyLevel) {
    let exists = alerts.iter().any(|item| match (item, &alert) {
        (SafetyLevel::Warning(left), SafetyLevel::Warning(right)) => left == right,
        (SafetyLevel::Destructive(left), SafetyLevel::Destructive(right)) => left == right,
        _ => false,
    });

    if !exists {
        alerts.push(alert);
    }
}

fn diff_primary_key(
    old_table: &ParsedTable,
    new_table: &ParsedTable,
    alerts: &mut Vec<SafetyLevel>,
) -> Vec<MigrationStep> {
    if old_table.primary_key_fields == new_table.primary_key_fields {
        return Vec::new();
    }

    let old_primary_key = if old_table.primary_key_fields.is_empty() {
        "(none)".to_string()
    } else {
        old_table.primary_key_fields.join(", ")
    };
    let new_primary_key = if new_table.primary_key_fields.is_empty() {
        "(none)".to_string()
    } else {
        new_table.primary_key_fields.join(", ")
    };

    push_safety_alert(
        alerts,
        SafetyLevel::Destructive(format!(
            "Primary key changed on table '{}': [{}] -> [{}]. This can fail if existing data violates the new primary key or dependent relations still use the old key.",
            new_table.database_name, old_primary_key, new_primary_key
        )),
    );

    let mut steps = Vec::new();

    if !old_table.primary_key_fields.is_empty() {
        steps.push(MigrationStep::DropPrimaryKey {
            table_name: old_table.database_name.clone(),
            constraint_name: primary_key_constraint_name(&old_table.database_name),
        });
    }

    if !new_table.primary_key_fields.is_empty() {
        steps.push(MigrationStep::AddPrimaryKey {
            table_name: new_table.database_name.clone(),
            columns: new_table.primary_key_fields.clone(),
            constraint_name: primary_key_constraint_name(&new_table.database_name),
        });
    }

    steps
}

pub fn calculate_diff(old_schema: &Option<ParsedSchema>, new_schema: &ParsedSchema) -> MigrationPlan {
    let old_schema =
        old_schema.clone().unwrap_or(ParsedSchema { config: new_schema.config.clone(), enums: vec![], tables: vec![] });

    let mut safety_alerts = Vec::new();

    let mut create_enum_steps = Vec::new();
    let mut alter_enum_steps = Vec::new();
    let mut drop_enum_steps = Vec::new();
    let mut drop_fk_steps = Vec::new();
    let mut drop_table_steps = Vec::new();
    let mut create_table_steps = Vec::new();
    let mut add_column_steps = Vec::new();
    let mut drop_column_steps = Vec::new();
    let mut alter_column_steps = Vec::new();
    let mut primary_key_steps = Vec::new();
    let mut create_index_steps = Vec::new();
    let mut add_fk_steps = Vec::new();
    let mut created_table_names = HashSet::new();

    let old_enums_map: HashMap<&String, _> = old_schema.enums.iter().map(|e| (&e.name, e)).collect();
    let new_enums_map: HashMap<&String, _> = new_schema.enums.iter().map(|e| (&e.name, e)).collect();

    for (name, new_enum) in &new_enums_map {
        if let Some(old_enum) = old_enums_map.get(name) {
            if old_enum.values != new_enum.values {
                alter_enum_steps.push(MigrationStep::AlterEnum {
                    name: (*name).clone(),
                    old_variants: old_enum.values.clone(),
                    new_variants: new_enum.values.clone(),
                });
            }
        } else {
            create_enum_steps
                .push(MigrationStep::CreateEnum { name: (*name).clone(), variants: new_enum.values.clone() });
        }
    }

    for name in old_enums_map.keys() {
        if !new_enums_map.contains_key(name) {
            drop_enum_steps.push(MigrationStep::DropEnum((*name).clone()));
        }
    }

    let old_map: HashMap<&String, &ParsedTable> = old_schema.tables.iter().map(|t| (&t.name, t)).collect();
    let new_map: HashMap<&String, &ParsedTable> = new_schema.tables.iter().map(|t| (&t.name, t)).collect();

    let mut old_join_tables = HashMap::new();
    for table in &old_schema.tables {
        let (_, jts) = extract_relations(None, table, &old_schema.tables);
        for jt in jts {
            old_join_tables.insert(jt.name.clone(), jt);
        }
    }

    let mut new_join_tables_map = HashMap::new();

    for (name, new_table) in &new_map {
        if let Some(old_table) = old_map.get(name) {
            if old_table.database_name != new_table.database_name {
                push_safety_alert(
                    &mut safety_alerts,
                    SafetyLevel::Warning(format!(
                        "Table '{}' will be renamed to '{}'. Existing queries or raw SQL using the old physical table name may need updates.",
                        old_table.database_name, new_table.database_name
                    )),
                );
                created_table_names.insert(new_table.database_name.clone());
                create_table_steps.push(MigrationStep::RenameTable {
                    old_name: old_table.database_name.clone(),
                    new_name: new_table.database_name.clone(),
                });
            }

            primary_key_steps.extend(diff_primary_key(old_table, new_table, &mut safety_alerts));

            for step in diff_columns(old_table, new_table, &mut safety_alerts) {
                match step {
                    MigrationStep::AddColumn { .. } => add_column_steps.push(step),
                    MigrationStep::DropColumn { .. } => drop_column_steps.push(step),
                    MigrationStep::DropForeignKey { .. } => drop_fk_steps.push(step),
                    MigrationStep::AlterColumn { .. } | MigrationStep::RenameColumn { .. } => {
                        alter_column_steps.push(step)
                    }
                    _ => {}
                }
            }
        } else {
            create_table_steps.push(MigrationStep::CreateTable((*new_table).clone()));
            created_table_names.insert(new_table.database_name.clone());
        }

        let (relations_steps, join_tables) =
            extract_relations(old_map.get(name).copied(), new_table, &new_schema.tables);

        for step in relations_steps {
            match step {
                MigrationStep::AddForeignKey { ref table_name, .. } => {
                    if !created_table_names.contains(table_name) {
                        add_fk_steps.push(step);
                    }
                }
                MigrationStep::DropForeignKey { .. } => drop_fk_steps.push(step),
                MigrationStep::CreateIndex { .. } => create_index_steps.push(step),
                _ => {}
            }
        }

        for join_table in join_tables {
            new_join_tables_map.insert(join_table.name.clone(), join_table.clone());

            if !old_map.contains_key(&join_table.name) && !old_join_tables.contains_key(&join_table.name) {
                created_table_names.insert(join_table.database_name.clone());
                create_table_steps.push(MigrationStep::CreateTable(join_table));
            }
        }
    }

    for (name, old_table) in &old_map {
        if !new_map.contains_key(*name) {
            for field in &old_table.fields {
                if let ParsedRelation::ManyToOne(_, local_cols, _, _, _)
                | ParsedRelation::OneToOneOwner(_, local_cols, _, _, _) = &field.relation
                {
                    if !local_cols.is_empty() {
                        drop_fk_steps.push(MigrationStep::DropForeignKey {
                            table_name: old_table.database_name.clone(),
                            constraint_name: format!("fk_{}_{}", old_table.database_name, local_cols.join("_")),
                        });
                    }
                }
            }
        }
    }

    for name in old_join_tables.keys() {
        if !new_join_tables_map.contains_key(name) {
            drop_table_steps.push(MigrationStep::DropTable(name.clone()));
        }
    }

    for name in old_map.keys() {
        if !new_map.contains_key(name) {
            let old_table = old_map.get(name).unwrap();
            safety_alerts.push(SafetyLevel::Destructive(format!(
                "Dropping table '{}'. All records will be permanently deleted.",
                old_table.database_name
            )));

            drop_table_steps.push(MigrationStep::DropTable(old_table.database_name.clone()));
        }
    }

    let mut final_steps = Vec::new();

    final_steps.extend(create_enum_steps);
    final_steps.extend(drop_fk_steps);
    final_steps.extend(drop_table_steps);
    final_steps.extend(alter_enum_steps);
    final_steps.extend(create_table_steps);
    final_steps.extend(drop_column_steps);
    final_steps.extend(add_column_steps);
    final_steps.extend(alter_column_steps);
    final_steps.extend(primary_key_steps);
    final_steps.extend(create_index_steps);
    final_steps.extend(add_fk_steps);
    final_steps.extend(drop_enum_steps);

    MigrationPlan { steps: final_steps, safety_alerts }
}
fn diff_columns(old_table: &ParsedTable, new_table: &ParsedTable, alerts: &mut Vec<SafetyLevel>) -> Vec<MigrationStep> {
    let mut steps = Vec::new();

    let old_fields: HashMap<&String, &ParsedField> = old_table.fields.iter().map(|f| (&f.name, f)).collect();
    let new_fields: HashMap<&String, &ParsedField> = new_table.fields.iter().map(|f| (&f.name, f)).collect();

    let mut added_fields = Vec::new();
    let mut dropped_fields = Vec::new();

    for (name, new_field) in &new_fields {
        if matches!(new_field.field_type, ParsedFieldType::Relation(..)) {
            continue;
        }

        if let Some(old_field) = old_fields.get(name) {
            let type_changed = old_field.field_type != new_field.field_type;
            let became_required = old_field.is_optional && !new_field.is_optional;
            let default_value = old_field.default_value != new_field.default_value;
            let unique = old_field.is_unique != new_field.is_unique;

            if type_changed || became_required || default_value || unique {
                if type_changed && is_destructive_cast(&old_field.field_type, &new_field.field_type) {
                    push_safety_alert(
                        alerts,
                        SafetyLevel::Destructive(format!(
                            "Incompatible type change in '{}.{}': {:?} -> {:?}. Existing values may not be convertible.",
                            new_table.database_name, name, old_field.field_type, new_field.field_type
                        )),
                    );
                }

                if became_required && new_field.default_value == ParsedFieldDefault::NotDefined {
                    push_safety_alert(
                        alerts,
                        SafetyLevel::Warning(format!(
                            "Column '{}.{}' was changed from optional to required without a default value. This migration can fail if existing rows contain NULL.",
                            new_table.database_name, name
                        )),
                    );
                }

                steps.push(MigrationStep::AlterColumn {
                    table_name: new_table.database_name.clone(),
                    old_field: (*old_field).clone(),
                    new_field: (*new_field).clone(),
                });
            }
        } else {
            added_fields.push(*new_field);
        }
    }

    for (name, old_field) in &old_fields {
        if matches!(old_field.field_type, ParsedFieldType::Relation(..)) {
            continue;
        }

        if !new_fields.contains_key(name) {
            dropped_fields.push(*old_field);
        }
    }

    let mut resolved_adds = HashSet::new();
    let mut resolved_drops = HashSet::new();

    for add_f in added_fields.iter() {
        if let Some(drop_f) = dropped_fields.iter().find(|d| {
            !resolved_drops.contains(&d.name)
                && d.field_type == add_f.field_type
                && d.is_optional == add_f.is_optional
                && d.default_value == add_f.default_value
                && d.relation == add_f.relation
        }) {
            steps.push(MigrationStep::RenameColumn {
                table_name: new_table.database_name.clone(),
                old_name: drop_f.name.clone(),
                new_name: add_f.name.clone(),
            });

            if let ParsedRelation::ManyToOne(_, local_cols, _, _, _)
            | ParsedRelation::OneToOneOwner(_, local_cols, _, _, _) = &drop_f.relation
            {
                if !local_cols.is_empty() {
                    steps.push(MigrationStep::DropForeignKey {
                        table_name: old_table.database_name.clone(),
                        constraint_name: format!("fk_{}_{}", old_table.database_name, local_cols.join("_")),
                    });
                }
            }

            resolved_adds.insert(add_f.name.clone());
            resolved_drops.insert(drop_f.name.clone());
        }
    }

    for add_f in added_fields {
        if !resolved_adds.contains(&add_f.name) {
            if !add_f.is_optional
                && add_f.default_value == ParsedFieldDefault::NotDefined
                && old_table.fields.iter().any(|field| !matches!(field.field_type, ParsedFieldType::Relation(..)))
            {
                push_safety_alert(
                    alerts,
                    SafetyLevel::Warning(format!(
                        "Adding required column '{}.{}' without a default value can fail on tables that already contain rows.",
                        new_table.database_name, add_f.name
                    )),
                );
            }

            steps.push(MigrationStep::AddColumn { table_name: new_table.database_name.clone(), field: add_f.clone() });
        }
    }

    for drop_f in dropped_fields {
        if !resolved_drops.contains(&drop_f.name) {
            if let ParsedRelation::ManyToOne(_, local_cols, _, _, _)
            | ParsedRelation::OneToOneOwner(_, local_cols, _, _, _) = &drop_f.relation
            {
                if !local_cols.is_empty() {
                    steps.push(MigrationStep::DropForeignKey {
                        table_name: old_table.database_name.clone(),
                        constraint_name: format!("fk_{}_{}", old_table.database_name, local_cols.join("_")),
                    });
                }
            }

            push_safety_alert(
                alerts,
                SafetyLevel::Destructive(format!(
                    "Dropping column '{}.{}'. All data stored in this column will be permanently lost.",
                    old_table.database_name, drop_f.name
                )),
            );

            steps
                .push(MigrationStep::DropColumn { table_name: old_table.database_name.clone(), field: drop_f.clone() });
        }
    }

    steps
}

pub fn extract_relations(
    old_table: Option<&ParsedTable>,
    new_table: &ParsedTable,
    all_tables: &Vec<ParsedTable>,
) -> (Vec<MigrationStep>, Vec<ParsedTable>) {
    let mut fk_steps = Vec::new();
    let mut join_tables = Vec::new();

    let mut processed_m2m = HashSet::new();

    for field in &new_table.fields {
        let is_unchanged = if let Some(old_t) = old_table {
            if let Some(old_f) = old_t.fields.iter().find(|f| f.name == field.name) {
                old_f.relation == field.relation && old_f.field_type == field.field_type
            } else {
                false
            }
        } else {
            false
        };

        match &field.relation {
            ParsedRelation::ManyToOne(_, local_cols, ref_cols, on_delete, on_update)
            | ParsedRelation::OneToOneOwner(_, local_cols, ref_cols, on_delete, on_update) => {
                if is_unchanged {
                    continue;
                }

                if !local_cols.is_empty() && local_cols.len() == ref_cols.len() {
                    let ref_table = match &field.field_type {
                        ParsedFieldType::Relation(name) => name.clone(),
                        _ => field.field_type.to_string(),
                    };

                    fk_steps.push(MigrationStep::AddForeignKey {
                        table_name: new_table.database_name.clone(),
                        columns: local_cols.clone(),
                        referenced_table: all_tables
                            .iter()
                            .find(|table| table.name == ref_table)
                            .map(|table| table.database_name.clone())
                            .unwrap_or(ref_table),
                        referenced_columns: ref_cols.clone(),
                        on_delete: on_delete.clone(),
                        on_update: on_update.clone(),
                        constraint_name: format!("fk_{}_{}", new_table.database_name, local_cols.join("_")),
                    });
                }
            }
            ParsedRelation::ManyToMany(relation_name) => {
                let target_table_name = match &field.field_type {
                    ParsedFieldType::Relation(name) => name.clone(),
                    _ => continue,
                };

                if new_table.name <= target_table_name {
                    let Some(safe_rel_name) = build_many_to_many_join_table_name(
                        new_table,
                        field,
                        &target_table_name,
                        relation_name.as_deref(),
                        all_tables,
                    ) else {
                        continue;
                    };

                    if !processed_m2m.insert(safe_rel_name.clone()) {
                        continue;
                    }

                    let t1_clean = new_table.database_name.replace("\"", "").to_lowercase();
                    let t2_clean = all_tables
                        .iter()
                        .find(|table| table.name == target_table_name)
                        .map(|table| table.database_name.replace("\"", "").to_lowercase())
                        .unwrap_or_else(|| target_table_name.replace("\"", "").to_lowercase());

                    let target_table = all_tables.iter().find(|t| t.name == target_table_name);
                    let current_primary_keys = new_table.primary_key_fields.clone();
                    let target_primary_keys =
                        target_table.map(|table| table.primary_key_fields.clone()).unwrap_or_default();

                    let self_relation = new_table.name == target_table_name;
                    let current_join_columns = current_primary_keys
                        .iter()
                        .map(|field_name| {
                            if self_relation {
                                format!("{}_A_{}", t1_clean, field_name)
                            } else {
                                format!("{}_{}", t1_clean, field_name)
                            }
                        })
                        .collect::<Vec<_>>();
                    let target_join_columns = target_primary_keys
                        .iter()
                        .map(|field_name| {
                            if self_relation {
                                format!("{}_B_{}", t1_clean, field_name)
                            } else {
                                format!("{}_{}", t2_clean, field_name)
                            }
                        })
                        .collect::<Vec<_>>();

                    let mut join_fields = Vec::new();
                    for (column_name, field_name) in current_join_columns.iter().zip(current_primary_keys.iter()) {
                        join_fields.push(ParsedField {
                            name: column_name.clone(),
                            field_type: primary_key_type(new_table, field_name).unwrap_or(ParsedFieldType::Integer),
                            is_primary_key: false,
                            is_optional: false,
                            is_unique: false,
                            is_list: false,
                            relation: ParsedRelation::NotDefined,
                            default_value: ParsedFieldDefault::NotDefined,
                        });
                    }

                    if let Some(target_table) = target_table {
                        for (column_name, field_name) in target_join_columns.iter().zip(target_primary_keys.iter()) {
                            join_fields.push(ParsedField {
                                name: column_name.clone(),
                                field_type: primary_key_type(target_table, field_name)
                                    .unwrap_or(ParsedFieldType::Integer),
                                is_primary_key: false,
                                is_optional: false,
                                is_unique: false,
                                is_list: false,
                                relation: ParsedRelation::NotDefined,
                                default_value: ParsedFieldDefault::NotDefined,
                            });
                        }
                    }

                    let join_table = ParsedTable {
                        name: safe_rel_name.clone(),
                        database_name: safe_rel_name.clone(),
                        primary_key_fields: current_join_columns
                            .iter()
                            .chain(target_join_columns.iter())
                            .cloned()
                            .collect(),
                        fields: join_fields,
                    };

                    join_tables.push(join_table.clone());

                    if is_unchanged {
                        continue;
                    }

                    fk_steps.push(MigrationStep::AddForeignKey {
                        table_name: safe_rel_name.clone(),
                        columns: current_join_columns.clone(),
                        referenced_table: new_table.database_name.clone(),
                        referenced_columns: new_table.primary_key_fields.clone(),
                        on_delete: Some(ReferentialAction::Cascade),
                        on_update: Some(ReferentialAction::Cascade),
                        constraint_name: format!("fk_{}_{}", safe_rel_name, current_join_columns.join("_")),
                    });

                    fk_steps.push(MigrationStep::AddForeignKey {
                        table_name: safe_rel_name.clone(),
                        columns: target_join_columns.clone(),
                        referenced_table: all_tables
                            .iter()
                            .find(|table| table.name == target_table_name)
                            .map(|table| table.database_name.clone())
                            .unwrap_or(target_table_name.clone()),
                        referenced_columns: all_tables
                            .iter()
                            .find(|table| table.name == target_table_name)
                            .map(|table| table.primary_key_fields.clone())
                            .unwrap_or_else(|| vec!["id".to_string()]),
                        on_delete: Some(ReferentialAction::Cascade),
                        on_update: Some(ReferentialAction::Cascade),
                        constraint_name: format!("fk_{}_{}", safe_rel_name, target_join_columns.join("_")),
                    });

                    let index_name =
                        format!("{}_{}_idx", safe_rel_name.replace("\"", ""), target_join_columns.join("_"));

                    fk_steps.push(MigrationStep::CreateIndex {
                        table_name: safe_rel_name.clone(),
                        columns: target_join_columns,
                        index_name,
                        is_unique: false,
                    });
                }
            }
            _ => {}
        }
    }

    (fk_steps, join_tables)
}

fn build_many_to_many_join_table_name(
    current_table: &ParsedTable,
    current_field: &ParsedField,
    target_table_name: &str,
    relation_name: Option<&str>,
    all_tables: &[ParsedTable],
) -> Option<String> {
    if let Some(relation_name) = relation_name {
        return Some(format!("_{}", relation_name.replace('"', "")));
    }

    if current_table.name == target_table_name {
        let anchor_field =
            find_many_to_many_counterpart(current_table, current_field, target_table_name, relation_name, all_tables)
                .map(|field| if current_field.name <= field.name { current_field } else { field })
                .unwrap_or(current_field);

        return Some(format!("_{}{}", current_table.name, pascal_case(&anchor_field.name)));
    }

    if current_table.name.as_str() < target_table_name {
        return Some(format!("_{}{}", current_table.name, pascal_case(&current_field.name)));
    }

    let counterpart =
        find_many_to_many_counterpart(current_table, current_field, target_table_name, relation_name, all_tables)?;

    Some(format!("_{}{}", target_table_name, pascal_case(&counterpart.name)))
}

fn find_many_to_many_counterpart<'a>(
    current_table: &ParsedTable,
    current_field: &ParsedField,
    target_table_name: &str,
    relation_name: Option<&str>,
    all_tables: &'a [ParsedTable],
) -> Option<&'a ParsedField> {
    let target_table = all_tables.iter().find(|table| table.name == target_table_name)?;

    target_table.fields.iter().find(|candidate| {
        if !matches!(candidate.field_type, ParsedFieldType::Relation(ref target) if target == &current_table.name) {
            return false;
        }

        if current_table.name == target_table_name && candidate.name == current_field.name {
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
