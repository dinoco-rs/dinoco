# Full-text search

Dinoco provides native full-text search on PostgreSQL and MySQL with a substring fallback on SQLite. The `.fulltext(...)` method is generated only for fields that enable the feature in `schema.dinoco`.

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

A model may have several `@fulltext` fields. Each declaration receives its own index on PostgreSQL and MySQL.

The rules are:

- only `String` and `String?`;
- no arguments;
- cannot coexist with `@index` on the same field;
- cannot be applied to a relation.

### Build one document from several fields

Use `@@fulltexts` when one search should cover an ordered field group:

```dinoco
model Article {
    id       String  @id @default(uuid())
    title    String
    subtitle String?
    body     String

    @@fulltexts([title, subtitle, body])
}
```

All members receive the generated `.fulltext(...)` capability. Calling `article.title.fulltext("dinoco")`, `article.subtitle.fulltext("dinoco")`, or `article.body.fulltext("dinoco")` searches the same combined document. This is important on MySQL: the query uses `MATCH(title, subtitle, body)`, exactly matching the composite `FULLTEXT` index.

Use separate field-level `@fulltext` declarations when fields need independent indexes. A field may belong to only one full-text declaration and cannot also participate in `@index` or `@@indexes`.

## 2. Generate migrations and models

```bash
dinoco migrate generate
dinoco migrate run
```

In addition to a native index where applicable, the generated Rust model receives the capability that enables the method:

```rust
account.name.fulltext("matheus");
```

`account.email.fulltext(...)` does not compile because `email` has neither `@fulltext` nor membership in `@@fulltexts`.

## Use in find first

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

The return remains `Option<Account>`.

## Use in find many

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .order_by(|account| account.id.asc())
    .execute(&client)
    .await?;
```

The return remains `Vec<Account>`.

## Use in find and update

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.biography.fulltext("dinoco"))
    .update(|account| account.reviewed.set(true))
    .execute(&client)
    .await?;
```

The search selects the row to update and return. See [Find and update](/v1.2.3/orm/find-and-update).

## Use in relation includes

The method works in generated one and many include builders:

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

The related entity field must also have `@fulltext`.

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

Full-text can appear in any `and`, `or`, `or_many`, or `not` group.

## Use in transactions

Full-text conditions remain part of a mutation executed through the transaction context:

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

The adapter compiles the same full-text predicate into the `UPDATE` on the transaction connection.

## Database behavior

| Adapter | Query | Index |
| --- | --- | --- |
| PostgreSQL / PgBouncer | `to_tsvector('simple', ...) @@ plainto_tsquery('simple', ...)` | Expression GIN |
| MySQL | `MATCH (...) AGAINST (... IN NATURAL LANGUAGE MODE)` | `FULLTEXT` |
| SQLite | `(field_a LIKE '%term%' OR field_b LIKE '%term%')` | No native index |

For composite declarations, PostgreSQL builds the same concatenated text document in the GIN index and query. MySQL keeps the exact declared `MATCH(...)` column list. SQLite searches a substring in every group member and may scan large tables.

## Migration behavior

PostgreSQL and MySQL use `idx_<table>_<field...>_fulltext`. The planner preserves the declared field order and creates, drops, introspects, and drift-checks these indexes separately from standard and unique indexes.

SQLite omits the index migration because an ordinary B-tree does not accelerate a leading-wildcard `LIKE`.

## Limitations

- `@fulltext` creates one independent single-field index; `@@fulltexts([...])` creates one combined document.
- A field can belong to only one full-text declaration and cannot overlap `@index` or `@@indexes`.
- Every composite member must be `String` or `String?`.
- Ranking, configurable language, phrases, and custom tokenizers are outside v1.2.3.
- SQLite substring semantics differ from native token search.
