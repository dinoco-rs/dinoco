# 개요

**Dinoco**는 **PostgreSQL**, **MySQL**, **SQLite** 데이터베이스를 기본적으로 지원합니다.  
곧 새로운 데이터베이스로 지원을 확장하기 위해 노력하고 있습니다!

> ⚠️
> 현재 Dinoco는 **직접 연결 URL**만 지원합니다.

---

## 지원되는 버전

| 데이터베이스 | 최소 버전 |
| :------------- | :------------ |
| **SQLite**     | `>= 3.25`     |
| **PostgreSQL** | `>= 10`       |
| **MySQL**      | `>= 8.0`      |

---

## 연결 구성

구성은 스키마 파일의 `config` 블록 내에서 이루어집니다.

### SQLite

SQLite를 사용할 경우, 데이터베이스 파일은 `dinoco` 폴더 내에 자동으로 생성됩니다.

```dinoco
config {
  database = "sqlite"
  database_url = "file:./database.sqlite" # env("DATABASE_URL")
}
```

## PostgreSQL 또는 MySQL

외부 데이터베이스의 경우, 해당 연결 문자열을 입력하세요:

```dinoco
config {
  database = "postgresql" # 또는 "mysql"
  database_url = env("DATABASE_URL")
}
```

## 타입 매핑

| Dinoco       | Sqlite   | PostgreSQL       | MySQL            |
| :----------- | :------- | :--------------- | :--------------- |
| **String**   | TEXT     | TEXT             | VARCHAR(255)     |
| **Boolean**  | BOOLEAN  | BOOLEAN          | TINYINT(1)       |
| **Integer**  | INTEGER  | BIGINT           | BIGINT           |
| **Float**    | REAL     | DOUBLE PRECISION | DOUBLE PRECISION |
| **DateTime** | DATETIME | TIMESTAMP        | TIMESTAMP        |
| **Date**     | DATE     | DATE             | DATE             |
| **Json**     | BLOB     | JSONB            | JSON             |
