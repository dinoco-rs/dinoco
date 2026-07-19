# find_many

Usato per recuperare un elenco di record.

---

## Cosa puoi fare

- `.select::&lt;T&gt;()`: scambia la proiezione predefinita del modello con una proiezione personalizzata.
- `.cond(...)`: aggiunge condizioni di filtro alla query.
- `.take(...)`: limita la quantità di record restituiti.
- `.skip(...)`: salta una quantità di record prima di restituire il risultato.
- `.order_by(...)`: definisce l'ordinamento della query.
- `.includes(...)`: carica le relazioni insieme ai record principali.
- `.count(...)`: calcola i contatori delle relazioni e popola campi come `posts_count`.
- `.cache(...)`: interroga prima in Redis usando la chiave fornita; in caso di cache miss esegue la query, salva e restituisce il risultato. In caso di cache hit, il query logger registra `CACHE HIT key=...`.
- `.cache_with_expiration(...)`: stesso comportamento della cache standard, ma salva con TTL in secondi.
- `.read_in_primary()`: forza la lettura nel database principale, senza usare la replica.
- `.execute(&client)`: esegue la query nel database.

## Ritorno

Senza `select::&lt;T&gt;()`, il ritorno è:

```rust
DinocoResult<Vec<M>>
```

Con `select::&lt;T&gt;()`, il ritorno diventa:

```rust
DinocoResult<Vec<T>>
```

## Esempio base

```rust
let users = dinoco::find_many::<User>()
    .execute(&client)
    .await?;
```

## Esempio con filtro

```rust
let users = dinoco::find_many::<User>()
    .cond(|w| w.email.eq("ana@acme.com"))
    .execute(&client)
    .await?;
```

## Esempio con paginazione e ordinamento

```rust
let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .skip(20)
    .take(10)
    .execute(&client)
    .await?;
```

## Esempio di select personalizzato

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserListItem {
    id: i64,
    name: String,
}

let users = dinoco::find_many::<User>()
    .select::<UserListItem>()
    .execute(&client)
    .await?;
```

## Esempio con include semplice

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPosts>()
    .includes(|i| i.posts())
    .execute(&client)
    .await?;
```

## Esempio con include filtrato

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPublishedPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPublishedPosts>()
    .includes(|i| i.posts().cond(|w| w.published.eq(true)))
    .execute(&client)
    .await?;
```

## Esempio con include annidato

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Comment)]
struct CommentListItem {
    id: i64,
    text: String,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Post)]
struct PostWithComments {
    id: i64,
    title: String,
    comments: Vec<CommentListItem>,
    comments_count: usize,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<PostWithComments>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPosts>()
    .includes(|i| {
        i.posts()
            .includes(|post| post.comments().take(3))
            .count(|post| post.comments())
    })
    .execute(&client)
    .await?;
```

## Esempio con conteggio di relazione

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPostsCount {
    id: i64,
    name: String,
    posts_count: usize,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPostsCount>()
    .count(|i| i.posts())
    .execute(&client)
    .await?;
```

## Esempio di lettura nel database principale

```rust
let users = dinoco::find_many::<User>()
    .read_in_primary()
    .take(5)
    .execute(&client)
    .await?;
```

## Esempio con worker

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-read", |job| async move {
        println!("Lotto letto con {} utenti", job.data.len());
        job.success();
    })
    .run()
    .await?;

let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .take(20)
    .enqueue("user.batch-read")
    .execute(&client)
    .await?;
```

Vedi di più sui worker in [**`queues`**](/v0.0.2/orm/queues).

## Esempio con cache

Questo metodo viene generato solo quando il `config {}` dello schema ha `redis`.

```rust
use database::*;

let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .cache("users:list")
    .execute(&client)
    .await?;
```

## Esempio con cache e scadenza

```rust
use database::*;

let users = dinoco::find_many::<User>()
    .take(20)
    .cache_with_expiration("users:top-20", 60)
    .execute(&client)
    .await?;
```

## Prossimi passi

- [**`find_first::&lt;M&gt;()`**](/v0.0.2/orm/find-first): versione per cercare al massimo un record.
- [**`count::&lt;M&gt;()`**](/v0.0.2/orm/count): conteggio dei record con filtro.
