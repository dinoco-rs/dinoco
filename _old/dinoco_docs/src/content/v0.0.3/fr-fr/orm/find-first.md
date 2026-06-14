# find_first

Utilisé pour rechercher au maximum un enregistrement.

---

## Ce que vous pouvez faire

- `.select::&lt;T&gt;()`: échange la projection par défaut contre une projection personnalisée.
- `.cond(...)`: ajoute des filtres à la recherche.
- `.take(...)`: limite la quantité maximale d'enregistrements considérés.
- `.skip(...)`: saute des enregistrements avant de sélectionner le premier résultat.
- `.order_by(...)`: définit quel enregistrement doit être considéré en premier.
- `.includes(...)`: charge les relations avec l'élément principal.
- `.count(...)`: calcule les compteurs de relations dans la projection.
- `.cache(...)`: tente de rechercher d'abord dans Redis et ne consulte la base de données que si la clé n'existe pas. En cas de succès du cache, le journal de requête enregistre `CACHE HIT key=...`.
- `.cache_with_expiration(...)`: effectue le même flux, mais enregistre avec un TTL en secondes.
- `.read_in_primary()`: force la lecture sur la base de données principale.
- `.execute(&client)`: exécute la requête et retourne au maximum un élément.

## Retour

Sans `select::&lt;T&gt;()`, le retour est :

```rust
DinocoResult<Option<M>>
```

Avec `select::&lt;T&gt;()`, le retour devient :

```rust
DinocoResult<Option<T>>
```

## Exemple de base

```rust
let user = dinoco::find_first::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

## Exemple avec select

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: i64,
    name: String,
}

let user = dinoco::find_first::<User>()
    .select::<UserSummary>()
    .cond(|w| w.id.eq(1_i64))
    .execute(&client)
    .await?;
```

## Exemple avec relation

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let user = dinoco::find_first::<User>()
    .select::<UserWithPosts>()
    .cond(|x| x.id.eq(1_i64))
    .includes(|x| x.posts())
    .execute(&client)
    .await?;
```

## Exemple avec tri

```rust
let latest_user = dinoco::find_first::<User>()
    .order_by(|x| x.id.desc())
    .execute(&client)
    .await?;
```

## Exemple avec worker

```rust
use database::*;

let _worker = workers()
    .on::<User, _, _>("user.first-read", |job| async move {
        println!("Premier utilisateur lu : {}", job.data.name);
        job.success();
    })
    .run()
    .await?;

let user = dinoco::find_first::<User>()
    .order_by(|x| x.id.desc())
    .enqueue("user.first-read")
    .execute(&client)
    .await?;
```

Voir plus sur les workers dans [**`queues`**](/v0.0.2/orm/queues).

## Exemple avec cache

Cette méthode n'existe que si le schéma a `redis` configuré.

```rust
use database::*;

let user = dinoco::find_first::<User>()
    .cond(|x| x.id.eq(1_i64))
    .cache("users:1")
    .execute(&client)
    .await?;
```

## Exemple avec cache

Cette méthode n'existe que si le schéma a `redis` configuré.

```rust
use database::*;

let user = dinoco::find_first::<User>()
    .cond(|x| x.id.eq(1_i64))
    .cache_with_expiration("users:1")
    .execute(&client)
    .await?;
```

## Prochaines étapes

- [**`find_many::&lt;M&gt;()`**](/v0.0.2/orm/find-many): recherche plusieurs enregistrements.
- [**`count::&lt;M&gt;()`**](/v0.0.2/orm/count): comptage d'enregistrements.
