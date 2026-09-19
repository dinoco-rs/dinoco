# Manager reference

`DinocoManager` is passed to `up` and `down`. Every method returns `Result<(), DbErr>` and compiles to the SQL of the connected database. The argument structs are the same ones the automatic engine uses internally, and are re-exported from `dinoco`.

## Methods

| Method | Argument | What it does |
| --- | --- | --- |
| `create_table` | `CreateTableMigration` | `CREATE TABLE`, with columns and foreign keys |
| `drop_table` | `DropTableMigration` | `DROP TABLE` |
| `rename_table` | `RenameTableMigration` | Renames a table |
| `add_column` | `AddColumnMigration` | Adds a column |
| `drop_column` | `DropColumnMigration` | Drops a column |
| `alter_column` | `AlterColumnMigration` | Changes a column from `current` to `desired` |
| `rename_column` | `RenameColumnMigration` | Renames a column |
| `add_foreign_key` | `AddForeignKeyMigration` | Adds a foreign key to an existing table |
| `drop_foreign_key` | `DropForeignKeyMigration` | Drops a foreign key by name |
| `create_index` | `CreateIndexMigration` | Creates a standard, unique, or full-text index |
| `drop_index` | `DropIndexMigration` | Drops an index |
| `create_enum` | `CreateEnumMigration` | Creates a database enum type |
| `drop_enum` | `DropEnumMigration` | Drops a database enum type |
| `alter_enum` | `AlterEnumMigration` | Moves an enum from `current_values` to `desired_values` |
| `execute` | `&str` | Runs one raw SQL statement |
| `backend` | — | Returns the underlying `Backend` |

Some operations need several statements on some databases (altering an enum on PostgreSQL, for example). The manager runs them all, in order, for you.

### Database support

Not every database can do everything, and the manager never pretends otherwise:

- **SQLite** cannot `alter_column`, `add_foreign_key`, or `drop_foreign_key` on an existing table. These return an error (`this operation is not supported by the connected database`) — declare foreign keys in `create_table`, or rebuild the table with `execute`.
- **Enums** are inline column types on SQLite and MySQL, so `create_enum`, `drop_enum`, and `alter_enum` have nothing to run there and succeed without doing anything. On PostgreSQL they create, drop, and alter the `ENUM` type.

## Argument structs

```rust
pub struct CreateTableMigration {
    pub table: String,
    pub columns: Vec<MigrationColumn>,
    pub foreign_keys: Vec<MigrationForeignKey>,
    pub if_not_exists: bool,
}

pub struct DropTableMigration { pub table: String, pub if_exists: bool }
pub struct RenameTableMigration { pub from: String, pub to: String }
pub struct AddColumnMigration { pub table: String, pub column: MigrationColumn }
pub struct DropColumnMigration { pub table: String, pub column: String }
pub struct AlterColumnMigration { pub table: String, pub current: MigrationColumn, pub desired: MigrationColumn }
pub struct RenameColumnMigration { pub table: String, pub from: String, pub to: String }
pub struct AddForeignKeyMigration { pub table: String, pub foreign_key: MigrationForeignKey }
pub struct DropForeignKeyMigration { pub table: String, pub name: String }
pub struct CreateIndexMigration { pub table: String, pub index: MigrationIndex }
pub struct DropIndexMigration { pub table: String, pub index: MigrationIndex }
pub struct CreateEnumMigration { pub name: String, pub values: Vec<String> }
pub struct DropEnumMigration { pub name: String }
pub struct AlterEnumMigration { pub name: String, pub current_values: Vec<String>, pub desired_values: Vec<String> }
```

`AlterColumnMigration` takes both the `current` and the `desired` column because SQLite rebuilds the table and MySQL redefines the whole column.

## Columns

```rust
pub struct MigrationColumn {
    pub name: String,
    pub ty: MigrationColumnType,
    pub primary_key: bool,
    pub unique: bool,
    pub nullable: bool,
    pub default: Option<MigrationDefault>,
}
```

Builder helpers: `MigrationColumn::new(name, ty)`, `.primary_key()`, `.unique()`, `.nullable()`, `.default(default)`.

| `MigrationColumnType` | Meaning |
| --- | --- |
| `String` | Text (`TEXT` on SQLite and PostgreSQL, `VARCHAR(255)` on MySQL) |
| `Text` | Text, currently the same column type as `String` |
| `Boolean` | Boolean |
| `Integer` | 64-bit integer |
| `Float` | Floating point |
| `DateTime` | Timestamp |
| `Date` | Calendar date |
| `Json` | JSON document |
| `Enum { name, values }` | A column typed by an enum |

| `MigrationDefault` | Default value |
| --- | --- |
| `String(String)` | A string literal |
| `Boolean(bool)` | `true`/`false` |
| `Integer(i64)` | An integer literal |
| `Float(f64)` | A float literal |
| `CurrentTimestamp` | The current time |
| `AutoIncrement` | An auto-incrementing key |

## Foreign keys and indexes

```rust
pub struct MigrationForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub references_table: String,
    pub references_columns: Vec<String>,
    pub on_update: ReferentialAction,
    pub on_delete: ReferentialAction,
}

pub enum ReferentialAction { Cascade, Restrict, NoAction, SetNull, SetDefault }

pub struct MigrationIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub automatic: bool,
    pub kind: MigrationIndexKind,
}

pub enum MigrationIndexKind { Standard, Unique, FullText }
```

Set `automatic: false` for indexes you declare yourself.

## Running a registry from code

The CLI is not the only way to run migrations. These functions take a `DinocoClient` and a registry (`Vec<MigrationEntry>`, what `dinoco/migrations/mod.rs` exposes as `migrations()`):

| Function | Returns |
| --- | --- |
| `migrate_up(&client, &entries)` | The names it applied, oldest first |
| `migrate_down(&client, &entries, steps)` | The names it reverted, newest first |
| `migration_status(&client, &entries)` | A `Vec<MigrationStatus { name, applied }>` |

```rust
#[path = "../dinoco/mod.rs"]
mod dinoco_generated;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = dinoco_generated::connect().await?;

    dinoco::migrate_up(&client, &dinoco_generated::migrations::migrations()).await?;

    Ok(())
}
```

All three create the `dinoco_manual_migrations` table on first use, refuse to run when the registry lists the same name twice, and fail when the database has an applied migration that is no longer registered.
