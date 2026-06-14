# Обзор

**Dinoco** обеспечивает нативную поддержку баз данных **PostgreSQL**, **MySQL** и **SQLite**.  
Мы работаем над расширением этой поддержки для новых баз данных в ближайшее время!

> ⚠️
> В настоящее время Dinoco поддерживает исключительно **URL-адреса прямого подключения**.

---

## Поддерживаемые версии

| База данных | Минимальная версия |
| :------------- | :------------ |
| **SQLite**     | `>= 3.25`     |
| **PostgreSQL** | `>= 10`       |
| **MySQL**      | `>= 8.0`      |

---

## Настройка подключения

Настройка выполняется внутри блока `config` в вашем файле схемы.

### SQLite

При использовании SQLite файл базы данных будет автоматически сгенерирован в вашей папке `dinoco`.

```dinoco
config {
  database = "sqlite"
  database_url = "file:./database.sqlite" # env("DATABASE_URL")
}
```

## PostgreSQL или MySQL

Для внешних баз данных вставьте соответствующую строку подключения:

```dinoco
config {
  database = "postgresql" # или "mysql"
  database_url = env("DATABASE_URL")
}
```

## Сопоставление типов

| Dinoco       | Sqlite   | PostgreSQL       | MySQL            |
| :----------- | :------- | :--------------- | :--------------- |
| **String**   | TEXT     | TEXT             | VARCHAR(255)     |
| **Boolean**  | BOOLEAN  | BOOLEAN          | TINYINT(1)       |
| **Integer**  | INTEGER  | BIGINT           | BIGINT           |
| **Float**    | REAL     | DOUBLE PRECISION | DOUBLE PRECISION |
| **DateTime** | DATETIME | TIMESTAMP        | TIMESTAMP        |
| **Date**     | DATE     | DATE             | DATE             |
| **Json**     | BLOB     | JSONB            | JSON             |
