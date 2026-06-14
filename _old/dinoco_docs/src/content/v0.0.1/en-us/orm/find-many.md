# find_many

Used to fetch a list of records.

---

## What you can do

- Select a projection with `.select::&lt;T&gt;()`
- Filter with `.cond(...)`
- Limit with `.take(...)`
- Paginate with `.skip(...)`
- Order with `.order_by(...)`
- Load relations with `.includes(...)`
- Count relations with `.count(...)`
- Force read from the primary database with `.read_in_primary()`
- Execute with `.execute(&client)`

## Method descriptions

- `.select::&lt;T&gt;()`: replaces the model's default projection with a custom projection.
- `.cond(...)`: adds filter conditions to the query.
- `.take(...)`: limits the quantity of records returned.
- `.skip(...)`: skips a quantity of records before returning the result.
- `.order_by(...)`: defines the query's ordering.
- `.includes(...)`: loads relations along with the main records.
- `.count(...)`: calculates relation counters and populates fields like `posts_count`.
- `.read_in_primary()`: forces reading from the primary database, without using a replica.
- `.execute(&client)`: executes the query against the database.

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

## Relation count example

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

## Example reading from primary database

```rust
let users = dinoco::find_many::<User>()
    .read_in_primary()
    .take(5)
    .execute(&client)
    .await?;
```

## Next steps

- [**`find_first::&lt;M&gt;()`**](/v0.0.1/orm/find-first): version to fetch at most one record.
- [**`count::&lt;M&gt;()`**](/v0.0.1/orm/count): record count with filter.
