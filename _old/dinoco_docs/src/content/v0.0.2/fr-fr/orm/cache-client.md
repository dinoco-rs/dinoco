# cache

`client.cache()` expose un accès direct à Redis configuré dans le `DinocoClient`, sans dépendre des helpers de cache couplés dans `find_first` et `find_many`.

## Ce que vous pouvez faire

- Lire une clé avec `.get::&lt;T&gt;(...)`
- Enregistrer une clé avec `.set(...)`
- Enregistrer avec expiration en utilisant `.set_with_ttl(...)`
- Supprimer une clé avec `.delete(...)`

## Quand l'utiliser

Utilisez `client.cache()` lorsque vous souhaitez :

- monter un cache manuel
- invalider des clés après des opérations d'écriture
- partager des charges utiles entre plusieurs requêtes
- stocker des structures prêtes pour une lecture rapide

## Comment ça marche

La méthode utilise Redis configuré dans `DinocoClientConfig::with_redis(...)`.

Si le client n'a pas Redis configuré, l'opération renvoie une erreur.

## Méthodes disponibles

- `.get::&lt;T&gt;(key)`: recherche et désérialise la valeur comme `T`
- `.set(key, &value)`: sérialise et enregistre sans TTL
- `.set_with_ttl(key, &value, ttl_seconds)`: sérialise et enregistre avec expiration en secondes
- `.delete(key)`: supprime la clé

## Exemple de base

```rust
use database::*;

let cache = client.cache();

cache.set("users:count", &42_i64).await?;

let count = cache.get::<i64>("users:count").await?;

println!("{count:?}");
```

## Exemple avec liste typée

```rust
use database::*;

let users = vec![
    User { id: 1, name: "Matheus".to_string() },
    User { id: 2, name: "Ana".to_string() },
];

client.cache().set("users:list", &users).await?;

let cached = client.cache().get::<Vec<User>>("users:list").await?;
```

## Exemple avec TTL

```rust
use database::*;

client.cache().set_with_ttl("users:top-10", &vec![1, 2, 3], 60).await?;
```

## Exemple d'invalidation

```rust
use database::*;

dinoco::update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .values(User { id: 1, name: "Novo nome".to_string() })
    .execute(&client)
    .await?;

client.cache().delete("users:1").await?;
client.cache().delete("users:list").await?;
```

## Types supportés

Les valeurs sont sérialisées en JSON, donc le type doit être compatible avec `serde`.

Exemples courants :

- `Vec&lt;User&gt;`
- `Option&lt;User&gt;`
- `String`
- `bool`
- `i64`
- structs sérialisables

## Remarques

- `client.cache()` est un cache manuel ; il n'exécute pas de requête en base de données.
- Pour un cache intégré aux requêtes, utilisez `find_first().cache(...)` et `find_many().cache(...)`.
- Vous pouvez appeler `client.cache()` autant de fois que vous le souhaitez ; il ne crée qu'un wrapper léger.

## Prochaines étapes

- [**`find_first::&lt;M&gt;()`**](/v0.0.2/orm/find-first)
- [**`find_many::&lt;M&gt;()`**](/v0.0.2/orm/find-many)
- [**`queues`**](/v0.0.2/orm/queues)
