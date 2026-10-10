# Full-text search

Dinoco provides native full-text search on PostgreSQL and MySQL, with an automatic substring-based fallback on SQLite so the same application code works everywhere. The `.fulltext(...)` method only exists on fields that explicitly opt into the feature in `schema.dinoco` — there's no way to call it by accident on a field that isn't indexed for it.

## 1. Mark searchable fields

```dinoco
model Account {
    id        String  @id @default(uuid())
    name      String  @fulltext
    biography String? @fulltext
    email     String
    reviewed  Boolean @default(false)
}
```

A model can have several independent `@fulltext` fields; each gets its own dedicated index on PostgreSQL and MySQL. The rules are narrow and enforced at compile time:

- Only `String` and `String?`.
- No arguments.
- Can't coexist with `@index` on the same field.
- Can't be applied to a relation field.

### Build one document from several fields

Reach for `@@fulltexts` when a single search should span an ordered group of fields together, as one combined document:

```dinoco
model Article {
    id       String  @id @default(uuid())
    title    String
    subtitle String?
    body     String

    @@fulltexts([title, subtitle, body])
}
```

Every member of the group gets the generated `.fulltext(...)` capability, and calling it from *any* of them — `article.title.fulltext("dinoco")`, `article.subtitle.fulltext(...)`, or `article.body.fulltext(...)` — searches the exact same combined document, not just that one field. This matters concretely on MySQL: the generated query is a single `MATCH(title, subtitle, body)`, matching the composite `FULLTEXT` index column-for-column.

> [!TIP]
> Use separate field-level `@fulltext` declarations instead when fields genuinely need *independent* indexes and searches. A field can only ever belong to one full-text declaration — never both a solo `@fulltext` and membership in a `@@fulltexts` group — and it still can't overlap `@index`/`@@indexes` either way.

## 2. Generate migrations and models

```bash
dinoco migrate generate
dinoco migrate run
```

Alongside the native index (where the adapter supports one), the generated Rust model gets exactly the capability needed to call `.fulltext(...)`:

```rust
account.name.fulltext("matheus");
```

`account.email.fulltext(...)` simply doesn't compile — `email` has neither `@fulltext` nor membership in any `@@fulltexts` group, so the method doesn't exist on it at all.

## Use in find first

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

The return type is unaffected — still `Option<Account>`.

## Use in find many

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .order_by(|account| account.id.asc())
    .execute(&client)
    .await?;
```

Still `Vec<Account>` — full-text is just another condition, composable with everything else `find_many` supports.

## Use in find and update

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.biography.fulltext("dinoco"))
    .update(|account| account.reviewed.set(true))
    .execute(&client)
    .await?;
```

The full-text condition here selects which row gets updated and returned — see [Find and update](/en-us/docs/orm/orm/find-and-update) for the rest of that builder's behavior.

## Use in relation includes

The same method works inside generated to-one and to-many include builders:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account
            .sessions()
            .where_(|session| session.label.fulltext("mobile"))
    })
    .execute(&client)
    .await?;
```

The related entity's field needs `@fulltext` in its own right — it isn't inherited from the parent model's configuration.

## Combine with where complex

```rust
let accounts = dinoco::find_many::<Account>()
    .where_complex(|account, m| {
        m.and([
            account.biography.fulltext("dinoco"),
            m.not(account.biography.fulltext("deprecated")),
        ])
    })
    .execute(&client)
    .await?;
```

A `.fulltext(...)` call is a normal condition value as far as `where_complex` is concerned — it can appear inside any `and`, `or`, `or_many`, or `not` group, nested arbitrarily.

## Use in transactions

Full-text conditions carry through unchanged into a mutation executed via the transaction context:

```rust
let account = dinoco::transaction(&client, |tx| async move {
    let account = dinoco::find_and_update::<Account>()
        .where_(|account| account.name.fulltext("matheus"))
        .update(|account| account.reviewed.set(true))
        .execute(tx)
        .await?;

    Ok(account)
})
.await?;
```

The adapter compiles the exact same full-text predicate into the `UPDATE` it runs on the transaction's connection — nothing about full-text search behaves differently inside a transaction.

## Database behavior

| Adapter | Query | Index |
| --- | --- | --- |
| PostgreSQL / PgBouncer | `to_tsvector('simple', ...) @@ plainto_tsquery('simple', ...)` | Expression GIN |
| MySQL | `MATCH (...) AGAINST (... IN NATURAL LANGUAGE MODE)` | `FULLTEXT` |
| SQLite | `(field_a LIKE '%term%' OR field_b LIKE '%term%')` | No native index |

For a composite `@@fulltexts` declaration, PostgreSQL builds one concatenated text document used identically by both the GIN index and every query against it. MySQL keeps the exact declared column list in its `MATCH(...)` clause. SQLite, having no native full-text mechanism, searches a plain substring across every member field — which means it can end up scanning the whole table on a large dataset.

## Migration behavior

PostgreSQL and MySQL name their generated indexes `idx_<table>_<field...>_fulltext`. The migration planner preserves declared field order and handles creation, removal, introspection, and drift detection for these separately from ordinary standard/unique indexes.

> [!NOTE]
> SQLite skips generating any index migration for full-text fields at all — an ordinary B-tree index provides no speedup for a leading-wildcard `LIKE '%term%'` query, so Dinoco doesn't create one that would just take up space and write overhead for nothing.

## Limitations

- `@fulltext` creates one independent, single-field index; `@@fulltexts([...])` creates one combined multi-field document instead.
- A field belongs to at most one full-text declaration, and never overlaps `@index`/`@@indexes`.
- Every member of a composite group must be `String` or `String?`.
- Relevance ranking, configurable language/stemming, phrase search, and custom tokenizers are all outside what Dinoco's full-text search does today — this is intentionally the common subset across three very different database full-text implementations, not a wrapper around each one's full feature set.
- SQLite's substring-based fallback has different match semantics than PostgreSQL/MySQL's real token-based search — expect `LIKE`-style behavior there, not stemming or word-boundary awareness.
