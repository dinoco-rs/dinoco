# Aperçu

**Dinoco** prend en charge nativement les bases de données **PostgreSQL**, **MySQL** et **SQLite**.  
Nous travaillons à étendre ce support à de nouvelles bases de données prochainement !

> ⚠️
> Actuellement, Dinoco prend en charge exclusivement les **URL de connexion directe**.

---

## Versions prises en charge

| Base de données | Version minimale |
| :-------------- | :--------------- |
| **SQLite**      | `>= 3.25`        |
| **PostgreSQL**  | `>= 10`          |
| **MySQL**       | `>= 8.0`         |

---

## Configuration de la connexion

La configuration est effectuée dans le bloc `config` de votre fichier de schéma.

### SQLite

Lors de l'utilisation de SQLite, le fichier de la base de données sera généré automatiquement dans votre dossier `dinoco`.

```dinoco
config {
  database = "sqlite"
  database_url = "file:./database.sqlite" # env("DATABASE_URL")
}
```

## PostgreSQL ou MySQL

Pour les bases de données externes, insérez la chaîne de connexion correspondante :

```dinoco
config {
  database = "postgresql" # ou "mysql"
  database_url = env("DATABASE_URL")
}
```

## Mappage des types

| Dinoco       | Sqlite   | PostgreSQL       | MySQL            |
| :----------- | :------- | :--------------- | :--------------- |
| **String**   | TEXT     | TEXT             | VARCHAR(255)     |
| **Boolean**  | BOOLEAN  | BOOLEAN          | TINYINT(1)       |
| **Integer**  | INTEGER  | BIGINT           | BIGINT           |
| **Float**    | REAL     | DOUBLE PRECISION | DOUBLE PRECISION |
| **DateTime** | DATETIME | TIMESTAMP        | TIMESTAMP        |
| **Date**     | DATE     | DATE             | DATE             |
| **Json**     | BLOB     | JSONB            | JSON             |
