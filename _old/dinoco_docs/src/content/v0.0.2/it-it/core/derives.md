# Derive

Panoramica dei derive e degli attributi centrali utilizzati in Dinoco.

---

## Cosa sono i derive in Dinoco

I derive di Dinoco aiutano a collegare le tue struct Rust al comportamento dell'ORM senza dover scrivere manualmente l'intera implementazione.

I più comuni nell'uso quotidiano sono:

- `#[derive(Rowable)]`
- `#[derive(Extend)]`
- `#[insertable]`

## Derive Rowable

Usa `Rowable` quando la struct rappresenta direttamente un modello che verrà letto e scritto da Dinoco.

```rust
#[derive(Debug, Clone, dinoco::Rowable)]
struct User {
    id: String,
    email: String,
    name: String,
}
```

Questo derive esegue la serializzazione della riga restituita dall'adapter nella struct Rust.

## Derive Extend

Usa `Extend` quando desideri una proiezione diversa dal modello base, normalmente per `select`, `include`, `count` o payload arricchiti.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: String,
    name: String,
}
```

In questo caso, la struct rimane collegata al modello `User`, ma può esporre solo i campi necessari per quel flusso.

## Attributo #[insertable]

Usa `#[insertable]` insieme a `Extend` quando `.values(...)` deve accettare nuove relazioni o connessioni esistenti all'interno del payload stesso.

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

Con questo, Dinoco inserisce il padre e poi elabora automaticamente gli elementi annidati.

## Esempio con Rowable

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

## Esempio con Extend

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

## Esempio con #[insertable]

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

## Quando usare ciascuno

- Usa `Rowable` per il modello principale generato o per le struct compatibili con la lettura diretta di riga.
- Usa `Extend` per proiezioni, conteggi, inclusioni e payload specializzati.
- Usa `#[insertable]` quando `Extend` servirà anche come payload di scrittura con annidamento.

## Prossimi passi

- [**Traits**](/v0.0.2/core/traits): vedi i trait implementati dal modello.
- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into): inserimento singolo con payload ricchi.
- [**`insert_many::&lt;M&gt;()`**](/v0.0.2/orm/insert-many): inserimento in batch con relazioni e connessioni annidate.
