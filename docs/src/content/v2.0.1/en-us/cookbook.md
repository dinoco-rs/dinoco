# Complete example

Where the [quickstart](/en-us/docs/orm/guide/quickstart) keeps things minimal on purpose, this page goes the other way: one realistic schema with several relation shapes, and every kind of query and write you're likely to reach for in a real application. Treat it as a reference you can copy from, not a tutorial you need to read top to bottom.

## Complete schema

```dinoco
config {
    database          = "sqlite"
    database_url      = env("DATABASE_URL")
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

enum TaskStatus {
    pending
    doing
    done
}

model Account {
    id             Integer   @id @default(snowflake())
    email          String    @unique
    name           String
    active         Boolean   @default(true)
    profile        Profile?
    owned_projects Project[] @relation(fields: [id], references: [owner_id])
    assigned_tasks Task[]    @relation(fields: [id], references: [assignee_id])
}

model Profile {
    id         String   @id @default(uuid())
    account_id Integer  @unique
    bio        String?
    account    Account? @relation(fields: [account_id], references: [id], onDelete: Cascade)
}

model Project {
    id         String   @id @default(uuid())
    owner_id   Integer
    name       String
    archived   Boolean  @default(false)
    created_at DateTime @default(now())
    owner      Account? @relation(fields: [owner_id], references: [id], onDelete: Cascade)
    tasks      Task[]   @relation(fields: [id], references: [project_id])
}

model Task {
    id          Integer    @id @default(snowflake())
    project_id  String
    assignee_id Integer?
    title       String
    status      TaskStatus @default(pending)
    priority    Integer    @default(0)
    project     Project?   @relation(fields: [project_id], references: [id], onDelete: Cascade)
    assignee    Account?   @relation(fields: [assignee_id], references: [id], onDelete: SetNull)
    tags        Tag[]
}

model Tag {
    id    String @id @default(uuid())
    name  String @unique
    tasks Task[]
}
```

This one schema exercises every relation shape Dinoco supports: `Account` ↔ `Profile` is one-to-one, `Account` ↔ `Project`/`Task` is one-to-many, and `Task` ↔ `Tag` is an **implicit** many-to-many (neither side declares `fields`/`references` — Dinoco generates and manages the pivot table for you). It's worth tracing the generated key types before moving on:

- `Account.id`, `Project.owner_id`, and `Task.assignee_id` are `Snowflake`.
- `Profile.id`, `Project.id`, `Task.project_id`, and `Tag.id` are `Uuid`.
- `Task.assignee_id` specifically is `Option<Snowflake>` (the relation is optional).
- The generated many-to-many virtual keys mirror whichever wrapper the *opposite* model's `@id` uses.

## Generate the database and models

```bash
export DATABASE_URL="./dinoco.sqlite"
export SNOWFLAKE_NODE_ID="1"

dinoco migrate generate
```

On another machine that already has migration files on disk — CI, a teammate's laptop — apply them without planning a new one:

```bash
dinoco migrate run
dinoco models generate
```

> [!WARNING]
> Never hand-edit files under `dinoco/models/`. They're regenerated from the schema on every `migrate generate`/`migrate run`/`models generate`, so any manual edit is silently discarded. If the generated code needs to be different, change `schema.dinoco` instead.

## Simple query

```rust
let account = dinoco::find_first::<Account>()
    .where_(|x| x.email.eq("ana@example.com"))
    .execute(&client)
    .await?;

let Some(account) = account else {
    anyhow::bail!("account not found");
};
```

`find_first` returns `Option<Account>` — a missing row is not a database error, so match on it explicitly rather than unwrapping.

## Query with multiple filters

Every `.where_(...)` call adds another condition, combined with `AND`:

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|x| x.assignee_id.eq(account.id))
    .where_(|x| x.priority.gte(5))
    .where_(|x| x.title.like("migration"))
    .order_by(|x| x.priority.desc())
    .take(20)
    .skip(0)
    .read_in_primary()
    .execute(&client)
    .await?;
```

`.read_in_primary()` opts this specific read out of read-replica routing — reach for it right after a write, when you need the result to reflect that write immediately. See [filters](/en-us/docs/orm/orm/filters) if you need `OR`/`NOT` instead of implicit `AND`.

## Complex query with includes

```rust
let projects = dinoco::find_many::<Project>()
    .where_(|project| project.archived.eq(false))
    .where_(|project| project.owner_id.eq(account.id))
    .order_by(|project| project.created_at.desc())
    .includes(|project| project.owner())
    .includes(|project| {
        project
            .tasks()
            .where_(|task| task.priority.gte(5))
            .order_by(|task| task.priority.desc())
            .take(10)
            .includes(|task| task.assignee())
    })
    .execute(&client)
    .await?;
