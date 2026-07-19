# insert_many

Used for batch insertion.

---

## What you can do

- `.values(Vec&lt;M&gt;)`: defines the records to be inserted in batch.
- `.with_relation(Vec&lt;R&gt;)`: inserts exactly one new related record for each parent.
- `.with_relations(Vec&lt;Vec\<R&gt;\>)` : inserts multiple new related records for each parent.
- `.with_connection(Vec&lt;R&gt;)`: connects exactly one existing relation for each inserted parent.
- `.with_connections(Vec&lt;Vec\<R&gt;\>)` : connects multiple existing relations for each inserted parent, typically in many-to-many scenarios.
- `.returning::&lt;T&gt;()`: changes the return to a typed list of the inserted items.
- `.execute(&client)`: executes the batch operation.

## Return

Without `.returning::&lt;T&gt;()`, the return is:

```rust
DinocoResult<()>
```

With `.returning::&lt;T&gt;()`, the return becomes:

```rust
DinocoResult<Vec<T>>
```

## Simple example

```rust
dinoco::insert_many::<User>()
    .values(vec![
        User { id: "u1".into(), email: "a@acme.com".into(), name: "A".into() },
        User { id: "u2".into(), email: "b@acme.com".into(), name: "B".into() },
    ])
    .execute(&client)
    .await?;
```

## Example with return

```rust
let created = dinoco::insert_many::<User>()
    .values(vec![
        User { id: 2, name: "Ana".to_string() },
        User { id: 3, name: "Caio".to_string() },
    ])
    .returning::<User>()
    .execute(&client)
    .await?;
```

## Example with with_relation(...)

Use `with_relation(...)` when each parent needs to be created and connected.

```rust
dinoco::insert_many::<Post>()
    .values(vec![
        Team { id: "team-11".into(), name: "Data".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_relation(vec![
        Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() },
        Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
    ])
    .execute(&client)
    .await?;
```

## Example with with_relations(...)

Use with_relations(...) when each parent record has more than one child; the records will be automatically created and linked.

```rust
dinoco::insert_many::<Post>()
    .values(vec![
        Team { id: "team-11".into(), name: "Data".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_relation(vec![
        vec![
			Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() },
        	Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
		],

		vec![Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() }]
    ])
    .execute(&client)
    .await?;
```

## Example with with_connection(...)

Use `with_connection(...)` when each parent needs to be connected to exactly one existing record.

```rust
dinoco::insert_many::<Team>()
    .values(vec![
        Team { id: "team-11".into(), name: "Data".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_connection(vec![
        Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() },
        Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
    ])
    .execute(&client)
    .await?;
```

## Example with with_connections(...)

Use `with_connections(...)` when each parent needs to be connected to multiple existing records.
This is a common use case in many-to-many relationships.

```rust
dinoco::insert_many::<Article>()
    .values(vec![
        Article { id: "article-11".into(), title: "Connect Multiple".into() },
        Article { id: "article-12".into(), title: "Connect Batch".into() },
    ])
    .with_connections(vec![
        vec![
            Label { id: "label-11".into(), name: "orm".into() },
            Label { id: "label-12".into(), name: "rust".into() },
        ],
        vec![
            Label { id: "label-10".into(), name: "backend".into() },
        ],
    ])
    .execute(&client)
    .await?;
```

## Notes

- `with_relation(...)` and `with_relations(...)` work best when the parent already has a known ID in memory.
- `with_connection(...)` and `with_connections(...)` are geared towards connecting existing relations.
- `with_connection(...)` requires the same number of items as `values(...)`.
- `with_connections(...)` requires the same number of groups relative to the main vector.

## Next steps

- [**`insert_into::&lt;M&gt;()`**](/v0.0.1/orm/insert-into): single insertion.
- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): update with filter.
