# find_many

Utilisé pour récupérer une liste d'enregistrements.

---

## Ce que vous pouvez faire

- `.select::&lt;T&gt;()`: échange la projection par défaut du modèle contre une projection personnalisée.
- `.cond(...)`: ajoute des conditions de filtre à la requête.
- `.take(...)`: limite le nombre d'enregistrements retournés.
- `.skip(...)`: saute un nombre d'enregistrements avant de retourner le résultat.
- `.order_by(...)`: définit l'ordre de la requête.
- `.includes(...)`: charge les relations avec les enregistrements principaux.
- `.count(...)`: calcule les compteurs de relations et remplit des champs comme `posts_count`.
- `.cache(...)`: interroge d'abord Redis en utilisant la clé fournie ; en cas de cache miss, exécute la requête, sauvegarde et retourne le résultat. En cas de cache hit, le logger de requête enregistre `CACHE HIT key=...`.
- `.cache_with_expiration(...)`: même comportement que le cache standard, mais enregistre avec un TTL en secondes.
- `.read_in_primary()`: force la lecture sur la base de données principale, sans utiliser de réplique.
- `.execute(&client)`: exécute la requête sur la base de données.

## Retour

Sans `select::&lt;T&gt;()`, le retour est :

```rust
DinocoResult<Vec<M>>
```

Avec `select::&lt;T&gt;()`, le retour devient :

```rust
DinocoResult<Vec<T>>
```

## Exemple de base

```rust
let users = dinoco::find_many::<User>()
    .execute(&client)
    .await?;
```

## Exemple avec filtre

```rust
let users = dinoco::find_many::<User>()
    .cond(|w| w.email.eq("ana@acme.com"))
    .execute(&client)
    .await?;
```

## Exemple avec pagination et tri

```rust
let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .skip(20)
    .take(10)
    .execute(&client)
    .await?;
```

## Exemple de sélection personnalisée

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

## Exemple avec inclusion simple

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

## Exemple avec inclusion filtrée

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

## Exemple avec inclusion imbriquée

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

## Exemple avec le compte de relation

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

## Exemple de lecture sur la base de données principale

```rust
let users = dinoco::find_many::<User>()
    .read_in_primary()
    .take(5)
    .execute(&client)
    .await?;
```

## Exemple avec worker

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-read", |job| async move {
        // Lot lu avec {} utilisateurs
        println!("Lot lu avec {} utilisateurs", job.data.len());
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

Voir plus sur les workers dans [**`queues`**](/v0.0.2/orm/queues).

## Exemple avec cache

Cette méthode n'est générée que si le `config {}` du schéma contient `redis`.

```rust
use database::*;

let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .cache("users:list")
    .execute(&client)
    .await?;
```

## Exemple avec cache et expiration

```rust
use database::*;

let users = dinoco::find_many::<User>()
    .take(20)
    .cache_with_expiration("users:top-20", 60)
    .execute(&client)
    .await?;
```

## Prochaines étapes

- [**`find_first::&lt;M&gt;()`**](/v0.0.2/orm/find-first): version pour rechercher au maximum un enregistrement.
- [**`count::&lt;M&gt;()`**](/v0.0.2/orm/count): nombre d'enregistrements avec filtre.
