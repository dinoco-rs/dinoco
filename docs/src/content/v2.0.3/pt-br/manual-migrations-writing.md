# Escrevendo migrations

Uma migration manual é um tipo que implementa `DinocoMigration`. `dinoco migrate generate <nome>` cria o arquivo para você:

```bash
dinoco migrate generate create_users
```

```rust
use ::dinoco::*;

#[dinoco(migration)]
pub struct CreateUsers;

impl DinocoMigration for CreateUsers {
    async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        Ok(())
    }

    async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

## As peças

- **`#[dinoco(migration)]`** marca o tipo como migration. O tipo em si não é alterado; o atributo adiciona uma verificação em tempo de compilação de que ele realmente implementa `DinocoMigration`, então um `impl` esquecido é reportado no tipo, e não no fundo do registro.
- **`DinocoMigration`** tem dois métodos obrigatórios, `up` e `down`. Ambos são `async fn` comuns — sem `#[async_trait]`.
- **`DinocoManager`** é o que `up`/`down` usam para alterar o banco. Ele compila cada operação para o dialeto do banco conectado (PostgreSQL, PgBouncer, MySQL ou SQLite).
- **`DbErr`** é o tipo de erro: um alias de `anyhow::Error`, então `?` funciona com qualquer erro e você pode anexar contexto com `.context(...)`.

Ambos os métodos devem terminar com `Ok(())` (ou retornar direto o último `.await`). O future retornado precisa ser `Send`, o que é verdade a menos que você mantenha um valor não-`Send` atravessando um `.await`.

## Criar uma tabela

```rust
async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .create_table(CreateTableMigration {
            table: "user".to_string(),
            if_not_exists: false,
            columns: vec![
                MigrationColumn::new("id", MigrationColumnType::String).primary_key(),
                MigrationColumn::new("email", MigrationColumnType::String).unique(),
                MigrationColumn::new("bio", MigrationColumnType::Text).nullable(),
                MigrationColumn::new("created_at", MigrationColumnType::DateTime)
                    .default(MigrationDefault::CurrentTimestamp),
            ],
            foreign_keys: Vec::new(),
        })
        .await
}

async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .drop_table(DropTableMigration { table: "user".to_string(), if_exists: false })
        .await
}
```

`MigrationColumn::new(nome, tipo)` cria uma coluna obrigatória, não única e sem default. Encadeie `.primary_key()`, `.unique()`, `.nullable()` e `.default(...)` para mudar isso. Também é possível preencher os campos da struct diretamente.

## Vários passos em uma migration

Cada chamada ao `manager` é aguardada em ordem. Desfaça-as na ordem **inversa** no `down`:

```rust
async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .add_column(AddColumnMigration {
            table: "user".to_string(),
            column: MigrationColumn::new("email", MigrationColumnType::String).nullable(),
        })
        .await?;

    manager
        .create_index(CreateIndexMigration {
            table: "user".to_string(),
            index: MigrationIndex {
                name: "user_email_idx".to_string(),
                columns: vec!["email".to_string()],
                automatic: false,
                kind: MigrationIndexKind::Standard,
            },
        })
        .await
}

async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .drop_index(DropIndexMigration {
            table: "user".to_string(),
            index: MigrationIndex {
                name: "user_email_idx".to_string(),
                columns: vec!["email".to_string()],
                automatic: false,
                kind: MigrationIndexKind::Standard,
            },
        })
        .await?;

    manager
        .drop_column(DropColumnMigration { table: "user".to_string(), column: "email".to_string() })
        .await
}
```

## Foreign keys

```rust
manager
    .add_foreign_key(AddForeignKeyMigration {
        table: "post".to_string(),
        foreign_key: MigrationForeignKey {
            name: "post_author_id_fkey".to_string(),
            columns: vec!["author_id".to_string()],
            references_table: "user".to_string(),
            references_columns: vec!["id".to_string()],
            on_update: ReferentialAction::Cascade,
            on_delete: ReferentialAction::Restrict,
        },
    })
    .await?;
```

O SQLite não consegue adicionar ou remover uma foreign key numa tabela existente, nem alterar uma coluna no lugar. Essas chamadas retornam um erro (`this operation is not supported by the connected database`) em vez de não fazer nada em silêncio — declare a foreign key em `foreign_keys` do `create_table`, ou reconstrua a tabela com SQL puro.

## Migrations de dados e SQL puro

`manager.execute(sql)` executa um comando SQL puro. Use para backfills e para tudo que os métodos tipados não cobrem:

```rust
async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .add_column(AddColumnMigration {
            table: "user".to_string(),
            column: MigrationColumn::new("display_name", MigrationColumnType::String).nullable(),
        })
        .await?;

    manager.execute("UPDATE \"user\" SET display_name = email WHERE display_name IS NULL").await
}
```

Coloque identificadores entre aspas e escreva SQL para o banco em que você faz deploy — comandos puros não são traduzidos entre dialetos. Para qualquer outra coisa que precise da conexão, `manager.backend()` retorna o `Backend` subjacente.

## Nomes, ordem e arquivos

- Os nomes das migrations são `<timestamp UTC>_<nome_em_snake_case>`, como `20260919120000_create_users`. Essa string é o que a tabela de histórico guarda, então **nunca renomeie uma migration já aplicada** — o Dinoco veria o nome antigo como aplicado, porém ausente.
- O tipo Rust é a forma PascalCase do nome (`CreateUsers`). Um nome que começaria com dígito ganha o prefixo `Migration`.
- A ordem em `migrations()` é a ordem de execução. Acrescente novas migrations no fim; não reordene as já aplicadas.
- As migrations são compiladas pelo runner (veja o [fluxo da CLI](/pt-br/docs/orm/guide/migration-workflow#como-as-migrations-sao-executadas)), então devem depender apenas de `::dinoco` — não dos módulos da sua aplicação.

## Comportamento em falhas

Uma migration só é registrada como aplicada depois que o `up` retorna `Ok`. Se falhar, nada é registrado e a execução para, mantendo as migrations anteriores aplicadas.

O Dinoco não envolve a migration em uma transação. PostgreSQL e SQLite poderiam reverter DDL, o MySQL não, e uma falha parcial se comportaria de forma diferente em cada banco. Mantenha cada migration pequena, prefira `if_not_exists`/`if_exists` em passos idempotentes e esteja pronto para corrigir à mão uma migration aplicada pela metade antes de rodar de novo.
