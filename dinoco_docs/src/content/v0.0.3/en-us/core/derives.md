# Derives

Overview of core derives and attributes used in Dinoco.

---

## What are derives in Dinoco

Dinoco derives help link your Rust structs to ORM behavior without needing to write the entire implementation manually.

The most common ones in daily use are:

- `#[derive(Rowable)]`
- `#[derive(Extend)]`
- `#[insertable]`

## Rowable Derive

Use `Rowable` when the struct directly represents a model that will be read and written by Dinoco.

```rust
#[derive(Debug, Clone, dinoco::Rowable)]
struct User {
    id: String,
    email: String,
    name: String,
}
```

This derive handles the serialization of the row returned by the adapter to the Rust struct.

## Extend Derive

Use `Extend` when you want a different projection from the base model, typically for `select`, `include`, `count`, or enriched payloads.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: String,
    name: String,
}
```

In this case, the struct remains linked to the `User` model but can expose only the fields necessary for that flow.

## #[insertable] Attribute

Use `#[insertable]` along with `Extend` when `.values(...)` needs to accept new relationships or existing connections within the payload itself.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}
```

With this, Dinoco inserts the parent and then automatically processes the nested items.

## Example with Rowable

```rust
#[derive(Debug, Clone, dinoco::Rowable)]
struct Team {
    id: String,
    name: String,
}

dinoco::insert_into::<Team>()
    .values(Team {
        id: "team-1".into(),
        name: "Platform".into(),
    })
    .execute(&client)
    .await?;
```

## Example with Extend

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPostsCount {
    id: String,
    name: String,
    posts_count: usize,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPostsCount>()
    .count(|user| user.posts())
    .execute(&client)
    .await?;
```

## Example with #[insertable]

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
            labels: vec![ArticleConnection::Label("label-10".into())],
        },
    ])
    .execute(&client)
    .await?;
```

## When to use each one

- Use `Rowable` for the main generated model or structs compatible with direct row reading.
- Use `Extend` for projections, counts, includes, and specialized payloads.
- Use `#[insertable]` when `Extend` will also serve as a write payload with nesting.

## Next steps

- [**Traits**](/v0.0.2/core/traits): see the traits implemented by the model.
- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into): single insertion with rich payloads.
- [**`insert_many::&lt;M&gt;()`**](/v0.0.2/orm/insert-many): batch insertion with nested relationships and connections.
