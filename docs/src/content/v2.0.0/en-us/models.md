# Models and fields

A `model` describes exactly two things at once: one database table, and one generated Rust struct. There's no separate "entity" layer to keep in sync — the field names you write in the schema are the same names you'll see in Rust, in SQL, and in every error message, which is what makes the schema itself the fastest way to understand a Dinoco project.

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

This produces a generated `Entity` named `User` and a SQL table named `user` (model names are `PascalCase`; table names are generated as `snake_case`). Every generated field is public, using the Rust type from the [scalar types](#scalar-types) table below.

## Primary-key requirement

Every model needs **exactly one** primary key, declared one of two ways:

- a single field-level `@id`, for a one-column key; or
- one `@@ids([...])` model attribute, for a composite key.

> [!WARNING]
> A composite `@@ids` counts as *one* primary-key declaration no matter how many fields it lists. A model with no primary key fails to compile, and so does one with two `@id` fields, two `@@ids` attributes, or `@id` combined with `@@ids` — pick exactly one form.

Primary-key fields must be required (non-optional) scalars or enums. For a composite key, field order in `@@ids([...])` is preserved in both the database constraint and its automatic index — put the column you'll filter by most often first.

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

That's the complete list — there's no `Bytes`, `Decimal`, or custom scalar escape hatch. Two of these get a more descriptive Rust type when they're identifiers, so a foreign key can't accidentally be compared against the wrong kind of ID:

| Declaration | Generated Rust |
| --- | --- |
| `String @default(uuid())` | `dinoco::Uuid` |
| `Integer @default(snowflake())` | `dinoco::Snowflake` |
| `String` foreign key referencing a UUID `@id` | `dinoco::Uuid` |
| `Integer` foreign key referencing a Snowflake `@id` | `dinoco::Snowflake` |

`DateTime`/`Date`/`Json`'s Rust types are re-exported through `dinoco_engine`, so every adapter decodes them the same way — you never need to add `chrono` or `serde_json` yourself just to name a field's type.

## Optional and list fields

Append `?` to make a scalar column, or a to-one relation, optional:

```dinoco
display_name String?
profile      Profile?
```

These generate as `Option<String>` and `Option<Profile>`. Append `[]` for a to-many relation:

```dinoco
posts Post[]
```

This generates as `Vec<Post>`. Note that `[]` in Dinoco always means "a relation to many rows" — there's no SQL array column type here, so you can't write `String[]` to store a list of strings on one row.

## Field attributes

- `@id` — marks the field as the (single-column) primary key.
- `@unique` — adds a uniqueness constraint.
- `@index` — adds a non-unique index.
- `@fulltext` — adds full-text search capability and an index, on a `String` field.
- `@default(value)` — a literal, an enum variant, or a generator call (`uuid()`, `snowflake()`, `autoincrement()`, `now()`).
- `@relation(...)` — declares relation identity, foreign key columns, and referential actions. See [Relations](/en-us/docs/orm/guide/relations).

## Model attributes

Where field attributes describe one column, model attributes describe an ordered group of them:

| Attribute | Purpose |
| --- | --- |
| `@@ids([tenant_id, id])` | Composite primary key |
| `@@uniques([tenant_id, slug])` | Composite uniqueness constraint |
| `@@indexes([tenant_id, created_at])` | Composite standard index |
| `@@fulltexts([title, body])` | Composite full-text index across several fields |
| `@@table_name("audit_users")` | Overrides the generated physical table name |

`@@ids`, `@@uniques`, `@@indexes`, and `@@fulltexts` each take one non-empty array of existing scalar or enum field names, with no duplicates. Every field listed in `@@fulltexts` must be `String` or `String?`.

> [!TIP]
> The formatter always moves model attributes after every field, separated by a blank line — you never have to think about where in the model body a `@@...` declaration goes; run the formatter and it lands in the same place every time.

See [Indexes and constraints](/en-us/docs/orm/guide/indexes) for the full picture on single-field vs. composite indexes, uniqueness, and the indexes Dinoco creates for you automatically.

## The generated new function

Every generated entity gets a `pub fn new(...) -> Self`. Its parameters are exactly the scalar fields that are both required *and* have no default or generator — everything else (optional fields, relation fields, fields with a default) is filled in automatically.

Given:

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

construction only needs the two fields that are actually required inputs:

```rust
let user = User::new("ana@example.com".to_string(), "Ana".to_string());
```

`id` is generated for you, `enabled` takes its declared default, `bio` starts as `None`, and `posts` starts as an empty `Vec`. Every field on the resulting struct is public, so you can adjust any of them — including the ones `new` didn't ask for — before passing the entity to `insert_into`.

## Implicit many-to-many generated fields

An **implicit** many-to-many relation (neither side declares `fields`/`references`) keeps its SQL pivot table entirely internal — there's no public `PostTag`-style entity to import. Instead, each side gets a write-only virtual field: `Post` gets `tag_id: Option<TagId>`, and `Tag` gets `post_id: Option<PostId>`.

Assigning that virtual field before `insert_into`, or on each item of an `insert_many` batch, creates the matching pivot row right after the endpoint itself is inserted:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.post_id = Some(post.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

> [!NOTE]
> This virtual field is not a real column on `tag` — it always reads back as `None`. It exists purely as a write target. For two endpoints that already exist, use the same generated field's `.connect(...)`/`.disconnect(...)` operations from `update` instead of touching it during insert. See [Implicit many-to-many](/en-us/docs/orm/guide/relations#implicit-many-to-many) for the complete contract, including named relations and referential actions.

## Generated files

`dinoco models generate` and `dinoco migrate generate` both produce the same predictable module tree:

```text
dinoco/
  mod.rs
  models/
    mod.rs
    user.rs
    post.rs
```

`dinoco/mod.rs` re-exports every model plus the `connect()` function. Enums are kept compact inside `models/mod.rs` via the `DinocoEnum` derive. One file per model keeps code review focused on whatever actually changed.

The generated root module starts with `#![allow(unused)]`, which suppresses unused-code warnings from the generated module and the files it imports — it has no effect on your own application code. If you need extra derives on generated types, that belongs in `config.custom_derives`, not in a hand-edited generated file (which would just be overwritten); see [Schema organization](/en-us/docs/orm/guide/schema-organization#custom-derives).
