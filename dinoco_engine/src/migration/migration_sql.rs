use dinoco_compiler::ParsedFieldType;

use super::mapper::map_field_to_definition;
use super::step::MigrationStep;

use crate::{
    AlterTableStatement,
    ConstraintDefinition,
    ConstraintType,
    CreateIndexStatement,
    CreateTableStatement,
    DinocoAdapter,
    DropIndexStatement,
    DropTableStatement,
    SqlDialectBuilders, // <-- Importante: trazer a trait para o escopo
    mapper::map_referential_action,
};

pub fn generate_up_sql<'a, T: DinocoAdapter>(adapter: &'a T, changes: Vec<MigrationStep>) -> Vec<String>
where
    T::Dialect: SqlDialectBuilders,
{
    let mut sql_statements = Vec::new();
    let dialect = adapter.dialect();

    for change in changes {
        match change {
            MigrationStep::CreateTable(table) => {
                let mut stmt = CreateTableStatement::new(dialect, &table.name);

                for field in &table.fields {
                    if let ParsedFieldType::Relation(_) = field.field_type {
                        continue;
                    }

                    stmt = stmt.column(map_field_to_definition(field));
                }

                let (sql, _) = dialect.build_create_table(&stmt);
                sql_statements.push(sql);
            }

            MigrationStep::DropTable(name) => {
                let stmt = DropTableStatement::new(dialect, &name);
                let (sql, _) = dialect.build_drop_table(&stmt);
                sql_statements.push(sql);
            }

            MigrationStep::AddColumn { table_name, field } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.add_column(map_field_to_definition(&field));

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::DropColumn { table_name, field } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.drop_column(&field.name);

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::AlterColumn { table_name, new_field, .. } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.modify_column(map_field_to_definition(&new_field));

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::RenameColumn { table_name, old_name, new_name } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.rename_column(&old_name, &new_name);

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::AddForeignKey {
                table_name,
                column_name,
                referenced_table,
                referenced_column,
                on_delete,
                on_update,
                constraint_name,
            } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);

                stmt = stmt.add_constraint(ConstraintDefinition {
                    name: &constraint_name,
                    constraint_type: ConstraintType::ForeignKey {
                        columns: vec![&column_name],
                        ref_table: &referenced_table,
                        ref_columns: vec![&referenced_column],
                        on_delete: map_referential_action(&on_delete),
                        on_update: map_referential_action(&on_update),
                    },
                });

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::DropForeignKey { table_name, constraint_name } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.drop_constraint(&constraint_name);

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::CreateIndex {
                table_name,
                column_name,
                index_name,
                is_unique,
            } => {
                let mut stmt = CreateIndexStatement::new(dialect, &table_name, &index_name).column(&column_name);

                if is_unique {
                    stmt = stmt.unique();
                }

                let (sql, _) = dialect.build_create_index(&stmt);
                sql_statements.push(sql);
            }
        }
    }

    sql_statements
}

pub fn generate_down_sql<T: DinocoAdapter>(adapter: &T, changes: Vec<MigrationStep>) -> String
where
    T::Dialect: SqlDialectBuilders,
{
    let mut sql_statements = Vec::new();
    let dialect = adapter.dialect();

    for change in changes.into_iter().rev() {
        match change {
            MigrationStep::CreateTable(table) => {
                let stmt = DropTableStatement::new(dialect, &table.name).cascade();
                let (sql, _) = dialect.build_drop_table(&stmt);

                sql_statements.push(sql);
            }

            MigrationStep::DropTable(name) => {
                sql_statements.push(format!(
                    "-- ERROR: Cannot accurately recreate table '{}' from a down migration without schema context.",
                    name
                ));
            }

            MigrationStep::AddColumn { table_name, field } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.drop_column(&field.name);

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::DropColumn { table_name, field } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.add_column(map_field_to_definition(&field));

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::AlterColumn { table_name, old_field, .. } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);

                stmt = stmt.modify_column(map_field_to_definition(&old_field));

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::RenameColumn { table_name, old_name, new_name } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.rename_column(&new_name, &old_name);

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::AddForeignKey { table_name, constraint_name, .. } => {
                let mut stmt = AlterTableStatement::new(dialect, &table_name);
                stmt = stmt.drop_constraint(&constraint_name);

                for (sql, _) in dialect.build_alter_table(&stmt) {
                    sql_statements.push(sql);
                }
            }

            MigrationStep::DropForeignKey { constraint_name, .. } => {
                sql_statements.push(format!("-- ERROR: Cannot accurately recreate foreign key '{}' without context.", constraint_name));
            }

            MigrationStep::CreateIndex { index_name, table_name, .. } => {
                let stmt = DropIndexStatement::new(dialect, &index_name).on_table(&table_name);

                let (sql, _) = dialect.build_drop_index(&stmt);
                sql_statements.push(sql);
            }
        }
    }

    if sql_statements.is_empty() {
        String::new()
    } else {
        format!("{};\n", sql_statements.join(";\n\n"))
    }
}

