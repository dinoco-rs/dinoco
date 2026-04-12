# Derives

Visão geral dos derives e atributos centrais usados no Dinoco.

---

## O que são derives no Dinoco

Os derives do Dinoco ajudam a ligar suas structs Rust ao comportamento do ORM sem precisar escrever toda a implementação manualmente.

Os mais comuns no dia a dia são:

- `#[derive(Rowable)]`
- `#[derive(Extend)]`
- `#[insertable]`

## Derive Rowable

Use `Rowable` quando a struct representa diretamente um model que será lido e escrito pelo Dinoco.

```rust
#[derive(Debug, Clone, dinoco::Rowable)]
struct User {
    id: String,
    email: String,
    name: String,
}
```

Esse derive faz a serialização da row retornada pelo adapter para a struct Rust.

## Derive Extend

Use `Extend` quando você quiser uma projeção diferente do model base, normalmente para `select`, `include`, `count` ou payloads enriquecidos.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: String,
    name: String,
}
```

Nesse caso, a struct continua ligada ao model `User`, mas pode expor só os campos necessários para aquele fluxo.

## Atributo #[insertable]

Use `#[insertable]` junto com `Extend` quando `.values(...)` precisar aceitar relações novas ou conexões existentes dentro do próprio payload.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}
```

Com isso, o Dinoco insere o pai e depois processa os itens aninhados automaticamente.

## Exemplo com Rowable

```rust
#[derive(Debug, Clone, dinoco::Rowable)]
struct Team {
    id: String,
    name: String,
}

dinoco::insert_into::<Team>()
    .values(Team {
        id: "team-1".into(),
        name: "Platform".into(),
    })
    .execute(&client)
    .await?;
```

## Exemplo com Extend

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPostsCount {
    id: String,
    name: String,
    posts_count: usize,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPostsCount>()
    .count(|user| user.posts())
    .execute(&client)
    .await?;
```

## Exemplo com #[insertable]

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}

dinoco::insert_many::<Article>()
    .values(vec![
        ArticleWithLabels {
            id: "article-11".into(),
            title: "Connect Multiple".into(),
            labels: vec![
                ArticleConnection::Label("label-11".into()),
                ArticleConnection::Label("label-12".into()),
            ],
        },
        ArticleWithLabels {
            id: "article-12".into(),
            title: "Connect Batch".into(),
            labels: vec![ArticleConnection::Label("label-10".into())],
        },
    ])
    .execute(&client)
    .await?;
```

## Quando usar cada um

- Use `Rowable` para o model principal gerado ou structs compatíveis com leitura direta de linha.
- Use `Extend` para projeções, contagens, includes e payloads especializados.
- Use `#[insertable]` quando o `Extend` também for servir como payload de escrita com aninhamento.

## Próximos passos

- [**Traits**](/v0.0.2/core/traits): veja as traits implementadas pelo model.
- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into): inserção única com payloads ricos.
- [**`insert_many::&lt;M&gt;()`**](/v0.0.2/orm/insert-many): inserção em lote com relações e conexões aninhadas.
