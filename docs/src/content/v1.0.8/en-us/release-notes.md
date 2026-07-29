# Dinoco v1.0.8

Dinoco v1.0.8 adds schema indexes, atomic transaction batches, and explicitly grouped boolean filters. The release is available across SQLite, PostgreSQL Direct, PgBouncer, and MySQL unless a limitation is called out below.

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

Transactions accept finds, counts, flat inserts, scalar updates, and deletes. SQLite and PostgreSQL also support `returning` and `find_and_update` in a batch. The detailed limitations for includes, nested relation payloads, relation connect/disconnect, and MySQL returning operations are listed on the Transactions page.

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

The v1.0.8 behavior is covered by parser, migration, query-builder, adapter, and documentation tests. The database integration suite exercises SQLite, PostgreSQL, and MySQL; PgBouncer uses the same PostgreSQL compiler and transactional execution path.
