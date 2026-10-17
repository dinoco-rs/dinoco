# Indexes and constraints

Indexes in Dinoco are declared right in `schema.dinoco`, alongside the fields they cover, and flow through the same migration, introspection, and drift-detection pipeline as tables — there's no separate "index management" step. This page covers three distinct mechanisms: explicit standard indexes, the indexes Dinoco creates automatically, and full-text search.

## Choose the index type

| Requirement | Declaration | Result |
| --- | --- | --- |
| Speed up equality, ordering, or ranges | `@index` | Non-unique B-tree index |
| Speed up an ordered field group | `@@indexes([...])` | Composite non-unique index |
| Enforce uniqueness for a field group | `@@uniques([...])` | Composite unique index |
| Search text tokens | `@fulltext` | PostgreSQL GIN, MySQL `FULLTEXT`, or SQLite fallback |
| Search one text document built from several fields | `@@fulltexts([...])` | Composite native full-text index or SQLite fallback |
| Primary key | `@id` or `@@ids` | Automatic constraint index |
| Foreign key | `@relation(fields: [...])` | Automatic index over local columns |

> [!NOTE]
> Standard and full-text indexing are two different strategies, not a spectrum — a field can't participate in both. `@index`/`@@indexes` and `@fulltext`/`@@fulltexts` are mutually exclusive on the same field.

## Declare an explicit index

Add `@index` directly to a scalar or enum field:

```dinoco
model Post {
    id           String   @id @default(uuid())
    slug         String   @index
    published_at DateTime @index(map: "idx_post_publication")
}
```

Without `map`, the physical index name follows `idx_<table>_<field>` automatically. Pass `map` when you need a specific name — for matching an existing index during a migration from another tool, for instance. Either way, `@index` never implies uniqueness on its own; reach for `@unique` when duplicate values should be rejected.

## Declare composite indexes and uniqueness

Composite declarations are model attributes, placed at the end of the model body:

```dinoco
model Article {
    tenant_id String
    id        String
    slug      String
    category  String
    title     String
    body      String?

    @@ids([tenant_id, id])
    @@uniques([tenant_id, slug])
    @@indexes([tenant_id, category])
    @@fulltexts([title, body])
}
```

`@@uniques` rejects duplicate tuples at the database level and produces a composite unique index; `@@indexes` produces the same shape without the uniqueness constraint. Both preserve field order faithfully through migration, introspection, and drift comparison — order matters for a composite index's usefulness, so Dinoco never silently reorders it. A model can declare several `@@uniques`/`@@indexes`/`@@fulltexts` attributes when it needs more than one distinct group.

> [!TIP]
> Every array here must be non-empty, reference fields that actually exist on the model, and contain no duplicate field name — and the formatter always relocates `@@...` declarations to after the fields, so don't worry about where in the model body you write them.

## Primary keys are indexed

