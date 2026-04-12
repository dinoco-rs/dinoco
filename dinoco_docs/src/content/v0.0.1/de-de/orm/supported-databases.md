# Übersicht

**Dinoco** bietet native Unterstützung für die Datenbanken **PostgreSQL**, **MySQL** und **SQLite**.
Wir arbeiten daran, diese Unterstützung bald auf neue Datenbanken auszuweiten!

> ⚠️
> Derzeit unterstützt Dinoco ausschließlich **direkte Verbindungs-URLs**.

---

## Unterstützte Versionen

| Datenbank      | Mindestversion |
| :------------- | :------------ |
| **SQLite**     | `>= 3.25`     |
| **PostgreSQL** | `>= 10`       |
| **MySQL**      | `>= 8.0`      |

---

## Verbindungskonfiguration

Die Konfiguration erfolgt innerhalb des `config`-Blocks in Ihrer Schema-Datei.

### SQLite

Bei der Verwendung von SQLite wird die Datenbankdatei automatisch in Ihrem `dinoco`-Ordner generiert.

```dinoco
config {
  database = "sqlite"
  database_url = "file:./database.sqlite" # env("DATABASE_URL")
}
```

## PostgreSQL oder MySQL

Für externe Datenbanken geben Sie die entsprechende Verbindungszeichenfolge ein:

```dinoco
config {
  database = "postgresql" # oder "mysql"
  database_url = env("DATABASE_URL")
}
```

## Typzuordnung

| Dinoco       | Sqlite   | PostgreSQL       | MySQL            |
| :----------- | :------- | :--------------- | :--------------- |
| **String**   | TEXT     | TEXT             | VARCHAR(255)     |
| **Boolean**  | BOOLEAN  | BOOLEAN          | TINYINT(1)       |
| **Integer**  | INTEGER  | BIGINT           | BIGINT           |
| **Float**    | REAL     | DOUBLE PRECISION | DOUBLE PRECISION |
| **DateTime** | DATETIME | TIMESTAMP        | TIMESTAMP        |
| **Date**     | DATE     | DATE             | DATE             |
| **Json**     | BLOB     | JSONB            | JSON             |
