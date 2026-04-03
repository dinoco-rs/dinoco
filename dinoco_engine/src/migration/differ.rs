use dinoco_compiler::{ParsedField, ParsedFieldDefault, ParsedFieldType, ParsedRelation, ParsedSchema, ParsedTable, ReferentialAction};
use std::collections::{HashMap, HashSet};

use super::step::MigrationStep;

pub fn calculate_diff(old_schema: &Option<ParsedSchema>, new_schema: &ParsedSchema) -> Vec<MigrationStep> {
    let old_schema = old_schema.clone().unwrap_or(ParsedSchema {
        config: new_schema.config.clone(),
        enums: vec![],
        tables: vec![],
    });

    let mut create_enum_steps = Vec::new();
    let mut alter_enum_steps = Vec::new();
    let mut drop_enum_steps = Vec::new();
    let mut drop_fk_steps = Vec::new();
    let mut drop_table_steps = Vec::new();
    let mut create_table_steps = Vec::new();
    let mut add_column_steps = Vec::new();
    let mut drop_column_steps = Vec::new();
    let mut alter_column_steps = Vec::new();
    let mut create_index_steps = Vec::new();
    let mut add_fk_steps = Vec::new();

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
            create_enum_steps.push(MigrationStep::CreateEnum {
                name: (*name).clone(),
                variants: new_enum.values.clone(),
            });
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
            for step in diff_columns(old_table, new_table) {
                match step {
                    MigrationStep::AddColumn { .. } => add_column_steps.push(step),
                    MigrationStep::DropColumn { .. } => drop_column_steps.push(step),
                    MigrationStep::DropForeignKey { .. } => drop_fk_steps.push(step),
                    MigrationStep::AlterColumn { .. } | MigrationStep::RenameColumn { .. } => alter_column_steps.push(step),
                    _ => {}
                }
            }
        } else {
            create_table_steps.push(MigrationStep::CreateTable((*new_table).clone()));
        }

        let (relations_steps, join_tables) = extract_relations(old_map.get(name).copied(), new_table, &new_schema.tables);

        for step in relations_steps {
            match step {
                MigrationStep::AddForeignKey { .. } => add_fk_steps.push(step),
                MigrationStep::DropForeignKey { .. } => drop_fk_steps.push(step),
                MigrationStep::CreateIndex { .. } => create_index_steps.push(step),
                _ => {}
            }
        }

        for join_table in join_tables {
            new_join_tables_map.insert(join_table.name.clone(), join_table.clone());

            if !old_map.contains_key(&join_table.name) && !old_join_tables.contains_key(&join_table.name) {
                create_table_steps.push(MigrationStep::CreateTable(join_table));
            }
        }
    }

    for (name, old_table) in &old_map {
        if !new_map.contains_key(*name) {
            for field in &old_table.fields {
                if let ParsedRelation::ManyToOne(_, local_cols, _, _, _) | ParsedRelation::OneToOneOwner(_, local_cols, _, _, _) = &field.relation {
                    if let Some(local_col) = local_cols.first() {
                        drop_fk_steps.push(MigrationStep::DropForeignKey {
                            table_name: old_table.name.clone(),
                            constraint_name: format!("fk_{}_{}", old_table.name, local_col),
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
            drop_table_steps.push(MigrationStep::DropTable((*name).clone()));
        }
    }

    let mut final_steps = Vec::new();

    final_steps.extend(create_enum_steps);
    final_steps.extend(alter_enum_steps);
    final_steps.extend(drop_fk_steps);
    final_steps.extend(drop_table_steps);
    final_steps.extend(create_table_steps);
    final_steps.extend(add_column_steps);
    final_steps.extend(drop_column_steps);
    final_steps.extend(alter_column_steps);
    final_steps.extend(create_index_steps);
    final_steps.extend(add_fk_steps);
    final_steps.extend(drop_enum_steps);

    final_steps
}

fn diff_columns(old_table: &ParsedTable, new_table: &ParsedTable) -> Vec<MigrationStep> {
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
            if old_field.field_type != new_field.field_type || old_field.is_optional != new_field.is_optional || old_field.default_value != new_field.default_value {
                steps.push(MigrationStep::AlterColumn {
                    table_name: new_table.name.clone(),
                    old_field: (*old_field).clone(),
                    new_field: (*new_field).clone(),
                });
            }
        } else {
            added_fields.push(*new_field);
        }
    }

    for (name, old_field) in &old_fields {
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
                table_name: new_table.name.clone(),
                old_name: drop_f.name.clone(),
                new_name: add_f.name.clone(),
            });

            if let ParsedRelation::ManyToOne(_, local_cols, _, _, _) | ParsedRelation::OneToOneOwner(_, local_cols, _, _, _) = &drop_f.relation {
                if let Some(local_col) = local_cols.first() {
                    steps.push(MigrationStep::DropForeignKey {
                        table_name: old_table.name.clone(),
                        constraint_name: format!("fk_{}_{}", old_table.name, local_col),
                    });
                }
            }

            resolved_adds.insert(add_f.name.clone());
            resolved_drops.insert(drop_f.name.clone());
        }
    }

    for add_f in added_fields {
        if !resolved_adds.contains(&add_f.name) {
            steps.push(MigrationStep::AddColumn {
                table_name: new_table.name.clone(),
                field: add_f.clone(),
            });
        }
    }

    for drop_f in dropped_fields {
        if !resolved_drops.contains(&drop_f.name) {
            if let ParsedRelation::ManyToOne(_, local_cols, _, _, _) | ParsedRelation::OneToOneOwner(_, local_cols, _, _, _) = &drop_f.relation {
                if let Some(local_col) = local_cols.first() {
                    steps.push(MigrationStep::DropForeignKey {
                        table_name: old_table.name.clone(),
                        constraint_name: format!("fk_{}_{}", old_table.name, local_col),
                    });
                }
            }

            if !matches!(drop_f.field_type, ParsedFieldType::Relation(_)) {
                steps.push(MigrationStep::DropColumn {
                    table_name: old_table.name.clone(),
                    field: drop_f.clone(),
                });
            }
        }
    }

    steps
}

