# files d'attente

Le système de files d'attente de Dinoco permet de publier des événements asynchrones à partir de `insert`, `find` et `update`, en ne stockant dans `Redis` que les données nécessaires pour réhydrater l'enregistrement lorsque le worker s'exécutera.

## Ce que vous pouvez faire

- Permettre l'enchaînement de .enqueue(...), .enqueue_in(...) et .enqueue_at(...) sur toutes les méthodes du client, à l'exception des opérations de suppression.
- Traiter les jobs avec `workers().on::&lt;Model&gt;(... )`.
- Charger des projections avec relations dans le worker en utilisant `workers().on_with_relation::&lt;Model, Projection&gt;(... )`.
- Recharger les données les plus récentes avant l'exécution du handler.
- Ignorer automatiquement les jobs dont l'enregistrement n'existe plus.

## Comment ça marche

Lorsque vous utilisez `enqueue`, Dinoco :

1. Exécute l'opération principale normalement.
2. Enregistre dans la file d'attente uniquement les critères de recherche de l'enregistrement.
3. Dans le worker, récupère les données les plus récentes dans la base de données.
4. Ce n'est qu'alors qu'il exécute le handler.

Si l'enregistrement n'existe plus au moment de la recherche, le handler ne s'exécute pas et le job est supprimé de la file d'attente.

## Méthodes d'enfilement

- `.enqueue(événement)`: planifie pour une exécution immédiate.
- `.enqueue_in(événement, delay_ms)`: planifie avec un délai en millisecondes.
- `.enqueue_at(événement, date_utc)`: planifie pour une date spécifique en `chrono::DateTime&lt;Utc&gt;`.

```rust
use database::*;

insert_into::<User>()
    .values(User { id: 1, name: "Matheus".to_string() })
    .enqueue("user.created")
    .execute(&client)
    .await?;

insert_into::<User>()
    .values(User { id: 2, name: "Ana".to_string() })
    .enqueue_in("user.reminder", 30_000)
    .execute(&client)
    .await?;

insert_into::<User>()
    .values(User { id: 3, name: "Lia".to_string() })
    .enqueue_at("user.scheduled", dinoco::Utc::now() + chrono::Duration::hours(1))
    .execute(&client)
    .await?;
```

## Worker

Utilisez `workers()` pour enregistrer des handlers asynchrones. Le contexte du worker expose :

- `data`: l'enregistrement réhydraté.
- `client`: une copie du `DinocoClient`.
- `success()` ou `remove()`: supprime le job de la file d'attente.
- `fail()`: replanifie avec la tentative par défaut.
- `retry_in()` et `retry_at()`: même idée que `enqueue_in` et `enqueue_at`.
- `.run().await?`: démarre le worker en arrière-plan et retourne immédiatement un `JoinHandle`.

Lors de l'appel à `run`, Dinoco crée un nouveau `DinocoClient` exclusif pour les workers, avec son propre pool de connexions. Ainsi, la boucle de la file d'attente ne réutilise pas le même pool que l'application principale.

```rust
use database::*;

let _worker = workers()
    .on::<User, _, _>("user.created", |job| async move {
        println!("Processando {}", job.data.name); // Traitement de {}
        job.success()
    })
    .run()
    .await?;
```

Si vous avez besoin d'une projection avec des relations, utilisez `on_with_relation`. Dinoco recharge l'enregistrement et applique l'arbre d'inclusions avant l'exécution du handler.

```rust
use database::*;

let _worker = workers()
    .on_with_relation::<User, UserWithPosts, _, _, _, _>(
        "user.created",
        |user| user.posts().select::<PostListItem>(),
        |job| async move {
            println!("{} tem {} posts", job.data.name, job.data.posts.len()); // {} a {} posts
            job.success()
        },
    )
    .run()
    .await?;
```

Vous pouvez également travailler avec des listes :

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-updated", |job| async move {
        for user in job.data {
            println!("{}", user.name);
        }

        job.success()
    })
    .run()
    .await?;
```

## Configuration de Redis

Les files d'attente utilisent le `Redis` configuré dans `DinocoClientConfig::with_redis(...)`. Pour `enqueue_in` et `enqueue_at`, il est conseillé d'activer la persistance, car les jobs planifiés résident dans Redis jusqu'à l'heure d'exécution.

Recommandation : utilisez **AOF + RDB ensemble**.

```conf
# Ativa AOF
appendonly yes

# Frequência de fsync
appendfsync everysec

# Arquivo AOF
appendfilename "appendonly.aof"

# --- SNAPSHOT (RDB) ---
save 900 1
save 300 10
save 60 10000

# --- SEGURANÇA ---
dir /data

# --- PERFORMANCE ---
no-appendfsync-on-rewrite yes
```

## Observations

- `delete` et `delete_many` ne prennent pas en charge `enqueue`.
- Le worker est entièrement asynchrone et utilise `tokio` en coulisses.
- `run()` ne bloque pas le reste de l'application : il lance la boucle en arrière-plan.
- `Option&lt;User&gt;` est accepté lors de l'enregistrement du worker, mais le handler ne s'exécute que lorsqu'il y a un enregistrement à réhydrater.

## Prochaines étapes

- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into)
- [**`find_many::&lt;M&gt;()`**](/v0.0.2/orm/find-many)
- [**`update::&lt;M&gt;()`**](/v0.0.2/orm/update)
