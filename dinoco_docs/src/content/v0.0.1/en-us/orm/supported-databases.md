# Overview

**Dinoco** natively supports **PostgreSQL**, **MySQL**, and **SQLite** databases.
We are working to expand this support to new databases soon!

> ⚠️
> Currently, Dinoco exclusively supports **direct connection URLs**.

---

## Supported Versions

| Database       | Minimum Version |
| :------------- | :------------ |
| **SQLite**     | `>= 3.25`     |
| **PostgreSQL** | `>= 10`       |
| **MySQL**      | `>= 8.0`      |

---

## Connection Configuration

Configuration is done within the `config` block in your schema file.

### SQLite

When using SQLite, the database file will be automatically generated inside your `dinoco` folder.

```dinoco
config {
  database = "sqlite"
  database_url = "file:./database.sqlite" # env("DATABASE_URL")
}
```

## PostgreSQL or MySQL

For external databases, enter the corresponding connection string:

```dinoco
config {
  database = "postgresql" # or "mysql"
  database_url = env("DATABASE_URL")
}
```

## Type Mapping

| Dinoco       | Sqlite   | PostgreSQL       | MySQL            |
| :----------- | :------- | :--------------- | :--------------- |
| **String**   | TEXT     | TEXT             | VARCHAR(255)     |
| **Boolean**  | BOOLEAN  | BOOLEAN          | TINYINT(1)       |
| **Integer**  | INTEGER  | BIGINT           | BIGINT           |
| **Float**    | REAL     | DOUBLE PRECISION | DOUBLE PRECISION |
| **DateTime** | DATETIME | TIMESTAMP        | TIMESTAMP        |
| **Date**     | DATE     | DATE             | DATE             |
| **Json**     | BLOB     | JSONB            | JSON             |