Every `@id` field is already indexed, by its own `PRIMARY KEY` constraint — Dinoco tracks that index in its internal comparison model (so drift detection knows it's accounted for) but never emits a redundant second `CREATE INDEX` for it.

A composite primary key keeps its declared field order in that same index:

```dinoco
model Membership {
    tenant_id String
    user_id   String

    @@ids([tenant_id, user_id])
}
```

The database ends up with one composite index over `(tenant_id, user_id)`, in that order — useful for queries filtering on `tenant_id` alone, less so for queries filtering on `user_id` alone.

## Foreign keys are indexed

Every materialized relation gets an automatic index over its `fields`, with no `@index` needed:

```dinoco
model Session {
    id         String   @id @default(uuid())
    account_id String
    account    Account? @relation(
        fields: [account_id],
        references: [id]
    )
}
```

This generates `idx_session_account_id`. A composite relation (multiple `fields`) gets one composite index in the same order the relation declares them.

Implicit many-to-many pivot tables get three indexes automatically:

1. The composite primary-key index.
2. An index on the first side's foreign key.
3. An index on the second side's foreign key.

> [!WARNING]
> Don't add `@index` to a field just because it's already part of `@id` or `@relation` — you'd be asking Dinoco to create a second, redundant index alongside the automatic one, wasting write throughput and disk space for no query benefit.

## Full-text indexes

`@fulltext` only works on `String` or `String?`:

```dinoco
model Article {
    id      String  @id @default(uuid())
    title   String  @fulltext
    summary String? @fulltext
}
```

A model can have several **independent** `@fulltext` fields, each searchable on its own. `@@fulltexts([title, summary])` is a different thing entirely: it builds *one* ordered document out of both fields and backs it with *one* native index, so calling `.fulltext(...)` on either generated field searches across the whole combined group. On SQLite — which has no native full-text index — Dinoco skips creating a B-tree index that wouldn't help anyway, and instead searches every field in the group with `LIKE '%term%'` joined by `OR`.

See [Full-text search](/en-us/docs/orm/orm/full-text-search) for how `.fulltext(...)` shows up across every find builder.

## Migration workflow

Same as any other schema change — after editing an index:

```bash
dinoco migrate generate
dinoco migrate run
```

The planner emits `CREATE INDEX`, `DROP INDEX`, or the active adapter's full-text-specific variant as needed. Introspection compares each index's name, columns, order, and kind against what the schema currently describes, so an index changed outside Dinoco is still detected as drift.

## Long index names

PostgreSQL keeps at most 63 bytes of a name and silently cuts the rest; MySQL refuses names over 64 characters. Generated names grow with the table and field names, so they can pass those limits:

```dinoco
model BusinessReceivableTransaction {
    id          Integer   @id @default(autoincrement())
    business_id Integer
    executed_at DateTime?
    unlock_at   DateTime

    @@indexes([business_id, executed_at, unlock_at])
}
```

The full name, `idx_business_receivable_transaction_business_id_executed_at_unlock_at`, has 71 characters. Dinoco cuts every generated index, unique, full-text and foreign key name to the database's limit, the same way PostgreSQL does, so the migration creates the name the database stores and introspection finds it again:

| Database   | Limit          | Index name                                                          |
| ---------- | -------------- | ------------------------------------------------------------------- |
| PostgreSQL | 63 bytes       | `idx_business_receivable_transaction_business_id_executed_at_unl`  |
| MySQL      | 64 characters  | `idx_business_receivable_transaction_business_id_executed_at_unlo` |
| SQLite     | none           | `idx_business_receivable_transaction_business_id_executed_at_unlock_at` |

When two names of the same table become equal once cut, the second one gets a `_2` suffix (then `_3`, ...) that still fits the limit. A name passed with `map` is cut the same way. Implicit many-to-many pivot tables and their columns follow the same rule.

> [!NOTE]
> Before v2.1.5, PostgreSQL cut these names on its own while Dinoco kept comparing against the full name, so every `dinoco migrate` reported `Missing index` and `Unexpected index` for the same index. The cut names match what PostgreSQL already stored, so upgrading needs no migration.

Names you choose are never cut: a table, column or enum name longer than the limit makes `migrate generate` stop before it connects, listing every such name. Shorten them, giving a long model a shorter `@@table_name("...")`. PostgreSQL would otherwise cut the name on its own, so the database would never match the schema, and MySQL would refuse the migration halfway.

## Validation rules

- `@index` accepts only the optional `map: "name"` argument.
- `@index` only works on scalar and enum fields — never on a relation field.
- `@fulltext` accepts no arguments at all.
- `@fulltext` only works on `String` and `String?`.
- A field can never belong to both a standard and a full-text declaration, including their composite forms.
- Multiple `@fulltext` fields are fine on the same model.
- Every member of `@@fulltexts` must be `String` or `String?`, and can belong to at most one full-text declaration.
- `@@uniques`, `@@indexes`, and `@@fulltexts` can each be repeated for different, independent field groups.
