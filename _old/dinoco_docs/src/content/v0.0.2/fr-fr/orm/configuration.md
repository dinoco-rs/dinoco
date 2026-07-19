# Configuration

Le bloc `config {}` définit comment Dinoco se connectera à la base de données et quelles fonctionnalités supplémentaires sont disponibles dans le projet.

## Ce qui est inclus dans `config {}`

- `database`: définit l'adaptateur principal, tel que `sqlite`, `postgresql` ou `mysql`.
- `database_url`: indique l'URL de connexion principale.
- `read_replicas`: facultatif, ajoute des répliques de lecture.
- `redis`: facultatif, active le cache et les files d'attente basés sur Redis.

## Exemple de base

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}
```

## Exemple avec Redis

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")

    redis = {
        url = env("REDIS_URL")
    }
}
```

## Exemple avec répliques

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")

    read_replicas = [
        env("DATABASE_READ_URL_1"),
        env("DATABASE_READ_URL_2")
    ]
}
```

## Ce qui change dans l'API générée

- Avec `redis`, le projet expose des fonctions auxiliaires telles que `client.cache()`, `.cache(...)` et des files d'attente avec `workers()`.
- Avec `read_replicas`, les lectures peuvent utiliser une réplique tandis que les écritures continuent sur la base de données principale.
- Sans `redis`, les fonctionnalités de cache et de file d'attente ne sont pas entièrement disponibles.

## Quand modifier ce bloc

- Lors du changement de la base de données principale du projet.
- Lors de l'ajout de Redis pour le cache ou les files d'attente.
- Lors de la configuration d'une réplique de lecture.
- Lors de l'ajustement des variables d'environnement de connexion.

## Prochaines étapes

- [**Vue d'ensemble**](/v0.0.2/orm/supported-databases)
- [**Modèles**](/v0.0.2/orm/models)
- [**Relations**](/v0.0.2/orm/relations)
