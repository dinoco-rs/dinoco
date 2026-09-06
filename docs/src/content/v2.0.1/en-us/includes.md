# Includes

`includes(...)` is how you opt in to loading a relation field. Without it, relations keep whatever empty value they're generated with: `Vec::new()` for a to-many relation, `None` for a to-one one — Dinoco never loads data you didn't ask for.

## Include a many relation

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

Dinoco collects every parent's key first, then loads all the matching children in one batched follow-up query — never one query per parent row.

## Include a one relation

```rust
let session = dinoco::find_first::<AccountSession>()
    .where_(|session| session.id.eq(&session_id))
    .includes(|session| session.account())
    .execute(&client)
    .await?;
```

To-one relations use a left-join-equivalent strategy and stay `None` gracefully when nothing matches — an unmatched to-one include is not an error.

## Filter the relation

The relation builder inside `.includes(...)` exposes the same generated filters as a top-level find:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account
            .sessions()
            .where_(|session| session.revoked.eq(false))
            .order_by(|session| session.created_at.desc())
            .take(5)
            .skip(0)
    })
    .execute(&client)
    .await?;
```

> [!NOTE]
> For a to-many relation, `take(5)` applies **per parent**, not to the combined result across all parents — each account gets up to five of its own most recent sessions. Under the hood, the compiler achieves this with a window-partitioned query rather than issuing a separate query per parent.

## Use where complex and full-text

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account.sessions().where_complex(|session, m| {
            m.and([
                session.label.fulltext("mobile"),
                m.not(session.revoked.eq(true)),
            ])
        })
    })
    .execute(&client)
    .await?;
```

The same rule as top-level finds applies here too: once a relation builder uses `where_complex`, every plain `where_` on that same builder is ignored. `.fulltext(...)` is only available when the related field actually has `@fulltext` in the schema.

## Build nested includes

```rust
let projects = dinoco::find_many::<Project>()
    .includes(|project| project.owner())
    .includes(|project| {
        project
            .tasks()
            .order_by(|task| task.priority.desc())
            .take(10)
            .includes(|task| task.assignee())
    })
    .execute(&client)
    .await?;
```

Sibling includes — `owner()` and `tasks()` here — are awaited in parallel rather than one after another, and each nested level repeats whatever loading strategy is appropriate for its own relation kind (batched for to-many, left-join-equivalent for to-one), all the way down.

## Combine with select

The relation builder also accepts `select::<S>()`, exactly like a top-level find. The relation's key is tracked separately from the projection internally, purely for grouping rows back to their parent correctly — the projection itself never needs to expose that key as a field. See [Select](/en-us/docs/orm/orm/select).

## Primary and transactions

`read_in_primary()` on the parent find routes both the parent row *and* every include underneath it to the primary — there's no way to keep an include on a replica while its parent reads from the primary.

The closure transaction API only accepts mutation builders, so run reads that use `.includes(...)` through `&client` directly, before or after a `transaction(...)` closure rather than inside it.
