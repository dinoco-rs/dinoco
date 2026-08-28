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

See [Indexes and constraints](/v1.2.6/guide/indexes) for single-field and composite indexes, uniqueness, and automatic primary- and foreign-key indexes.

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

## Implicit many-to-many generated fields

An implicit many-to-many keeps its SQL pivot internal and does not generate a public pivot entity. Instead, `Post` receives a write-only `tag_id: Option<TagId>` and `Tag` receives `post_id: Option<PostId>`.

Assigning one of these virtual fields before `insert_into` or on each `insert_many` item creates the corresponding pivot link after the endpoint is inserted:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.post_id = Some(post.id.clone());

dinoco::insert_into::<Tag>()
    .values(&tag)
    .execute(&client)
    .await?;
```

The virtual field is not a column of `tag` and always comes back as `None`. On existing endpoints, use the same generated field's `connect` and `disconnect` update operations. See [Implicit many-to-many](/v1.2.6/guide/relations#implicit-many-to-many) for the complete contract.

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

`dinoco/mod.rs` exports all models and a `connect()` function. Enums stay compact in `models/mod.rs` through `DinocoEnum`. One model per file keeps reviews focused.

The generated root module starts with `#![allow(dead_code)]`, so helpers that are not used by every application do not create warnings. Additional derives and their imports belong in `config.custom_derives`; see [Schema organization](/v1.2.6/guide/schema-organization#custom-derives).

An implicit many-to-many relation keeps its pivot internal. `Post` receives a virtual `tag_id` and `Tag` receives `post_id`; use those write-only fields to connect and disconnect links.
