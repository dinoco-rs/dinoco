# Relations

A relation has two separate parts:

- the scalar key stored in SQL, such as `account_id`;
- the navigation field, such as `account`, `sessions`, or `systems`.

Scalar keys participate in inserts, filters, and constraints. Navigation fields start empty and are populated when the relation is loaded.

## UUID and Snowflake key types

UUID keys are declared as `String` in the schema and Snowflake keys as `Integer`. Dinoco v1.1.1 follows the referenced field and preserves its generated Rust wrapper:

```dinoco
model Account {
    id       Integer   @id @default(snowflake())
    sessions Session[] @relation(fields: [id], references: [account_id])
}

model Session {
    id         String  @id @default(uuid())
    account_id Integer
    account    Account @relation(fields: [account_id], references: [id])
}
```

The generated types are `Uuid` for `Session.id` and `Snowflake` for both `Account.id` and `Session.account_id`. Optional foreign keys become `Option<Uuid>` or `Option<Snowflake>`.

Never use `Float` for a Snowflake key. Snowflakes are integers.

## Relation anatomy

```dinoco
model Post {
    id        String @id @default(uuid())
    author_id String?
    author    User?  @relation(
        fields: [author_id],
        references: [id],
        onDelete: SetNull,
        onUpdate: Cascade
    )
}
```

`fields` contains local scalar fields. `references` contains target scalar fields. Both arrays must have the same size and compatible types. A nullable key requires an optional relation.

Every materialized foreign key automatically receives an index over its `fields` columns in the same order, including composite relations. An implicit many-to-many pivot table has an index for its composite primary key and one for each foreign key.

## One-to-many and many-to-one

```dinoco
model Account {
    id       Integer   @id @default(snowflake())
    email    String    @unique
    sessions Session[] @relation(fields: [id], references: [account_id])
}

model Session {
    id         String  @id @default(uuid())
    account_id Integer
    token      String  @unique
    account    Account @relation(
        fields: [account_id],
        references: [id],
        onDelete: Cascade
    )
}
```

Insert both rows:

```rust
let account = Account::new("ana@example.com".to_string());
dinoco::insert_into::<Account>().values(&account).execute(&client).await?;

let session = Session::new(account.id, "secure-token".to_string());
dinoco::insert_into::<Session>().values(&session).execute(&client).await?;
```

Load either direction:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|x| {
        x.sessions()
            .where_(|session| session.token.starts_with("secure-"))
            .order_by(|session| session.id.desc())
            .take(10)
    })
    .execute(&client)
    .await?;

let session = dinoco::find_first::<Session>()
    .where_(|x| x.token.eq("secure-token"))
    .includes(|x| x.account())
    .execute(&client)
    .await?;
```

The list-side limit is applied per parent.

## One-to-one

A one-to-one relation is backed by a unique foreign key:

```dinoco
model User {
    id      String   @id @default(uuid())
    email   String   @unique
    profile Profile?
}

model Profile {
    id      String @id @default(uuid())
    user_id String @unique
    bio     String
    user    User   @relation(fields: [user_id], references: [id], onDelete: Cascade)
}
```

Without `@unique`, multiple profiles could point to one user and the relation would be many-to-one.

For a composite one-to-one foreign key, declare uniqueness over the complete local tuple with `@@uniques([field_a, field_b])`. A composite `references` list may target the related model's `@@ids([...])` or a matching `@@uniques([...])` group.

## Implicit many-to-many

List fields on both sides without `fields` and `references` create an implicit pivot:

```dinoco
model Account {
    id      Integer   @id @default(snowflake())
    email   String    @unique
    systems Systems[]
}

model Systems {
    id          Integer   @id @default(snowflake())
    name        String
    group       String
    description String
    accounts    Account[]
}
```

Dinoco generates:

- SQL table `_account_to_systems`;
- composite key `(account_id, systems_id)`;
- Rust entity `AccountSystems`;
- `Snowflake` types for both pivot keys.

The pivot entity name concatenates model names in alphabetical order. `Post` and `Tag` generate `PostTag`.

### Connect many-to-many records

Insert both records before creating the link:

```rust
let account = Account::new("ana@example.com".to_string());
let system = Systems::new(
    "Backoffice".to_string(),
    "internal".to_string(),
    "Administrative system".to_string(),
);

dinoco::insert_into::<Account>().values(&account).execute(&client).await?;
dinoco::insert_into::<Systems>().values(&system).execute(&client).await?;
```

Then connect through the generated pivot entity:

```rust
dinoco::update::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .update(|pivot| pivot.systems_id.connect(system.id))
    .execute(&client)
    .await?;
