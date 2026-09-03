# Indexes and constraints

Dinoco indexes are declared in `schema.dinoco` and participate in the same migration, introspection, and drift workflow as tables. This page separates explicit indexes, automatic indexes, and full-text search.

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

Standard and full-text declarations represent different strategies. A field cannot participate in both `@index`/`@@indexes` and `@fulltext`/`@@fulltexts`.

## Declare an explicit index

Add `@index` to a scalar or enum field:

```dinoco
model Post {
    id           String   @id @default(uuid())
    slug         String   @index
    published_at DateTime @index(map: "idx_post_publication")
}
```

Without `map`, the name follows `idx_<table>_<field>`. `map` chooses the physical name. This index does not enforce uniqueness; use `@unique` when duplicate values are invalid.

## Declare composite indexes and uniqueness

Place ordered field groups at the end of the model:

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

`@@uniques` rejects duplicate tuples and emits a composite unique index. `@@indexes` is non-unique. Both preserve field order during migration, introspection, and drift comparison. Multiple declarations of either kind are allowed when a model needs several different groups.

The formatter moves every `@@...` declaration after the fields. Each array must be non-empty, contain existing scalar or enum fields, and contain no duplicate field name.

## Primary keys are indexed

Every `@id` field is indexed by its `PRIMARY KEY` constraint. Dinoco represents that index in the desired schema for comparison but does not emit a duplicate `CREATE INDEX`.

A composite primary key preserves its declared order:

```dinoco
model Membership {
    tenant_id String
    user_id   String

    @@ids([tenant_id, user_id])
}
```

The database keeps one composite index over `(tenant_id, user_id)`.

## Foreign keys are indexed

Every materialized relation receives an automatic index over `fields`:

```dinoco
model Session {
    id         String  @id @default(uuid())
    account_id String
    account    Account? @relation(
        fields: [account_id],
        references: [id]
    )
}
```

The generated name is `idx_session_account_id`. Composite relations receive one composite index in the same order as `fields`.

Implicit many-to-many pivot tables receive:

1. the composite primary-key index;
2. an index for the first foreign key;
3. an index for the second foreign key.

Do not repeat `@index` merely because a field already participates in `@id` or `@relation`.

## Full-text indexes

Use `@fulltext` only on `String` or `String?`:

```dinoco
model Article {
    id      String  @id @default(uuid())
    title   String  @fulltext
    summary String? @fulltext
}
```

A model may have several independent `@fulltext` fields. `@@fulltexts([title, summary])` instead creates one ordered full-text document and one native index. Calling `.fulltext(...)` on either generated field searches that complete group. SQLite omits an ineffective B-tree index and searches every group field with `LIKE '%term%'` joined by `OR`.

See [Full-text search](/v1.3.2/orm/full-text-search) for `.fulltext(...)` across every find builder.

## Migration workflow

After changing an index:

```bash
dinoco migrate generate
dinoco migrate run
```

The planner emits `CREATE INDEX`, `DROP INDEX`, or the adapter's full-text variant. Introspection compares index name, columns, order, and kind. Snapshots created before v1.2.0 remain compatible: a historically missing `indexes` property is not interpreted as an intentional removal.

## Validation rules

- `@index` accepts only `map: "name"`.
- `@index` works on scalar and enum fields, not relation fields.
- `@fulltext` accepts no arguments.
- `@fulltext` works only on `String` and `String?`.
- A field cannot belong to both a standard and a full-text declaration, including the composite forms.
- Multiple `@fulltext` fields are allowed per model.
- Every `@@fulltexts` member must be `String` or `String?` and may belong to only one full-text declaration.
- `@@uniques`, `@@indexes`, and `@@fulltexts` may be repeated for different ordered groups.
