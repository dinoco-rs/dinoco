# Referência do manager

O `DinocoManager` é passado para `up` e `down`. Todo método retorna `Result<(), DbErr>` e compila para o SQL do banco conectado. As structs de argumento são as mesmas que o motor automático usa internamente e são reexportadas por `dinoco`.

## Métodos

| Método | Argumento | O que faz |
| --- | --- | --- |
| `create_table` | `CreateTableMigration` | `CREATE TABLE`, com colunas e foreign keys |
| `drop_table` | `DropTableMigration` | `DROP TABLE` |
| `rename_table` | `RenameTableMigration` | Renomeia uma tabela |
| `add_column` | `AddColumnMigration` | Adiciona uma coluna |
| `drop_column` | `DropColumnMigration` | Remove uma coluna |
| `alter_column` | `AlterColumnMigration` | Muda uma coluna de `current` para `desired` |
| `rename_column` | `RenameColumnMigration` | Renomeia uma coluna |
| `add_foreign_key` | `AddForeignKeyMigration` | Adiciona uma foreign key a uma tabela existente |
| `drop_foreign_key` | `DropForeignKeyMigration` | Remove uma foreign key pelo nome |
| `create_index` | `CreateIndexMigration` | Cria um índice padrão, único ou full-text |
| `drop_index` | `DropIndexMigration` | Remove um índice |
| `create_enum` | `CreateEnumMigration` | Cria um tipo enum do banco |
| `drop_enum` | `DropEnumMigration` | Remove um tipo enum do banco |
| `alter_enum` | `AlterEnumMigration` | Leva um enum de `current_values` para `desired_values` |
| `execute` | `&str` | Executa um comando SQL puro |
| `backend` | — | Retorna o `Backend` subjacente |

Algumas operações precisam de vários comandos em alguns bancos (alterar um enum no PostgreSQL, por exemplo). O manager executa todos, em ordem, por você.

### Suporte por banco

Nem todo banco faz tudo, e o manager nunca finge o contrário:

- **SQLite** não consegue `alter_column`, `add_foreign_key` nem `drop_foreign_key` numa tabela existente. Essas chamadas retornam um erro (`this operation is not supported by the connected database`) — declare foreign keys no `create_table` ou reconstrua a tabela com `execute`.
- **Enums** são tipos inline de coluna no SQLite e no MySQL, então `create_enum`, `drop_enum` e `alter_enum` não têm o que executar ali e têm sucesso sem fazer nada. No PostgreSQL eles criam, removem e alteram o tipo `ENUM`.

## Structs de argumento

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

`AlterColumnMigration` recebe a coluna `current` e a `desired` porque o MySQL redefine a coluna inteira.

## Colunas

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

Helpers: `MigrationColumn::new(nome, ty)`, `.primary_key()`, `.unique()`, `.nullable()`, `.default(default)`.

| `MigrationColumnType` | Significado |
| --- | --- |
| `String` | Texto (`TEXT` no SQLite e no PostgreSQL, `VARCHAR(255)` no MySQL) |
| `Text` | Texto, atualmente o mesmo tipo de coluna que `String` |
| `Boolean` | Booleano |
| `Integer` | Inteiro de 64 bits |
| `Float` | Ponto flutuante |
| `DateTime` | Timestamp |
| `Date` | Data de calendário |
| `Json` | Documento JSON |
| `Enum { name, values }` | Uma coluna tipada por um enum |

| `MigrationDefault` | Valor padrão |
| --- | --- |
| `String(String)` | Um literal de string |
| `Boolean(bool)` | `true`/`false` |
| `Integer(i64)` | Um literal inteiro |
| `Float(f64)` | Um literal float |
| `CurrentTimestamp` | O horário atual |
| `AutoIncrement` | Uma chave auto-incrementada |

## Foreign keys e índices

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

Use `automatic: false` para índices que você mesmo declara.

## Executando um registro pelo código

A CLI não é a única forma de rodar migrations. Estas funções recebem um `DinocoClient` e um registro (`Vec<MigrationEntry>`, o que `dinoco/migrations/mod.rs` expõe como `migrations()`):

| Função | Retorna |
| --- | --- |
| `migrate_up(&client, &entries)` | Os nomes aplicados, da mais antiga para a mais nova |
| `migrate_down(&client, &entries, steps)` | Os nomes revertidos, da mais nova para a mais antiga |
| `migration_status(&client, &entries)` | Um `Vec<MigrationStatus { name, applied }>` |

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

As três criam a tabela `dinoco_manual_migrations` no primeiro uso, recusam rodar quando o registro lista o mesmo nome duas vezes e falham quando o banco tem uma migration aplicada que não está mais registrada.
