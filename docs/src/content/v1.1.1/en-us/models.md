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

## Primary-key requirement

Every model must declare exactly one primary key:

- use one field-level `@id` for a single-column key; or
- use one `@@ids([...])` declaration for a composite key.

A composite `@@ids` is one primary-key declaration, regardless of how many fields it contains. A model without either form fails schema compilation. Two `@id` fields, two `@@ids` declarations, or `@id` combined with `@@ids` also fail.

Primary-key fields must be required scalar or enum fields. Their order in `@@ids` is preserved in the database constraint and its automatic index.

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

Generated identifiers and their foreign keys preserve descriptive wrappers:

| Declaration | Generated Rust |
| --- | --- |
| `String @default(uuid())` | `dinoco::Uuid` |
| `Integer @default(snowflake())` | `dinoco::Snowflake` |
| `String` FK referencing a UUID | `dinoco::Uuid` |
| `Integer` FK referencing a Snowflake | `dinoco::Snowflake` |

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
- `@index` creates a non-unique index for the field.
- `@fulltext` creates a full-text capability and index for a String field.
- `@default(value)` supplies a literal, enum, or generated default.
- `@relation(...)` defines relation identity, key columns, and referential actions.

## Model attributes

Model attributes apply to ordered field groups:

| Attribute | Purpose |
| --- | --- |
| `@@ids([tenant_id, id])` | Composite primary key |
| `@@uniques([tenant_id, slug])` | Composite uniqueness |
| `@@indexes([tenant_id, created_at])` | Composite standard index |
| `@@fulltexts([title, body])` | Composite full-text index |
| `@@table_name("audit_users")` | Physical database table name |

`@@ids`, `@@uniques`, `@@indexes`, and `@@fulltexts` accept one non-empty array of existing scalar or enum fields without duplicates. Every field in `@@fulltexts` must be `String` or `String?`.

The formatter always places model attributes after all fields, separated by one blank line. This gives every model a stable field-first structure.

See [Indexes and constraints](/v1.1.1/guide/indexes) for single-field and composite indexes, uniqueness, and automatic primary- and foreign-key indexes.

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
    post_tag.rs
```

`dinoco/mod.rs` exports all models and a `connect()` function. Enums stay compact in `models/mod.rs` through `DinocoEnum`. One model per file keeps reviews focused.

An implicit many-to-many relation also generates a pivot entity. `Post` plus `Tag` produces `PostTag` in `post_tag.rs`; use it to query, connect, and disconnect links.
