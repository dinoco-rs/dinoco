# Defaults and enums

A default belongs in the schema whenever the value is a database rule, or an identifier the library itself manages — not something your application logic decides. Beyond seeding the column's default at the database level, declaring `@default(...)` has a second effect worth knowing early: it removes that field from the generated `new()` constructor's parameter list, since Dinoco (or the database) already knows how to produce a value for it.

## Literal defaults

Boolean, numeric, string, and enum literals can be declared directly:

```dinoco
model Feature {
    id      Integer @id @default(autoincrement())
    name    String
    enabled Boolean @default(false)
    weight  Float   @default(1.0)
}
```

The literal has to be compatible with the field's type — `@default(false)` on a `Boolean`, `@default(1.0)` on a `Float`, and so on. This default is written into the actual database migration, not just enforced in Rust, so rows inserted by anything other than your Dinoco application (a manual `INSERT`, a different service) still get the same value.

## Generated values

Four generator functions are supported, each tied to a specific scalar type:

- `autoincrement()` — `Integer` fields only.
- `uuid()` — `String` fields only.
- `snowflake()` — `Integer` fields only.
- `now()` — `DateTime` or `Date` fields only.

> [!NOTE]
> The compiler checks this pairing at compile time, not at migration time. `id String @default(snowflake())` fails immediately with a clear error, instead of producing a schema that only breaks once you try to run a migration against it.

## UUID

```dinoco
id String @id @default(uuid())
```

The generated Rust type is `dinoco::Uuid` — a string-backed identifier type the insert pipeline understands specifically as an ID, not just any `String`. Dinoco generates the actual value client-side, before the row is inserted, which is what makes it possible to insert a parent and its related rows (one-to-many, many-to-one, one-to-one) in the same logical operation: the child rows can reference the parent's new key before the parent row has even hit the database.

## Snowflake

```dinoco
config {
    database          = "postgresql"
    database_url      = env("DATABASE_URL")
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

model Event {
    id Integer @id @default(snowflake())
}
```

The Rust field becomes `dinoco::Snowflake`, backed by `i64` — sortable by creation time, unlike a random UUID, which is why it's often preferred for high-write tables where insertion order matters. The node ID is mandatory the moment any field anywhere in the schema uses `snowflake()`, and it must come from the environment (see [Snowflake IDs](/en-us/docs/orm/guide/configuration#snowflake-ids)).

## Auto-increment

```dinoco
id Integer @id @default(autoincrement())
```

The database itself creates the integer — Dinoco doesn't generate it client-side the way it does UUIDs and Snowflakes. When an insert needs to return the entity, or propagate the new key into a nested relation write, Dinoco retrieves the generated value using whatever mechanism the active adapter's SQL compiler emits (`RETURNING` on PostgreSQL and SQLite, a follow-up read on MySQL).

## Enums

Declare an enum once, then use it both as a field's type and inside a `@default(...)`:

```dinoco
enum Role {
    USER
    ADMIN
}

model User {
    id   Integer @id @default(autoincrement())
    role Role    @default(USER)
}
```

Generated Rust variants are `PascalCase`, while the value actually stored in the database preserves the exact spelling from the schema. Codegen emits a compact, Serde-ready enum built on the `DinocoEnum` derive:

```rust
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    dinoco::serde::Serialize,
    dinoco::serde::Deserialize,
    dinoco::DinocoEnum,
)]
#[serde(crate = "::dinoco::serde")]
pub enum Role {
    #[default]
    #[dinoco(value = "USER")]
    #[serde(rename = "USER")]
    User,

    #[dinoco(value = "ADMIN")]
    #[serde(rename = "ADMIN")]
    Admin,
}
```

`DinocoEnum` generates every conversion `DinocoValue`, SQLite, PostgreSQL, and MySQL need — there's no per-adapter implementation to hand-write or keep in sync.

Each variant also gets `#[serde(rename = "...")]`, so JSON (de)serialization round-trips through the exact database value instead of the PascalCase Rust name. The same mapping powers `Display`, so `.to_string()` returns the schema's spelling too, and the reverse direction — `TryFrom<&str>`, `TryFrom<String>`, `FromStr` — returns an error for anything that isn't a known value rather than panicking. Concretely: `waiting_payment` becomes the variant `PaymentState::WaitingPayment`; `.to_string()` on it returns `"waiting_payment"` again, and `PaymentState::try_from("waiting_payment")` reconstructs the exact same variant.

> [!TIP]
> Already have a Rust enum you'd rather write by hand than generate? Derive `DinocoEnum` on it directly and map each database value explicitly with `#[dinoco(value = "...")]`. Only unit variants are supported (no `Variant(T)` or `Variant { field: T }`):
>
> ```rust
> #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, dinoco::DinocoEnum)]
> enum PaymentState {
>     #[default]
>     #[dinoco(value = "waiting-payment")]
>     Waiting,
>
>     #[dinoco(value = "paid")]
>     Paid,
> }
> ```

Storage differs by adapter but the Rust-facing API doesn't: PostgreSQL gets a real native enum type, MySQL uses its own dialect's enum column, and SQLite — which has no native enum concept — stores and validates a compatible scalar representation instead.

## Changing enums safely

`dinoco migrate generate` diffs enum definitions the same way it diffs tables. Adding a new value is usually a purely additive change. Renaming or removing one is different: existing rows may still hold the old value, so Dinoco's migration planner flags that as a destructive risk and asks for confirmation before generating a plan that could invalidate data.

> [!WARNING]
> Always read both `up.sql` and `down.sql` for an enum change before applying it. Not every database can cleanly reverse an enum rename or removal without rebuilding a dependent column, so the generated `down.sql` deserves the same scrutiny as `up.sql` — don't assume rollback is free just because Dinoco generated a file for it.
