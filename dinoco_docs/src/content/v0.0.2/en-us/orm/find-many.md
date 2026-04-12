# find_many

Used to fetch a list of records.

---

## What you can do

- `.select::&lt;T&gt;()`: swaps the model's default projection for a custom projection.
- `.cond(...)`: adds filter conditions to the query.
- `.take(...)`: limits the number of records returned.
- `.skip(...)`: skips a number of records before returning the result.
- `.order_by(...)`: defines the query's ordering.
- `.includes(...)`: loads relationships along with the main records.
- `.count(...)`: calculates relationship counters and populates fields like `posts_count`.
- `.cache(...)`: first queries Redis using the provided key; on cache miss, executes the query, saves, and returns the result. On cache hit, the query logger logs `CACHE HIT key=...`.
- `.cache_with_expiration(...)`: same behavior as standard cache, but saves with TTL in seconds.
- `.read_in_primary()`: forces reading from the primary database, without using a replica.
- `.execute(&client)`: executes the query in the database.

## Return

Without `select::&lt;T&gt;()`, the return is:

```rust
DinocoResult<Vec<M>>
```

With `select::&lt;T&gt;()`, the return becomes:

```rust
DinocoResult<Vec<T>>
```

## Basic example

```rust
let users = dinoco::find_many::<User>()
    .execute(&client)
    .await?;
```

## Example with filter

```rust
let users = dinoco::find_many::<User>()
    .cond(|w| w.email.eq("ana@acme.com"))
    .execute(&client)
    .await?;
```

## Example with pagination and ordering

```rust
let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .skip(20)
    .take(10)
    .execute(&client)
    .await?;
```

## Custom select example

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserListItem {
    id: i64,
    name: String,
}

let users = dinoco::find_many::<User>()
    .select::<UserListItem>()
    .execute(&client)
    .await?;
```

## Simple include example

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPosts>()
    .includes(|i| i.posts())
    .execute(&client)
    .await?;
```

## Filtered include example

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPublishedPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPublishedPosts>()
    .includes(|i| i.posts().cond(|w| w.published.eq(true)))
    .execute(&client)
    .await?;
```

## Nested include example

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Comment)]
struct CommentListItem {
    id: i64,
    text: String,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Post)]
struct PostWithComments {
    id: i64,
    title: String,
    comments: Vec<CommentListItem>,
    comments_count: usize,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<PostWithComments>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPosts>()
    .includes(|i| {
        i.posts()
            .includes(|post| post.comments().take(3))
            .count(|post| post.comments())
    })
    .execute(&client)
    .await?;
```

## Relationship count example

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPostsCount {
    id: i64,
    name: String,
    posts_count: usize,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPostsCount>()
    .count(|i| i.posts())
    .execute(&client)
    .await?;
```

## Example reading from the primary database

```rust
let users = dinoco::find_many::<User>()
    .read_in_primary()
    .take(5)
    .execute(&client)
    .await?;
```

## Worker example

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-read", |job| async move {
        println!("Batch read with {} users", job.data.len());
        job.success();
    })
    .run()
    .await?;

let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .take(20)
    .enqueue("user.batch-read")
    .execute(&client)
    .await?;
```

See more about workers in [**`queues`**](/v0.0.2/orm/queues).

## Cache example

This method is only generated when the schema's `config {}` has `redis`.

```rust
use database::*;

let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .cache("users:list")
    .execute(&client)
    .await?;
```

## Example with cache and expiration

```rust
use database::*;

let users = dinoco::find_many::<User>()
    .take(20)
    .cache_with_expiration("users:top-20", 60)
    .execute(&client)
    .await?;
```

## Next steps

- [**`find_first::&lt;M&gt;()`**](/v0.0.2/orm/find-first): version for fetching at most one record.
- [**`count::&lt;M&gt;()`**](/v0.0.2/orm/count): record count with filter.
