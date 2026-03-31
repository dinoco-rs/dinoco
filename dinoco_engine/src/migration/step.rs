use dinoco_compiler::{ParsedField, ParsedTable, ReferentialAction};

#[derive(Debug, Clone)]
pub enum MigrationStep {
    CreateTable(ParsedTable),
    DropTable(String),
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
    AddForeignKey {
        table_name: String,
        column_name: String,
        referenced_table: String,
        referenced_column: String,
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
        column_name: String,
        index_name: String,
        is_unique: bool,
    },
}
