# Complete example

This chapter combines a complete schema, migrations, simple and complex queries, projections, writes, and many-to-many operations.

## Complete schema

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
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
    id         String  @id @default(uuid())
    account_id Integer @unique
    bio        String?
    account    Account @relation(fields: [account_id], references: [id], onDelete: Cascade)
}

model Project {
    id         String   @id @default(uuid())
    owner_id   Integer
    name       String
    archived   Boolean  @default(false)
    created_at DateTime @default(now())
    owner      Account  @relation(fields: [owner_id], references: [id], onDelete: Cascade)
    tasks      Task[]   @relation(fields: [id], references: [project_id])
}

model Task {
    id          Integer    @id @default(snowflake())
    project_id  String
    assignee_id Integer?
    title       String
    status      TaskStatus @default(pending)
    priority    Integer    @default(0)
    project     Project    @relation(fields: [project_id], references: [id], onDelete: Cascade)
    assignee    Account?   @relation(fields: [assignee_id], references: [id], onDelete: SetNull)
    tags        Tag[]
}

model Tag {
    id    String @id @default(uuid())
    name  String @unique
    tasks Task[]
}
```

Generated relation keys preserve their wrappers: `Project.owner_id` is `Snowflake`, `Task.project_id` is `Uuid`, and `Task.assignee_id` is `Option<Snowflake>`. Implicit many-to-many virtual keys preserve the opposite model's wrapper too.

## Generate the database and models

```bash
export DATABASE_URL="./dinoco.sqlite"
export SNOWFLAKE_NODE_ID="1"
dinoco migrate generate
```

Use `dinoco migrate run` in an environment that already has migration artifacts. Change `schema.dinoco`, not generated model files.

## Simple query

```rust
let account = dinoco::find_first::<Account>()
    .where_(|x| x.email.eq("ana@example.com"))
    .execute(&client)
    .await?
    .ok_or_else(|| anyhow::anyhow!("account not found"))?;
```

## Multiple filters

Repeated filters are combined with `AND`:

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

The task limit applies per project.

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
    .execute(&client)
    .await?;
```

Projection fields must exactly match generated model field types.

## Insert and update

```rust
let account = Account::new("ana@example.com".to_string(), "Ana".to_string());
dinoco::insert_into::<Account>().values(&account).execute(&client).await?;

let project = Project::new(account.id, "Dinoco 1.1.2".to_string());
dinoco::insert_into::<Project>().values(&project).execute(&client).await?;

let task = Task::new(project.id.clone(), "Document relations".to_string());
dinoco::insert_into::<Task>().values(&task).execute(&client).await?;

dinoco::update::<Task>()
    .where_(|x| x.id.eq(task.id))
    .update(|x| x.priority.set(10))
    .update(|x| x.assignee_id.set(Some(account.id)))
    .execute(&client)
    .await?;
```

## Complete many-to-many flow

Connect a new `Tag` to the existing task directly through the virtual `task_id` during insert:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

The same rule applies to each `insert_many` item:

```rust
let mut tags = vec![
    Tag::new("rust".to_string()),
    Tag::new("database".to_string()),
];

for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>().values(&tags).execute(&client).await?;
```

When both endpoints already exist, `connect` remains available. This creates another tag first and connects it afterward:

```rust
let review_tag = Tag::new("review".to_string());
dinoco::insert_into::<Tag>().values(&review_tag).execute(&client).await?;

dinoco::update::<Task>()
    .where_(|item| item.id.eq(task.id))
    .update(|item| item.tag_id.connect(&review_tag.id))
    .execute(&client)
    .await?;
```

Load every link through the relation field:

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|item| item.id.eq(task.id))
    .includes(|item| item.tags())
    .execute(&client)
    .await?;
```

Disconnecting removes only the selected pivot row:

```rust
dinoco::update::<Task>()
    .where_(|item| item.id.eq(task.id))
    .update(|item| item.tag_id.disconnect(&review_tag.id))
    .execute(&client)
    .await?;
```

Assigning `tag.task_id` during insert creates the `Tag` and its pivot row. `connect` inserts only the pivot row for existing endpoints, and `disconnect` removes only that row. Virtual fields remain `None` after reads.

## Production checklist

1. Keep database URLs in environment variables.
2. Review generated `up.sql` and `down.sql`.
3. Treat `None` separately from database errors.
4. Order paginated queries.
5. Limit list includes.
6. Use `.read_in_primary()` when a read must observe a recent write.
7. Populate generated virtual IDs during `insert_into`/`insert_many`, or use `connect`/`disconnect` for existing many-to-many endpoints.
8. Never use `Float` for a Snowflake foreign key.
