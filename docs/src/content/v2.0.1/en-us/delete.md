# Delete

Dinoco deliberately splits deletion into two builders with different safety guarantees: single-purpose `delete`, which enforces a filter at compile time, and bulk `delete_many`, which allows an intentionally unfiltered, table-wide operation when that's genuinely what you want.

## Delete one record

```rust
dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .execute(&client)
    .await?;
```

Additional `.where_(...)` calls are allowed and combine with `AND`, exactly like every other builder.

## The mandatory filter

`delete::<M>()` starts life in a type state that simply has no `.execute()` method at all — only calling `.where_(...)` moves it into a state where `.execute()` exists. This means the following genuinely fails to compile, not just at runtime:

```rust
// Intentionally invalid: delete requires where_.
dinoco::delete::<User>()
    .execute(&client)
    .await?;
```

> [!TIP]
> This type-state trick is what prevents the single most common accidental mistake with a delete API — forgetting the filter and wiping a table — while keeping the filtered, common-case API exactly as concise as it would be without the safety net.

## Delete many records

Reach for `delete_many` when the operation is deliberately bulk:

```rust
dinoco::delete_many::<Session>()
    .where_(|x| x.expires_at.lt(cutoff))
    .execute(&client)
    .await?;
```

> [!WARNING]
> `delete_many::<M>().execute(...)` with no `.where_(...)` at all deletes every row in the table, and it compiles just fine — this builder doesn't have `delete`'s type-state protection. That's deliberate, so a genuine full-table reset job can express itself directly, but it means every unfiltered `delete_many` call site deserves the same scrutiny in review as a raw `DELETE FROM table` would.

## Return deleted data

Both builders support a projection, returning a vector of whatever rows they actually deleted:

```rust
let deleted = dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

Without `.returning(...)`, execution returns `()` — skip requesting deleted data entirely when a count, or an application-level log entry, is all you actually need; there's no reason to pay for reconstructing rows you're about to discard.

## Relations and referential actions

What happens to related rows when you delete a parent comes entirely from the referential action declared in the schema and enforced by the migration's foreign key constraint — Dinoco's runtime doesn't add its own layer of behavior on top:

- `Cascade` removes dependent rows along with the parent.
- `Restrict` or `NoAction` can reject the delete outright while dependents exist.
- `SetNull` detaches optional related rows instead of removing them.
- `SetDefault` falls back dependents to the foreign key's declared default.

The runtime never silently overrides whichever of these the schema chose. If a delete fails because of `Restrict`, handle that error explicitly — or disconnect/reassign the dependent relation yourself before attempting the delete — rather than expecting Dinoco to pick a different behavior on your behalf.
