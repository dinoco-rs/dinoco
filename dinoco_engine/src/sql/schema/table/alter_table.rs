use crate::{AlterAction, ColumnDefault, ColumnDefinition, ConstraintDefinition, DinocoValue, SqlBuilder, SqlDialect};

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

    pub fn modify_column(mut self, column: ColumnDefinition<'a>) -> Self {
        self.actions.push(AlterAction::ModifyColumn(column));

        self
    }

    pub fn rename_column(mut self, old_name: &'a str, new_name: &'a str) -> Self {
        self.actions.push(AlterAction::RenameColumn { old_name, new_name });

        self
    }

    pub fn add_constraint(mut self, constraint: ConstraintDefinition<'a>) -> Self {
        self.actions.push(AlterAction::AddConstraint(constraint));

        self
    }

    pub fn drop_constraint(mut self, constraint_name: &'a str) -> Self {
        self.actions.push(AlterAction::DropConstraint(constraint_name));

        self
    }
}
