# Models and fields

A model describes one database table and one generated Rust struct. Model and field names remain visible throughout the API, which makes the schema the easiest place to learn a Dinoco project.

## Declare a model

```dinoco
model User {
    id         String   @id @default(uuid())
    email      String   @unique
    display    String?
    active     Boolean  @default(true)
    score      Float
    created_at DateTime @default(now())
}
```

This produces an `Entity` named `User` and a SQL table named `user`. The generated fields are public and use Rust types corresponding to the schema.

## Scalar types

| Dinoco | Rust | SQLite | PostgreSQL | MySQL |
| --- | --- | --- | --- | --- |
| `String` | `String` | `TEXT` | `TEXT` | `VARCHAR(255)` |
| `Boolean` | `bool` | `BOOLEAN` | `BOOLEAN` | `TINYINT(1)` |
| `Integer` | `i64` | `INTEGER` | `BIGINT` | `BIGINT` |
| `Float` | `f64` | `REAL` | `DOUBLE PRECISION` | `DOUBLE PRECISION` |
| `DateTime` | `DateTime<Utc>` | `DATETIME` | `TIMESTAMP` | `TIMESTAMP` |
| `Date` | `NaiveDate` | `DATE` | `DATE` | `DATE` |
| `Json` | `serde_json::Value` | `BLOB` | `JSONB` | `JSON` |

Date/time and JSON types are re-exported through `dinoco_engine` in generated code, so adapter decoding stays consistent.

## Optional and list fields

Append `?` to make a column or one-side relation optional:

```dinoco
display_name String?
profile      Profile?
```

The generated Rust fields become `Option<String>` and `Option<Profile>`. Append `[]` for a relation list:

```dinoco
posts Post[]
```

This becomes `Vec<Post>`. Lists represent relations, not SQL array columns.

## Field attributes

- `@id` marks a primary-key field.
- `@unique` creates a uniqueness constraint.
- `@default(value)` supplies a literal, enum, or generated default.
- `@relation(...)` defines relation identity, key columns, and referential actions.

At model level, `@@table_name("audit_users")` overrides the generated table name and `@@ids([tenant_id, id])` declares a composite identifier.

## The generated new function

Every generated entity implements `pub fn new(...) -> Self`. Its parameters are only scalar fields that are required and have no default or automatic generator. Optional fields, relation fields, and defaulted fields receive their default Rust value.

For this schema:

```dinoco
model User {
    id      String  @id @default(uuid())
    email   String
    name    String
    enabled Boolean @default(true)
    bio     String?
    posts   Post[]
}
```

construction is concise:

```rust
let user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
```

`id` is generated, `enabled` receives its declared default, `bio` starts as `None`, and `posts` starts as an empty vector. You can modify any public field before inserting.

## Generated files

`dinoco models generate` and `dinoco migrate generate` create a predictable module tree:

```text
dinoco/
  mod.rs
  models/
    mod.rs
    user.rs
    post.rs
```

`dinoco/mod.rs` exports all models and a `connect()` function. `models/mod.rs` defines enums and exports each model module. One model per file keeps reviews focused and avoids a single generated file growing without bound.