pub fn extract_relations(old_table: Option<&ParsedTable>, new_table: &ParsedTable, all_tables: &Vec<ParsedTable>) -> (Vec<MigrationStep>, Vec<ParsedTable>) {
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
            ParsedRelation::ManyToOne(_, local_cols, ref_cols, on_delete, on_update) | ParsedRelation::OneToOneOwner(_, local_cols, ref_cols, on_delete, on_update) => {
                if is_unchanged {
                    continue;
                }

                if let (Some(local_col), Some(ref_col)) = (local_cols.first(), ref_cols.first()) {
                    let ref_table = match &field.field_type {
                        ParsedFieldType::Relation(name) => name.clone(),
                        _ => field.field_type.to_string(),
                    };

                    fk_steps.push(MigrationStep::AddForeignKey {
                        table_name: new_table.name.clone(),
                        column_name: local_col.clone(),
                        referenced_table: ref_table,
                        referenced_column: ref_col.clone(),
                        on_delete: on_delete.clone(),
                        on_update: on_update.clone(),
                        constraint_name: format!("fk_{}_{}", new_table.name, local_col),
                    });
                }
            }
            ParsedRelation::ManyToMany(Some(relation_name)) => {
                let target_table_name = match &field.field_type {
                    ParsedFieldType::Relation(name) => name.clone(),
                    _ => continue,
                };

                if new_table.name <= target_table_name {
                    let safe_rel_name = format!("_{}", relation_name.replace("\"", ""));

                    if !processed_m2m.insert(safe_rel_name.clone()) {
                        continue;
                    }

                    let t1_clean = new_table.name.replace("\"", "").to_lowercase();
                    let t2_clean = target_table_name.replace("\"", "").to_lowercase();

                    let mut col_a = format!("{}_id", t1_clean);
                    let mut col_b = format!("{}_id", t2_clean);

                    if col_a == col_b {
                        col_a = format!("{}_A_id", t1_clean);
                        col_b = format!("{}_B_id", t1_clean);
                    }

                    let pk_a_type = new_table
                        .fields
                        .iter()
                        .find(|f| f.is_primary_key)
                        .map(|f| f.field_type.clone())
                        .unwrap_or(ParsedFieldType::Integer);

                    let pk_b_type = all_tables
                        .iter()
                        .find(|t| t.name == target_table_name)
                        .and_then(|t| t.fields.iter().find(|f| f.is_primary_key))
                        .map(|f| f.field_type.clone())
                        .unwrap_or(ParsedFieldType::Integer);

                    let join_table = ParsedTable {
                        name: safe_rel_name.clone(),
                        fields: vec![
                            ParsedField {
                                name: col_a.clone(),
                                field_type: pk_a_type,
                                is_primary_key: true,
                                is_optional: false,
                                is_unique: false,
                                is_list: false,
                                relation: ParsedRelation::NotDefined,
                                default_value: ParsedFieldDefault::NotDefined,
                            },
                            ParsedField {
                                name: col_b.clone(),
                                field_type: pk_b_type,
                                is_primary_key: true,
                                is_optional: false,
                                is_unique: false,
                                is_list: false,
                                relation: ParsedRelation::NotDefined,
                                default_value: ParsedFieldDefault::NotDefined,
                            },
                        ],
                    };

                    join_tables.push(join_table);

                    if is_unchanged {
                        continue;
                    }

                    fk_steps.push(MigrationStep::AddForeignKey {
                        table_name: safe_rel_name.clone(),
                        column_name: col_a.clone(),
                        referenced_table: new_table.name.clone(),
                        referenced_column: "id".to_string(),
                        on_delete: Some(ReferentialAction::Cascade),
                        on_update: Some(ReferentialAction::Cascade),
                        constraint_name: format!("fk_{}_{}", safe_rel_name, col_a),
                    });

                    fk_steps.push(MigrationStep::AddForeignKey {
                        table_name: safe_rel_name.clone(),
                        column_name: col_b.clone(),
                        referenced_table: target_table_name.clone(),
                        referenced_column: "id".to_string(),
                        on_delete: Some(ReferentialAction::Cascade),
                        on_update: Some(ReferentialAction::Cascade),
                        constraint_name: format!("fk_{}_{}", safe_rel_name, col_b),
                    });

                    let index_name = format!("{}_{}_idx", safe_rel_name.replace("\"", ""), col_b);

                    fk_steps.push(MigrationStep::CreateIndex {
                        table_name: safe_rel_name.clone(),
                        column_name: col_b,
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
