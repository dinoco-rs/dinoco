# Dinoco v1.1.6

Dinoco v1.1.6 fixes enum values in `find_and_update`, `update`, and `update_many`, and uses each database's native enum support: named enum types in PostgreSQL, inline `ENUM` columns in MySQL, and checked `TEXT` columns in SQLite. `find_and_update` now returns an error when no row is affected.

This release also preserves acronym groups in generated table names (`BusinessCNAE` becomes `business_cnae`, while `BusinessOffice` remains `business_office`) and makes `migrate generate` show the detected changes and require `Y` confirmation before creating or applying a migration and regenerating models. It includes the workspace, runtime migration, Serde, transaction, relation, index, and query-builder improvements introduced during the v1.1 series.

## Bidirectional enum string conversion

Enums generated from schema values continue to use idiomatic PascalCase Rust variants. `.to_string()` returns the original schema value exactly, while `TryFrom<&str>`, `TryFrom<String>`, and `FromStr` convert that value back to the enum:

```dinoco
enum PaymentState {
    waiting_payment
    paid
}
```

```rust
PaymentState::WaitingPayment.to_string() // "waiting_payment"
PaymentState::Paid.to_string()           // "paid"

PaymentState::try_from("waiting_payment")?             // PaymentState::WaitingPayment
PaymentState::try_from("waiting_payment".to_string())? // PaymentState::WaitingPayment
"waiting_payment".parse::<PaymentState>()?             // PaymentState::WaitingPayment
```

Unknown values return an error instead of panicking. `DinocoEnum` implements every conversion from each variant's `#[dinoco(value = "...")]` mapping, so manually derived enums receive the same behavior.

## Implicit many-to-many endpoint API

An implicit relation such as `Business.systems System[]` and `System.business Business[]` still creates `_business_to_system` in SQL, but no longer generates a public `BusinessSystem` Rust entity. Instead, codegen adds two write-only virtual keys:

- `Business.system_id: Option<SystemId>`;
- `System.business_id: Option<BusinessId>`.

Reads leave these fields as `None`. They never compile into `SELECT business.system_id` or `SELECT system.business_id`. Relation navigation continues through `Business.systems` and `System.business`, and the include loader now joins the target model through the real pivot table. Nested includes, related-model filters, ordering, per-parent pagination, and relation counts use the same pivot-aware path.

Set a virtual key before `insert_into` or on each `insert_many` payload to create one pivot row per inserted endpoint. The same field works from `update`, `update_many`, `find_and_update`, and returning writes:

```rust
let mut system = System::new(
    "ERP".to_string(),
    "Enterprise resource planning".to_string(),
);
system.business_id = Some(business_id);

dinoco::insert_into::<System>()
    .values(&system)
    .execute(&client)
    .await?;

dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system_id))
    .execute(&client)
    .await?;
```

For `insert_many`, set the virtual key separately on each item; `None` inserts that endpoint without a link. `disconnect` removes only the matching pivot row. The same virtual insert keys and relation `connect`/`disconnect` operations now work inside transaction batches and roll back atomically with scalar writes. Transactional virtual-key inserts require an endpoint ID known before execution, such as UUID, Snowflake, or a caller-provided ID. Existing projects should regenerate models and replace direct uses of generated implicit-pivot entities; the SQL table and migration history remain unchanged.

## Named workspaces

A single schema can now define independent database configurations under `config.workspace`:

```dinoco
config {
    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database = "postgresql"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

Pass `--workspace dev` or `-w dev` to migration and model commands. When it is omitted, the CLI prompts for the workspace. Migration artifacts are isolated under `dinoco/migrations/<workspace>/`, and model generation clears output from a previously selected workspace before regenerating it.

## Opt-in SQLite runtime migrations

Generated code now embeds the selected workspace's migrations and exports `dinoco::migrate(&client)`. The generated `connect()` function only connects to SQLite and creates its file when needed; it never applies application migrations automatically:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;
```

Applications that need more control can use `dinoco::runtime::run_migrations` with their own `Migration` values. Runtime migrations are ordered, transactional, idempotent, and protected by checksums.

## Serializable generated enums

Enums emitted by codegen now derive `serde::Serialize` and `serde::Deserialize` through Dinoco's public re-export. Generated models can therefore be used directly in JSON payloads and other Serde-compatible formats without adding derives by hand.

## Axum and Send compatibility

