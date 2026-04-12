# insert_many

Used for batch insertion.

---

## What you can do

- `.values(Vec&lt;M&gt;)`: defines the records to be inserted in batch.
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

## Example with worker

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-created", |job| async move {
        // Users created in batch:
        println!("Users created in batch: {}", job.data.len());
        job.success();
    })
    .run()
    .await?;

dinoco::insert_many::<User>()
    .values(vec![
        User { id: 2, name: "Ana".to_string() },
        User { id: 3, name: "Caio".to_string() },
    ])
    .enqueue("user.batch-created")
    .execute(&client)
    .await?;
```

Learn more about workers in [**`queues`**](/v0.0.2/orm/queues).

## Example with nested relations

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Team)]
#[insertable]
struct TeamWithMembers {
    id: String,
    name: String,
    members: Vec<Member>,
}

dinoco::insert_many::<Team>()
    .values(vec![
        TeamWithMembers {
            id: "team-11".into(),
            name: "Data".into(),
            members: vec![
                Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() },
                Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
            ],
        },
        TeamWithMembers {
            id: "team-12".into(),
            name: "DX".into(),
            members: vec![
                Member { id: "member-13".into(), name: "Caio".into(), teamId: "legacy".into() },
            ],
        },
    ])
    .execute(&client)
    .await?;
```

## Example with nested connections

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}

dinoco::insert_many::<Article>()
    .values(vec![
        ArticleWithLabels {
            id: "article-11".into(),
            title: "Connect Multiple".into(),
            labels: vec![
                ArticleConnection::Label("label-11".into()),
                ArticleConnection::Label("label-12".into()),
            ],
        },
        ArticleWithLabels {
            id: "article-12".into(),
            title: "Connect Batch".into(),
            labels: vec![
                ArticleConnection::Label("label-10".into()),
            ],
        },
    ])
    .execute(&client)
    .await?;
```

## Example with ArticleConnection

When related items already exist, use the codegen-generated enum within the payload.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}

let values = vec![
    ArticleWithLabels {
        id: "article-21".into(),
        title: "Connect Multiple".into(),
        labels: vec![
            ArticleConnection::Label("label-11".into()),
            ArticleConnection::Label("label-12".into()),
        ],
    },
    ArticleWithLabels {
        id: "article-22".into(),
        title: "Connect Batch".into(),
        labels: vec![ArticleConnection::Label("label-10".into())],
    },
];

dinoco::insert_many::<Article>()
    .values(values)
    .execute(&client)
    .await?;
```

## Example with Extend #[insertable]

`insert_many` can also receive richer payloads via `.values(...)` when the struct is marked with `#[insertable]`. This is useful for creating parents and their nested relations in a single recursive flow.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<Label>,
}

dinoco::insert_many::<Article>()
    .values(vec![
        ArticleWithLabels {
            id: "article-11".into(),
            title: "Connect Multiple".into(),
            labels: vec![
                Label { id: "label-11".into(), name: "orm".into() },
                Label { id: "label-12".into(), name: "rust".into() },
            ],
        },
        ArticleWithLabels {
            id: "article-12".into(),
            title: "Connect Batch".into(),
            labels: vec![
                Label { id: "label-10".into(), name: "backend".into() },
            ],
        },
    ])
    .execute(&client)
    .await?;
```

## Notes

- `#[insertable]` on an `Extend` struct allows `.values(...)` to recursively insert new relations.
- For connections with existing records, use the codegen-generated `ModelConnection` enum.
- New relations can use `Vec&lt;ModelRelacionado&gt;` or other `Extend` payloads marked with `#[insertable]`.
- Existing connections work well in many-to-many scenarios and also in flows supported by the generated model.

## Next steps

- [**Derives**](/v0.0.2/core/derives): understand how to build payloads with `Extend` and `#[insertable]`.
- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into): single insertion.
- [**`update::&lt;M&gt;()`**](/v0.0.2/orm/update): update with filter.
