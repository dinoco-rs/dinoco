# Defaults and enums

Defaults belong in the schema when the value is a database rule or a library-managed identifier. Dinoco uses them in migrations and also excludes those fields from the generated `new()` constructor.

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

Use a value compatible with the field type. A migration preserves that default in the database, so inserts performed outside Dinoco observe the same rule.

## Generated values

The supported default functions are:

- `autoincrement()` for `Integer` fields.
- `uuid()` for `String` fields.
- `snowflake()` for `Integer` fields.
- `now()` for `DateTime` or `Date` fields.

The compiler rejects a function on an incompatible scalar instead of postponing the error until migration execution.

## UUID

```dinoco
id String @id @default(uuid())
```

The generated Rust type is `dinoco::Uuid`, an ID-oriented string type understood by the insert pipeline. Dinoco generates the value before related rows are inserted, so nested one-to-many, many-to-one, and one-to-one inserts can use the new key in the same operation.

## Snowflake

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_replicas = []
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

model Event {
    id Integer @id @default(snowflake())
}
```

The Rust field becomes `dinoco::Snowflake`, backed by `i64`. The node ID is mandatory and must come from the environment.

## Auto-increment

```dinoco
id Integer @id @default(autoincrement())
```

The database creates the integer. Dinoco retrieves generated values when an insert must return the entity or propagate the key to nested relation rows. The exact SQL is emitted by the active adapter's compiler.

## Enums

Declare an enum once, then use it as a field type and as a default:

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

Generated Rust variants use PascalCase while database values preserve the schema spelling:

```rust
pub enum Role {
    User,
    Admin,
}
```

PostgreSQL receives a native enum type. MySQL uses its dialect's enum representation. SQLite stores and validates a compatible scalar representation because it has no equivalent native enum type. Adapter-specific row conversions preserve the same Rust enum API.

## Changing enums safely

`dinoco migrate generate` compares enum definitions as well as tables. Adding a value is usually additive. Renaming or removing a value can invalidate existing rows, so the migration plan reports the destructive risk and asks for confirmation when data may be lost.

Review both `up.sql` and `down.sql`. A database may not support reversing every enum operation without rebuilding dependent columns, and the generated SQL should remain part of your code review.