`transactions(transaction).execute(&client)` now returns a `Send` future. Transaction result adapters are both `Send` and `Sync`, allowing PostgreSQL and MySQL transaction commands to remain borrowed across asynchronous database calls without making the enclosing handler future non-`Send`.

Compile-time regression coverage now checks every public CRUD operation future together with transaction execution. This keeps `find`, `count`, `insert`, `update`, `delete`, and transaction builders compatible with Axum's multithreaded handler requirements.

## Schema indexes

Add a non-unique index to a scalar or enum field with `@index`, optionally choosing its physical name with `map`:

```dinoco
model Post {
    id           Integer  @id @default(autoincrement())
    slug         String   @index
    published_at DateTime @index(map: "idx_post_publication")
}
```

Every primary key and foreign key now receives an automatic index. The primary-key constraint supplies the physical index, so Dinoco does not create a duplicate. Composite primary keys and relations preserve column order, and implicit many-to-many pivot tables receive their composite primary-key index plus one index for each foreign key.

Model-level `@@indexes([...])` and `@@uniques([...])` add ordered composite standard and unique indexes. The schema compiler also enforces exactly one primary-key declaration per model: one `@id` or one `@@ids([...])`.

## Full-text search

String fields marked with `@fulltext` expose a generated `.fulltext(term)` condition. PostgreSQL creates and introspects a GIN expression index, MySQL uses a native `FULLTEXT` index, and SQLite falls back to `LIKE '%term%'` without creating an ineffective B-tree index. Multiple full-text fields are allowed per model, but `@fulltext` cannot share a field with `@index`.

The migration engine plans, applies, reverses, and introspects index changes. It generates `CREATE INDEX` and `DROP INDEX`, detects drift, and remains compatible with snapshots created before indexes were recorded.

`@@fulltexts([...])` builds one searchable document from several String fields. PostgreSQL uses the same concatenated expression for its GIN index and query, MySQL uses the exact composite `MATCH(...)` list, and SQLite joins substring fallbacks with `OR`. Every member exposes `.fulltext(...)`, which searches the complete group.

The formatter moves all model-level `@@...` declarations after the fields. The VS Code extension highlights and completes the new declarations, completes referenced fields inside arrays, resolves their definitions and references, and reports missing or multiple primary keys.

## Atomic transactions

Use `Transaction`, `transactions`, or the `transaction!` macro to execute heterogeneous builders in insertion order on one physical primary connection:

```rust
let mut transaction = dinoco::Transaction::new();

transaction.push(
    dinoco::find_first::<Account>()
        .where_(|x| x.id.eq("account-1"))
);
transaction.push(
    dinoco::insert_into::<AccountSession>().values(&session)
);

let mut results = dinoco::transactions(transaction)
    .execute(&client)
    .await?;

let account: Option<Account> = results.take(0)?;
results.take::<()>(1)?;
```

The batch commits only after every operation succeeds and rolls back on SQL, constraint, or row-conversion errors. Results keep their normal Rust types and their push order. `Transcation` remains available as a compatibility alias for the spelling used in early examples.

Transactions accept finds, counts, inserts, updates, deletes, implicit many-to-many `connect`/`disconnect`, and populated virtual keys on `insert_into` and `insert_many`. SQLite and PostgreSQL also support `returning` and `find_and_update` in a batch. The detailed limitations for includes, nested non-many-to-many relation payloads, autoincrement virtual-key inserts, and MySQL returning operations are listed on the Transactions page.

## Complex filters

`where_complex` builds nested boolean expressions with explicit precedence:

```rust
let account = dinoco::find_first::<Account>()
    .where_complex(|x, m| {
        m.or(
            m.and([
                x.id.eq("account-1"),
                x.name.eq("Matheus"),
            ]),
            m.not(x.disabled.eq(true)),
        )
    })
    .execute(&client)
    .await?;
```

The manipulation value `m` provides `and`, `or`, `or_many`, and `not`; `x` is the generated `EntityWhere`. The same generated field can be reused in multiple branches. Once a builder uses `where_complex`, every `where_` on that builder is ignored, regardless of call order.

The API is available on `find_first`, `find_many`, `find_and_update`, and the find builders used by relation includes. Complex filters are preserved when those builders are executed in a transaction.

## Compatibility and verification

The v1.1.6 behavior is covered by parser, migration, query-builder, adapter, and documentation tests. The database integration suite exercises SQLite, PostgreSQL, and MySQL; PgBouncer uses the same PostgreSQL compiler and transactional execution path.