// use dinoco_compiler::ParsedFieldType;

// use super::mapper::map_field_to_definition;
// use super::step::MigrationStep;

// use crate::{
//     AlterTableStatement, ConstraintDefinition, ConstraintType, CreateIndexStatement, CreateTableStatement, DinocoAdapter, DropIndexStatement, DropTableStatement,
//     mapper::map_referential_action,
// };

// pub fn generate_up_sql<'a, T: DinocoAdapter>(adapter: &'a T, changes: Vec<MigrationStep>) -> Vec<String> {
//     let mut sql_statements = Vec::new();
//     let dialect = adapter.dialect();

//     for change in changes {
//         match change {
//             MigrationStep::CreateTable(table) => {
//                 let mut stmt = CreateTableStatement::new(dialect, &table.name);

//                 for field in &table.fields {
//                     if let ParsedFieldType::Relation(_) = field.field_type {
//                         continue;
//                     }

//                     stmt = stmt.column(map_field_to_definition(field));
//                 }

//                 let (sql, _) = stmt.to_sql();
//                 sql_statements.push(sql);
//             }

//             MigrationStep::DropTable(name) => {
//                 let (sql, _) = DropTableStatement::new(dialect, &name).to_sql();
//                 sql_statements.push(sql);
//             }

//             MigrationStep::AddColumn { table_name, field } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.add_column(map_field_to_definition(&field));

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::DropColumn { table_name, field } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.drop_column(&field.name);

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::AlterColumn { table_name, new_field, .. } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.modify_column(map_field_to_definition(&new_field));

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::RenameColumn { table_name, old_name, new_name } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.rename_column(&old_name, &new_name);

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::AddForeignKey {
//                 table_name,
//                 column_name,
//                 referenced_table,
//                 referenced_column,
//                 on_delete,
//                 on_update,
//                 constraint_name,
//             } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);

//                 stmt = stmt.add_constraint(ConstraintDefinition {
//                     name: &constraint_name,
//                     constraint_type: ConstraintType::ForeignKey {
//                         columns: vec![&column_name],
//                         ref_table: &referenced_table,
//                         ref_columns: vec![&referenced_column],
//                         on_delete: map_referential_action(&on_delete),
//                         on_update: map_referential_action(&on_update),
//                     },
//                 });

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::DropForeignKey { table_name, constraint_name } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.drop_constraint(&constraint_name);

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::CreateIndex {
//                 table_name,
//                 column_name,
//                 index_name,
//                 is_unique,
//             } => {
//                 let mut stmt = CreateIndexStatement::new(dialect, &table_name, &index_name).column(&column_name);

//                 if is_unique {
//                     stmt = stmt.unique();
//                 }

//                 let (sql, _) = stmt.to_sql();
//                 sql_statements.push(sql);
//             }
//         }
//     }

//     sql_statements
// }

// pub fn generate_down_sql<T: DinocoAdapter>(adapter: &T, changes: Vec<MigrationStep>) -> String {
//     let mut sql_statements = Vec::new();
//     let dialect = adapter.dialect();

//     for change in changes.into_iter().rev() {
//         match change {
//             MigrationStep::CreateTable(table) => {
//                 let (sql, _) = DropTableStatement::new(dialect, &table.name).cascade().to_sql();

//                 sql_statements.push(sql);
//             }

//             MigrationStep::DropTable(name) => {
//                 sql_statements.push(format!(
//                     "-- ERROR: Cannot accurately recreate table '{}' from a down migration without schema context.",
//                     name
//                 ));
//             }

//             MigrationStep::AddColumn { table_name, field } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.drop_column(&field.name);

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::DropColumn { table_name, field } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.add_column(map_field_to_definition(&field));

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::AlterColumn { table_name, old_field, .. } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);

//                 stmt = stmt.modify_column(map_field_to_definition(&old_field));

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::RenameColumn { table_name, old_name, new_name } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.rename_column(&new_name, &old_name);

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::AddForeignKey { table_name, constraint_name, .. } => {
//                 let mut stmt = AlterTableStatement::new(dialect, &table_name);
//                 stmt = stmt.drop_constraint(&constraint_name);

//                 for (sql, _) in stmt.to_sql() {
//                     sql_statements.push(sql);
//                 }
//             }

//             MigrationStep::DropForeignKey { constraint_name, .. } => {
//                 sql_statements.push(format!("-- ERROR: Cannot accurately recreate foreign key '{}' without context.", constraint_name));
//             }

//             MigrationStep::CreateIndex { index_name, table_name, .. } => {
//                 let stmt = DropIndexStatement::new(dialect, &index_name).on_table(&table_name);

//                 let (sql, _) = stmt.to_sql();
//                 sql_statements.push(sql);
//             }
//         }
//     }

//     if sql_statements.is_empty() {
//         String::new()
//     } else {
//         format!("{};\n", sql_statements.join(";\n\n"))
//     }
// }
