# Relations

A relation has two separate parts:

- the scalar key stored in SQL, such as `account_id`;
- the navigation field, such as `account`, `sessions`, or `systems`.

Scalar keys participate in inserts, filters, and constraints. Navigation fields start empty and are populated when the relation is loaded. Every singular navigation field must therefore use `?`, independently of whether its local foreign key is required or nullable.

## UUID and Snowflake key types

UUID keys are declared as `String` in the schema and Snowflake keys as `Integer`. Dinoco v1.3.2 follows the referenced field and preserves its generated Rust wrapper:

```dinoco
model Account {
    id       Integer   @id @default(snowflake())
    sessions Session[] @relation(fields: [id], references: [account_id])
}

model Session {
    id         String  @id @default(uuid())
    account_id Integer
    account    Account? @relation(fields: [account_id], references: [id])
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

`fields` contains local scalar fields. `references` contains target scalar fields. Both arrays must have the same size and compatible types. Singular relation fields are always optional: both `author_id String` with `author User?` and `author_id String?` with `author User?` are valid. The key controls the database constraint; the relation field represents whether navigation data was loaded.

`SetNull` is the exception for referential actions: every local foreign-key field must be nullable because the database needs to store `NULL` when the referenced row changes or is deleted.

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
    account    Account? @relation(
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
    user    User?   @relation(fields: [user_id], references: [id], onDelete: Cascade)
}
```

Without `@unique`, multiple profiles could point to one user and the relation would be many-to-one.

For a composite one-to-one foreign key, declare uniqueness over the complete local tuple with `@@uniques([field_a, field_b])`. A composite `references` list may target the related model's `@@ids([...])` or a matching `@@uniques([...])` group.

## Implicit many-to-many

List fields on both sides without `fields` and `references` define an implicit many-to-many relation:

```dinoco
model Business {
    id      Integer  @id @default(snowflake())
    name    String
    systems System[]
}

model System {
    id           Integer    @id @default(snowflake())
    name         String
    description  String
    business     Business[]
}
```

### What Dinoco generates

The database still has a real pivot table:

```text
_business_to_system
├── business_id  -> business.id
└── system_id    -> system.id
```

`(business_id, system_id)` is its composite primary key. Both columns receive foreign keys and indexes through the migration planner.

The generated Rust endpoints conceptually look like this:

```rust
pub struct Business {
    pub id: dinoco::Snowflake,
    pub name: String,
    pub systems: Vec<System>,

    // Virtual write key; it is not a column of `business`.
    pub system_id: Option<dinoco::Snowflake>,
}

pub struct System {
    pub id: dinoco::Snowflake,
    pub name: String,
    pub description: String,
    pub business: Vec<Business>,

    // Virtual write key; it is not a column of `system`.
    pub business_id: Option<dinoco::Snowflake>,
}
```

Dinoco does not generate a `BusinessSystem` Rust entity for an implicit pivot. `system_id` and `business_id` are virtual `Option<Id>` fields with two strict rules:

- they are accepted as write inputs for the pivot;
- database reads always initialize them as `None` and never select them from the endpoint tables.

The navigation fields (`systems` and `business`) are the read side of the relation. Their generated accessor names follow the schema exactly, so this example uses `business()` even though the field is a list. Load them with `includes`; do not inspect the virtual ID to read a link.

### Load either direction

The include loader joins through `_business_to_system`; it never looks for a nonexistent `business_id` column on `system`:

```rust
let business = dinoco::find_first::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .includes(|business| {
        business.systems()
            .where_(|system| system.name.starts_with("Back"))
            .order_by(|system| system.name.asc())
            .take(20)
            .includes(|system| system.business())
    })
    .execute(&client)
    .await?;
```

Filters and ordering apply to the related model. `take` and `skip` are applied per parent, and nested includes can cross the same pivot in the opposite direction.

The reverse direction uses the other navigation field:

```rust
let systems = dinoco::find_many::<System>()
    .includes(|system| system.business())
    .execute(&client)
    .await?;
```

### Count related records

Relation counts also traverse the pivot and accept filters on the related model:

```rust
let result = dinoco::count::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .includes(|business| {
        business.systems().where_(|system| system.name.starts_with("Back"))
    })
    .execute(&client)
    .await?;

assert_eq!(result.systems, Some(1));
```

### Connect existing endpoints

Insert both endpoints first, then call `connect` on the virtual target ID:

