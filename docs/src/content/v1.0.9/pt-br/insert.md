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

One-to-many, many-to-one e one-to-one usam os metadados da relation. Em many-to-many, insira os endpoints e gerencie a entity pivot com `connect` e `disconnect`.

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
