# Insert

O próprio model gerado é o payload de criação. Não existe `#[insertable]`, create struct separada nem `with_relation`.

## Crie uma entity

```rust
let mut user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
user.bio = Some("Rust developer".to_string());
```

`new()` recebe apenas fields escalares obrigatórios sem default ou generator.

## Insira uma entity

```rust
dinoco::insert_into::<User>()
    .values(&user)
    .execute(&client)
    .await?;
```

O empréstimo mantém `user` disponível após o insert. Um valor owned também é aceito.

## Insira várias entities

```rust
let users = vec![
    User::new("ana@example.com".to_string(), "Ana".to_string()),
    User::new("leo@example.com".to_string(), "Leo".to_string()),
];

dinoco::insert_many::<User>()
    .values(&users)
    .execute(&client)
    .await?;
```

Rows escalares são agrupadas em operações multi-row do adapter.

## Insira relações

Preencha diretamente o `Vec` ou `Option` da relação:

```rust
let mut user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
user.tokens = vec![UserToken::new(), UserToken::new()];
user.profile = Some(Profile::new());

dinoco::insert_into::<User>()
    .values(&user)
    .execute(&client)
    .await?;
```

One-to-many, many-to-one e one-to-one usam os metadados da relation. Em many-to-many, preencha o ID virtual antes de `insert_into` ou em cada item enviado para `insert_many`. O Dinoco não inclui esse field nas colunas SQL do endpoint; depois do insert, cria um vínculo na pivô para cada payload preenchido. Outra opção é inserir os endpoints e usar `connect`/`disconnect` nos próprios models. O ID virtual sempre volta como `None` em reads e retornos.

### Conecte many-to-many durante o insert

Em uma relação many-to-many implícita, cada endpoint recebe um `Option<Id>` virtual do model oposto. Preencha-o antes de `insert_into` para inserir o endpoint e criar o vínculo na pivô durante a mesma execução do builder:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>()
    .values(&tag)
    .execute(&client)
    .await?;
```

O mesmo comportamento é aplicado separadamente para cada payload de `insert_many`:

```rust
let mut tags = vec![
    Tag::new("rust".to_string()),
    Tag::new("database".to_string()),
];

for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>()
    .values(&tags)
    .execute(&client)
    .await?;
```

O Dinoco exclui `task_id` das colunas SQL da tabela `tag`. Depois de inserir cada `Tag`, usa o valor virtual para inserir `(task.id, tag.id)` na pivô. `None` insere o endpoint sem criar vínculo. Outra opção é inserir os dois endpoints primeiro e usar `connect` na API de update.

O ID virtual é write-only e continua `None` em reads e retornos, inclusive com `.returning::<S>()`.

Os mesmos payloads podem ser executados pelo contexto de uma transaction. O insert do endpoint e seu vínculo na pivô recebem commit ou rollback juntos:

```rust
dinoco::transaction(&client, |tx| async move {
    dinoco::insert_into::<Tag>().value(&tag).execute(tx).await?;
    dinoco::insert_many::<Tag>().values(&tags).execute(tx).await?;
    Ok(())
})
.await?;
```

Consulte [Many-to-many implícito](/v1.3.2/guide/relations#many-to-many-implícito) para entender os fields gerados, o comportamento da pivô, includes nos dois sentidos, counts, connect/disconnect e a migração de código existente.

## Identificadores gerados

UUID e Snowflake são criados pela lib antes de montar children. Autoincrement é recuperado do banco. Assim, relações funcionam mesmo quando o ID não era parâmetro de `new()`.

## Retorne uma projeção

```rust
let inserted = dinoco::insert_into::<User>()
    .values(&user)
    .returning::<UserSummary>()
    .execute(&client)
    .await?;

let inserted_many = dinoco::insert_many::<User>()
    .values(&users)
    .returning::<User>()
    .execute(&client)
    .await?;
```

Sem `returning`, o resultado é `()`. Com ele, insert retorna `S` e insert_many retorna `Vec<S>`.
