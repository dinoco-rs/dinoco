use dinoco_compiler::{ParsedField, ParsedTable, ReferentialAction};

use crate::DinocoValue;

#[derive(Debug, Clone)]
pub enum SafetyLevel {
    Warning(String),
    Destructive(String),
}

#[derive(Debug)]
pub struct MigrationPlan {
    pub steps: Vec<MigrationStep>,
    pub safety_alerts: Vec<SafetyLevel>,
}

impl MigrationPlan {
    pub fn is_destructive(&self) -> bool {
        self.safety_alerts
            .iter()
            .any(|alert| matches!(alert, SafetyLevel::Destructive(_)))
    }

    pub fn has_warnings(&self) -> bool {
        self.safety_alerts
            .iter()
            .any(|alert| matches!(alert, SafetyLevel::Warning(_)))
    }
}

#[derive(Debug, Clone)]
pub enum MigrationStep {
    CreateTable(ParsedTable),
    RenameTable {
        old_name: String,
        new_name: String,
    },
    DropTable(String),

    CreateEnum {
        name: String,
        variants: Vec<String>,
    },
    AlterEnum {
        name: String,
        old_variants: Vec<String>,
        new_variants: Vec<String>,
    },
    DropEnum(String),

    AddColumn {
        table_name: String,
        field: ParsedField,
    },
    DropColumn {
        table_name: String,
        field: ParsedField,
    },
    AlterColumn {
        table_name: String,
        old_field: ParsedField,
        new_field: ParsedField,
    },
    RenameColumn {
        table_name: String,
        old_name: String,
        new_name: String,
    },

    AddPrimaryKey {
        table_name: String,
        columns: Vec<String>,
        constraint_name: Option<String>,
    },
    DropPrimaryKey {
        table_name: String,
        constraint_name: Option<String>,
    },

    AddForeignKey {
        table_name: String,
        columns: Vec<String>,
        referenced_table: String,
        referenced_columns: Vec<String>,
        on_delete: Option<ReferentialAction>,
        on_update: Option<ReferentialAction>,
        constraint_name: String,
    },
    DropForeignKey {
        table_name: String,
        constraint_name: String,
    },

    CreateIndex {
        table_name: String,
        columns: Vec<String>,
        index_name: String,
        is_unique: bool,
    },
    DropIndex {
        table_name: String,
        index_name: String,
    },
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
