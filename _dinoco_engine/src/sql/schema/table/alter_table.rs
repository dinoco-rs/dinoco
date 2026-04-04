use dinoco_compiler::{ParsedEnum, ParsedTable};

use crate::{AlterAction, ColumnDefinition, ConstraintDefinition, SqlDialect};

pub struct AlterTableStatement<'a, D: SqlDialect> {
    pub table_name: &'a str,
    pub actions: Vec<AlterAction<'a>>,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> AlterTableStatement<'a, D> {
    pub fn new(dialect: &'a D, table_name: &'a str) -> Self {
        Self {
            table_name,
            actions: vec![],
            dialect,
        }
    }

    pub fn add_column(mut self, column: ColumnDefinition<'a>) -> Self {
        self.actions.push(AlterAction::AddColumn(column));

        self
    }

    pub fn drop_column(mut self, column_name: &'a str) -> Self {
        self.actions.push(AlterAction::DropColumn(column_name));

        self
    }

    pub fn modify_column(mut self, table: ParsedTable, enums: Vec<ParsedEnum>, column: ColumnDefinition<'a>) -> Self {
        self.actions.push(AlterAction::ModifyColumn(table, enums, column));

        self
    }

    pub fn rename_column(mut self, old_name: &'a str, new_name: &'a str) -> Self {
        self.actions.push(AlterAction::RenameColumn { old_name, new_name });

        self
    }

    pub fn add_constraint(mut self, table: ParsedTable, enums: Vec<ParsedEnum>, constraint: ConstraintDefinition<'a>) -> Self {
        self.actions.push(AlterAction::AddConstraint(table, enums, constraint));

        self
    }

    pub fn drop_constraint(mut self, table: ParsedTable, enums: Vec<ParsedEnum>, constraint_name: &'a str) -> Self {
        self.actions.push(AlterAction::DropConstraint(table, enums, constraint_name));

        self
    }
}