```

Use `connect`, not `set`. Connecting does not insert either endpoint.

### Disconnect many-to-many records

```rust
dinoco::update::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .update(|pivot| pivot.systems_id.disconnect(system.id))
    .execute(&client)
    .await?;
```

Only the pivot row is removed.

### Connect multiple records

```rust
dinoco::update::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .update(|pivot| pivot.systems_id.connect(first_system.id))
    .update(|pivot| pivot.systems_id.connect(second_system.id))
    .execute(&client)
    .await?;

dinoco::update_many::<AccountSystems>()
    .where_(|pivot| pivot.account_id.batch([first_account.id, second_account.id]))
    .update(|pivot| pivot.systems_id.connect(system.id))
    .execute(&client)
    .await?;
```

The composite primary key prevents duplicate pairs.

### Query many-to-many records

Query the pivot, collect target IDs, and query the target model:

```rust
let links = dinoco::find_many::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .execute(&client)
    .await?;

let system_ids = links.iter().map(|link| link.systems_id).collect::<Vec<_>>();

let systems = dinoco::find_many::<Systems>()
    .where_(|system| system.id.batch(system_ids))
    .order_by(|system| system.name.asc())
    .execute(&client)
    .await?;
```

Delete every link for one account with `delete_many::<AccountSystems>()`; this does not delete systems.

### Common many-to-many mistakes

1. Connecting before both endpoints exist.
2. Updating `Account` instead of `AccountSystems`.
3. Calling `set` instead of `connect`.
4. Swapping `account_id` and `systems_id`.
5. Connecting the same pair twice.
6. Expecting a populated `account.systems` vector to insert pivot rows automatically.

## Many-to-many with extra fields

Use an explicit model when the link stores role, timestamps, permissions, or other data:

```dinoco
model Account {
    id            Integer               @id @default(snowflake())
    system_access AccountSystemAccess[] @relation(fields: [id], references: [account_id])
}

model Systems {
    id             Integer               @id @default(snowflake())
    account_access AccountSystemAccess[] @relation(fields: [id], references: [systems_id])
}

model AccountSystemAccess {
    account_id Integer
    systems_id Integer
    role       String
    created_at DateTime @default(now())

    account Account @relation(fields: [account_id], references: [id], onDelete: Cascade)
    system  Systems @relation(fields: [systems_id], references: [id], onDelete: Cascade)
}
```

Insert this entity normally. Its generated foreign-key fields are still `Snowflake`.

## Repeated relations

Use matching names for multiple paths between the same models:

```dinoco
model User {
    id             String @id @default(uuid())
    authored_posts Post[] @relation(name: "PostAuthor", fields: [id], references: [author_id])
    reviewed_posts Post[] @relation(name: "PostReviewer", fields: [id], references: [reviewer_id])
}

model Post {
    id          String @id @default(uuid())
    author_id   String
    reviewer_id String?
    author      User   @relation(name: "PostAuthor", fields: [author_id], references: [id])
    reviewer    User?  @relation(name: "PostReviewer", fields: [reviewer_id], references: [id])
}
```

## Self relations

```dinoco
model Employee {
    id         String     @id @default(uuid())
    manager_id String?
    manager    Employee?  @relation(name: "Management", fields: [manager_id], references: [id], onDelete: SetNull)
    reports    Employee[] @relation(name: "Management", fields: [id], references: [manager_id])
}
```

Use one explicit relation name and two distinct fields.

## Referential actions

| Action | Effect |
| --- | --- |
| `Cascade` | Propagates updates or deletes to dependent rows. |
| `Restrict` | Rejects the operation while dependents exist. |
| `NoAction` | Delegates enforcement timing to the database. |
| `SetNull` | Stores `NULL`; requires optional key and relation fields. |
| `SetDefault` | Applies the foreign key's declared default. |

## Relation checklist

1. Identify which model stores the foreign key.
2. Use schema `String` for UUID and `Integer` for Snowflake.
3. Keep foreign-key and relation optionality consistent.
4. Add `@unique` for one-to-one.
5. Map both one-to-many sides with `fields` and `references`.
6. Leave both lists unmapped only for implicit many-to-many.
7. Use the generated pivot entity for `connect` and `disconnect`.
8. Use an explicit pivot model when the link contains extra data.
9. Name repeated and self relations.
10. Review generated migration constraints before applying them.
