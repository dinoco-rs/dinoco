use dinoco_compiler::{ParsedEnum, ParsedTable};

use crate::DinocoValue;

pub enum AlterAction<'a> {
    AddColumn(ColumnDefinition<'a>),
    DropColumn(&'a str),

    ModifyColumn(ParsedTable, Vec<ParsedEnum>, ColumnDefinition<'a>),

    AddConstraint(ParsedTable, Vec<ParsedEnum>, ConstraintDefinition<'a>),
    DropConstraint(ParsedTable, Vec<ParsedEnum>, &'a str),

    RenameColumn { old_name: &'a str, new_name: &'a str },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColumnDefault {
    Value(DinocoValue),
    Function(String),
    Raw(String),
    EnumValue(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColumnType {
    Integer,
    Float,
    Text,
    Boolean,
    Json,
    DateTime,
    Bytes,
    Enum(String),
    EnumInline(Vec<String>),
}

pub enum ConstraintType<'a> {
    Unique(Vec<&'a str>),
    PrimaryKey(Vec<&'a str>),
    Check(String),
    ForeignKey {
        columns: Vec<&'a str>,
        ref_table: &'a str,
        ref_columns: Vec<&'a str>,
        on_delete: Option<&'a str>,
        on_update: Option<&'a str>,
    },
}

pub struct ConstraintDefinition<'a> {
    pub name: &'a str,
    pub constraint_type: ConstraintType<'a>,
}

#[derive(Debug)]
pub struct ColumnDefinition<'a> {
    pub name: &'a str,
    pub col_type: ColumnType,
    pub primary_key: bool,
    pub not_null: bool,
    pub auto_increment: bool,
    pub default: Option<ColumnDefault>,
}

impl<'a> ColumnDefinition<'a> {
    pub fn new(name: &'a str, col_type: ColumnType) -> Self {
        Self {
            name,
            col_type,
            primary_key: false,
            not_null: true,
            auto_increment: false,
            default: None,
        }
    }

    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self.not_null = true;

        self
    }

    pub fn not_null(mut self) -> Self {
        self.not_null = true;

        self
    }

    pub fn auto_increment(mut self) -> Self {
        self.auto_increment = true;

        self
    }

    pub fn default(mut self, val: DinocoValue) -> Result<Self, String> {
        if matches!(val, DinocoValue::Null) {
            if self.not_null {
                return Err(format!("Column '{}' is NOT NULL, but the default value is Null.", self.name));
            }

            self.default = Some(ColumnDefault::Value(val));

            return Ok(self);
        }

        let is_valid = match (&self.col_type, &val) {
            (ColumnType::Integer, DinocoValue::Integer(_)) => true,
            (ColumnType::Float, DinocoValue::Float(_)) => true,
            (ColumnType::Text, DinocoValue::String(_)) => true,
            (ColumnType::Boolean, DinocoValue::Boolean(_)) => true,
            (ColumnType::Json, DinocoValue::Json(_)) => true,
            (ColumnType::DateTime, DinocoValue::DateTime(_)) => true,

            (ColumnType::Enum(_), DinocoValue::String(_)) => true,
            (ColumnType::EnumInline(variants), DinocoValue::String(s)) => variants.contains(s),

            _ => false,
        };

        if !is_valid {
            return Err(format!(
                "Type mismatch on column '{}': cannot use this default value for type {:?}",
                self.name, self.col_type
            ));
        }

        self.default = Some(ColumnDefault::Value(val));

        Ok(self)
    }

    pub fn default_function(mut self, func: &str) -> Self {
        self.default = Some(ColumnDefault::Function(func.to_string()));

        self
    }
}

impl<'a> ConstraintDefinition<'a> {
    pub fn unique(name: &'a str, columns: Vec<&'a str>) -> Self {
        Self {
            name,
            constraint_type: ConstraintType::Unique(columns),
        }
    }

    pub fn check(name: &'a str, expr: &str) -> Self {
        Self {
            name,
            constraint_type: ConstraintType::Check(expr.to_string()),
        }
    }

    pub fn foreign_key(name: &'a str, columns: Vec<&'a str>, ref_table: &'a str, ref_columns: Vec<&'a str>) -> Self {
        Self {
            name,
            constraint_type: ConstraintType::ForeignKey {
                columns,
                ref_table,
                ref_columns,
                on_delete: None,
                on_update: None,
            },
        }
    }

    pub fn on_delete(mut self, action: &'a str) -> Self {
        if let ConstraintType::ForeignKey { ref mut on_delete, .. } = self.constraint_type {
            *on_delete = Some(action);
        }

        self
    }

    pub fn on_update(mut self, action: &'a str) -> Self {
        if let ConstraintType::ForeignKey { ref mut on_update, .. } = self.constraint_type {
            *on_update = Some(action);
        }

        self
    }
}