```rust
let business = Business::new("Dinoco".to_string());
let system = System::new(
    "Backoffice".to_string(),
    "Administrative system".to_string(),
);

dinoco::insert_into::<Business>().values(&business).execute(&client).await?;
dinoco::insert_into::<System>().values(&system).execute(&client).await?;

let business_id = business.id;
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system.id))
    .execute(&client)
    .await?;
```

This inserts `(business.id, system.id)` into `_business_to_system`. The returned `business.system_id` is still `None`, because the field is write-only. Connecting does not insert either endpoint and does not update a column on `business` or `system`.

The reverse API creates the same pair:

```rust
dinoco::update::<System>()
    .where_(|system| system.id.eq(&system_id))
    .update(|system| system.business_id.connect(&business_id))
    .execute(&client)
    .await?;
```

### Disconnect endpoints

Use the same virtual field with `disconnect`:

```rust
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.disconnect(&system_id))
    .execute(&client)
    .await?;
```

Only the matching pivot row is deleted. Both endpoints remain untouched.

### Supported update builders

Virtual many-to-many keys support `connect` and `disconnect` in:

- `update::<M>()`;
- `update_many::<M>()`;
- `find_and_update::<M>()`;
- `update` and `update_many` with `.returning::<S>()`.

The endpoint filter can use regular fields, not only its primary key. Dinoco resolves every matching endpoint ID on the primary connection before changing the pivot. For example, one system can be connected to several businesses:

```rust
dinoco::update_many::<Business>()
    .where_(|business| business.name.starts_with("Dinoco"))
    .update(|business| business.system_id.connect(&system_id))
    .execute(&client)
    .await?;
```

These relation operations also accept the closure transaction context. Execute them with `.execute(tx)` so the pivot change and the builder's scalar update use the same physical transaction connection and a later failure rolls both back.

### Connect during insert

Set one virtual ID before inserting an endpoint. `insert_into` inserts the endpoint without treating the virtual field as a physical column, then creates the pivot row:

```rust
let mut system = System::new(
    "ERP".to_string(),
    "Enterprise resource planning".to_string(),
);
system.business_id = Some(business.id);

dinoco::insert_into::<System>()
    .values(&system)
    .execute(&client)
    .await?;
```

`insert_many` applies the same rule independently to every payload:

```rust
let mut systems = vec![
    System::new("CRM".to_string(), "Customer management".to_string()),
    System::new("BI".to_string(), "Business intelligence".to_string()),
];

for system in &mut systems {
    system.business_id = Some(business.id);
}

dinoco::insert_many::<System>()
    .values(&systems)
    .execute(&client)
    .await?;
```

This also works when `insert_into` or `insert_many` uses `.returning::<S>()`; returned virtual keys remain `None`. One virtual field stores one target ID. To connect several targets, insert the endpoint and add repeated `connect` updates. Assigning values to `business.systems` does not create implicit pivot rows.

`insert_into` and `insert_many` with populated virtual IDs can also execute with `.execute(tx)` inside the closure transaction API, making their endpoint and pivot rows atomic. See [Transactions](/v1.3.2/orm/transactions#supported-builders) for the supported mutation flow.

### Duplicate links and missing endpoints

The composite primary key rejects a duplicate pair. A repeated `connect` can therefore return a database constraint error. Foreign keys also reject links to endpoints that do not exist.

### Upgrading generated models from v1.1.1

Run model generation again after upgrading. Dinoco removes the old generated pivot file and stops exporting `BusinessSystem`. Replace code that updated the pivot entity:

```rust
// Before
dinoco::update::<BusinessSystem>()
    .where_(|pivot| pivot.business_id.eq(&business_id))
    .update(|pivot| pivot.system_id.connect(&system_id));

// v1.2.0
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system_id));
```

The SQL pivot and its migration history remain in place; only the public generated Rust API changes.

### Common mistakes

1. Using `set` instead of `connect` or `disconnect` on a virtual key.
2. Expecting `business.system_id` to be populated after a read.
3. Connecting before both endpoints exist.
4. Connecting the same pair twice.
5. Expecting a populated `business.systems` vector to create pivot rows.

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

    account Account? @relation(fields: [account_id], references: [id], onDelete: Cascade)
    system  Systems? @relation(fields: [systems_id], references: [id], onDelete: Cascade)
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
    author      User?   @relation(name: "PostAuthor", fields: [author_id], references: [id])
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
7. Populate the generated virtual ID during `insert_into`/`insert_many`, or use it with `connect`/`disconnect` for existing endpoints.
8. Use an explicit pivot model when the link contains extra data.
9. Name repeated and self relations.
10. Review generated migration constraints before applying them.