```

The `.take(10)` limit applies **per project**, not to the overall result — each returned project gets up to ten of its own highest-priority tasks. Sibling includes (`owner()` alongside `tasks()`) are loaded as separate batched queries, never as one query per row.

## Custom projection

```rust
use dinoco::EntityExtend;

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountSummary {
    pub id: dinoco::Snowflake,
    pub email: String,
    pub name: String,
}

let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .where_(|x| x.active.eq(true))
    .order_by(|x| x.name.asc())
    .execute(&client)
    .await?;
```

> [!WARNING]
> Every field on a projection struct must match the generated model's field **type** exactly — `id: dinoco::Snowflake`, not `i64` or `String`, because that's what `Account.id` actually generates as. A mismatch is a compile error, not a runtime surprise, but it's the most common first mistake when writing a `#[derive(EntityExtend)]` struct by hand.

## Insert related records

```rust
let account = Account::new("ana@example.com".to_string(), "Ana".to_string());
dinoco::insert_into::<Account>().values(&account).execute(&client).await?;

let project = Project::new(account.id, "Dinoco 2.0.1".to_string());
dinoco::insert_into::<Project>().values(&project).execute(&client).await?;

let task = Task::new(project.id.clone(), "Document relations".to_string());
dinoco::insert_into::<Task>().values(&task).execute(&client).await?;
```

`new`'s arguments are exactly the model's required fields without a default, in the order they're declared in the schema — `Project::new` takes `owner_id` and `name` because `id`, `archived`, and `created_at` all have defaults.

## Update and count

```rust
dinoco::update::<Task>()
    .where_(|x| x.id.eq(task.id))
    .update(|x| x.priority.set(10))
    .update(|x| x.assignee_id.set(Some(account.id)))
    .execute(&client)
    .await?;

let count = dinoco::count::<Task>()
    .where_(|x| x.assignee_id.eq(account.id))
    .execute(&client)
    .await?;
```

## Complete many-to-many flow

`Task` ↔ `Tag` is implicit, so Dinoco generates a hidden pivot table and a virtual `task_id`/`tag_id` field on each side you can assign directly. Connect a brand-new `Tag` to the existing task during insert:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

The same works for every item in a batch insert:

```rust
let mut tags = vec![Tag::new("rust".to_string()), Tag::new("database".to_string())];

for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>().values(&tags).execute(&client).await?;
```

When both endpoints already exist, use `connect` instead of touching the virtual field — this creates one more tag, then links it to the task separately:

```rust
let review_tag = Tag::new("review".to_string());
dinoco::insert_into::<Tag>().values(&review_tag).execute(&client).await?;

dinoco::update::<Task>()
    .where_(|item| item.id.eq(task.id))
    .update(|item| item.tag_id.connect(&review_tag.id))
    .execute(&client)
    .await?;
```

Load every linked tag through the relation field, exactly like any other include:

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|item| item.id.eq(task.id))
    .includes(|item| item.tags())
    .execute(&client)
    .await?;
```

`disconnect` removes only the selected pivot row — it never deletes the `Tag` itself:

```rust
dinoco::update::<Task>()
    .where_(|item| item.id.eq(task.id))
    .update(|item| item.tag_id.disconnect(&review_tag.id))
    .execute(&client)
    .await?;
```

> [!NOTE]
> Three different operations, three different effects: assigning the virtual field (`tag.task_id = Some(...)`) during insert **creates** both the `Tag` and its pivot row in one statement. `connect` creates **only** the pivot row, for two endpoints that already exist. `disconnect` removes **only** the pivot row — the tag and the task are both untouched. Virtual fields like `task_id` always read back as `None`; they exist to be written, not to be read.

## Production checklist

1. Keep every connection URL in an environment variable — never a literal in `schema.dinoco`.
2. Read the generated `up.sql`/`down.sql` before running a migration against a real database.
3. Handle `Option`/`None` from `find_first` as a normal outcome, distinct from an `Err`.
4. Always pair `.take(...)` with an explicit `.order_by(...)` on paginated queries.
5. Bound list includes with `.take(...)` — an unbounded `.includes(...)` on a large relation is still one batched query, but an unbounded result set.
6. Reach for `.read_in_primary()` when a read must observe a write that just happened on the primary.
7. Populate generated virtual many-to-many IDs during `insert_into`/`insert_many` for new endpoints, or use `connect`/`disconnect` for endpoints that already exist.
8. Never use `Float` for a field that's a foreign key to a `Snowflake` id — floating-point equality doesn't behave the way a join needs it to.
