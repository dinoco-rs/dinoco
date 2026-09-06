# Relations

Every relation in Dinoco is really two separate things wearing one name:

- **The scalar key** stored in SQL — `account_id`, a plain `Integer` or `String` column that participates in inserts, filters, and database constraints exactly like any other field.
- **The navigation field** — `account`, `sessions`, `systems` — which starts out empty and is populated only when you explicitly ask for it via `.includes(...)`.

> [!NOTE]
> Because navigation fields are unloaded by default, every *singular* navigation field must be written with `?` — `account Account?` — regardless of whether its local foreign key (`account_id`) is itself required or nullable. The `?` on the navigation field describes "is this loaded," not "is this relationship optional."

## UUID and Snowflake key types

Declare a UUID-backed key as `String` in the schema and a Snowflake-backed key as `Integer`. Dinoco follows the field a relation actually references and reuses its generated Rust wrapper on the other side, so the two stay in sync automatically:

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

Here, `Session.id` generates as `Uuid`, and both `Account.id` and `Session.account_id` generate as `Snowflake` — an optional foreign key would generate `Option<Uuid>` or `Option<Snowflake>` the same way.

> [!DANGER]
> Never use `Float` for a Snowflake-backed key or foreign key. Snowflakes are integers, and floating-point equality doesn't behave the way a join or a uniqueness check needs it to.

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

`fields` lists the local scalar columns; `references` lists the target model's columns they point at. Both arrays must be the same length, with pairwise-compatible types. The navigation field's optionality and the foreign key's optionality are two independent decisions — `author_id String` with `author User?`, and `author_id String?` with `author User?`, are both valid schemas. The key controls what the database enforces; the navigation field only controls whether related data has been loaded into memory.

`SetNull` comes with one hard requirement: every local foreign-key field it applies to must be nullable, because the database needs somewhere to put `NULL` when the referenced row changes or disappears.

Every materialized foreign key gets an automatic index over its `fields`, in declaration order — composite relations included. See [Indexes and constraints](/en-us/docs/orm/guide/indexes#foreign-keys-are-indexed) for the full detail, including what an implicit many-to-many pivot gets indexed on.

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

Insert both rows independently — the foreign key is just a plain scalar value at insert time:

```rust
let account = Account::new("ana@example.com".to_string());
dinoco::insert_into::<Account>().values(&account).execute(&client).await?;

let session = Session::new(account.id, "secure-token".to_string());
dinoco::insert_into::<Session>().values(&session).execute(&client).await?;
```

Load the relation from either side:

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

`.take(10)` on the list side applies **per parent account**, not to the overall result set.

## One-to-one

A one-to-one relation is really a one-to-many relation with a `@unique` constraint on the foreign key — that constraint is what makes "many" impossible:

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

> [!WARNING]
> Drop the `@unique` on `user_id` and this silently becomes a many-to-one relation instead — multiple profiles could then point at the same user, and Dinoco would have no way to know that wasn't intended.

For a composite one-to-one foreign key, declare uniqueness across the whole local tuple with `@@uniques([field_a, field_b])`. A composite `references` list can target either the related model's `@@ids([...])` or a matching `@@uniques([...])` group.

## Implicit many-to-many

Two list fields, on both sides, with no `fields`/`references` on either — that's all it takes to declare an implicit many-to-many relation:

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

Under the hood there's a real SQL pivot table:

```text
_business_to_system
├── business_id  -> business.id
└── system_id    -> system.id
```

`(business_id, system_id)` is its composite primary key, and both columns get their own foreign key and index through the migration planner — all of this is managed for you, but it's real SQL you could inspect directly if you needed to.

On the Rust side, each endpoint conceptually looks like this:

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

There's no public `BusinessSystem` Rust entity for an implicit pivot — instead, `system_id` and `business_id` are virtual `Option<Id>` fields governed by these rules:

- They're accepted as **write** inputs, to create or target a pivot row.
- They're accepted as **filter** inputs in `where_(...)` — a virtual ID compiles into a membership subquery over the pivot, so `find`/`find_first`/`count` on one side can be narrowed by the id of a row on the other side.
- A direct column **read** always leaves them as `None` — a `SELECT` never projects them from the endpoint tables.

The navigation fields (`systems` and `business`) are the read side of this relation, named exactly as the schema declares — note that `business` on `System` is a `Vec`, singular name notwithstanding, because that's what the field was called in the schema. Load them with `.includes(...)`; never read a virtual ID expecting it to tell you whether a link exists.

### Load either direction

The include loader always joins through `_business_to_system` itself — it never looks for a `business_id` column on `system` that doesn't exist:

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

Filters and ordering apply to the related model as usual, `take`/`skip` apply per parent, and a nested include can cross the very same pivot back in the opposite direction (as shown above with `system.business()`).

The reverse direction just uses the other side's navigation field:

```rust
let systems = dinoco::find_many::<System>()
    .includes(|system| system.business())
    .execute(&client)
    .await?;
```

### Filter by the other side

The same virtual ID field doubles as a `where_(...)` filter. `system.business_id.eq(&business_id)` keeps only the systems linked to that business through the pivot — Dinoco compiles it to `system.id IN (SELECT system_id FROM _business_to_system WHERE business_id = ?)`, so no join table is exposed and the endpoint rows still come back with the virtual key `None`:

```rust
// Only the systems connected to `business_id`.
let systems = dinoco::find_many::<System>()
    .where_(|system| system.business_id.eq(&business_id))
    .execute(&client)
    .await?;

// Mirror direction: the businesses connected to `system_id`.
let businesses = dinoco::find_many::<Business>()
    .where_(|business| business.system_id.eq(&system_id))
    .execute(&client)
    .await?;
```

The virtual key carries the **full [filter](/en-us/docs/orm/orm/filters) surface**, evaluated against the pivot's target column: `.eq(...)` / `.neq(...)`, `.gt(...)` / `.gte(...)` / `.lt(...)` / `.lte(...)`, `.batch([...])` / `.not_in([...])`, `.null()` / `.not_null()`, `.like(...)` / `.starts_with(...)` / `.ends_with(...)` for string keys, and `.between(a, b)` for numeric or date keys. `.neq(...)` and `.not_in([...])` negate membership (`NOT IN`), so they also keep rows with no pivot entry at all; every other method keeps rows linked to *some* pivot row matching the predicate. `.not_null()` is the "has any link" test. These filters compose with plain scalar filters and with `where_complex` (`w.not(system.business_id.eq(&id))` inverts any of them), and `count::<T>()` honours them too:

```rust
let linked = dinoco::count::<System>()
    .where_(|system| system.business_id.eq(&business_id))
    .execute(&client)
    .await?;
```

### Count related records

Relation counts traverse the pivot the same way includes do, and accept filters on the related model:

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

When both endpoints already exist, insert them first, then call `.connect(...)` on the virtual target ID:

```rust
let business = Business::new("Dinoco".to_string());
let system = System::new("Backoffice".to_string(), "Administrative system".to_string());

dinoco::insert_into::<Business>().values(&business).execute(&client).await?;
dinoco::insert_into::<System>().values(&system).execute(&client).await?;

let business_id = business.id;
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system.id))
    .execute(&client)
    .await?;
```

This inserts exactly `(business.id, system.id)` into `_business_to_system` — nothing else. The returned `business.system_id` is still `None`, because the field stays write-only even in a returning query; connecting never inserts an endpoint and never touches an actual column on `business` or `system`.

The reverse call creates the identical pair:

```rust
dinoco::update::<System>()
    .where_(|system| system.id.eq(&system_id))
    .update(|system| system.business_id.connect(&business_id))
    .execute(&client)
    .await?;
```

> [!WARNING]
> The pivot's composite primary key rejects a duplicate pair, so connecting the same two endpoints twice returns a database constraint error rather than silently succeeding. Foreign keys also reject a `connect` targeting an endpoint that doesn't exist — insert both sides before connecting them.

### Disconnect endpoints

Use the same virtual field, with `.disconnect(...)` instead:

```rust
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.disconnect(&system_id))
    .execute(&client)
    .await?;
```

Only the matching pivot row is deleted — both endpoints themselves are left completely untouched.

### Supported update builders

Virtual many-to-many keys support `.connect(...)`/`.disconnect(...)` from:

- `update::<M>()`
- `update_many::<M>()`
- `find_and_update::<M>()`
- `update`/`update_many` combined with `.returning::<S>()`

The filter selecting the endpoint can use any field, not just its primary key — Dinoco resolves every matching endpoint's ID on the primary connection before touching the pivot. That means one system can be connected to several businesses in a single call:

```rust
dinoco::update_many::<Business>()
    .where_(|business| business.name.starts_with("Dinoco"))
    .update(|business| business.system_id.connect(&system_id))
    .execute(&client)
    .await?;
```

These operations also accept the closure transaction context — call `.execute(tx)` so the pivot change and any other scalar update in the same closure share one physical connection, and a later failure rolls both back together.

### Connect during insert

Set the virtual ID *before* inserting the endpoint, and `insert_into` inserts the endpoint (without ever treating the virtual field as a real column) then creates the pivot row in the same call:

```rust
let mut system = System::new("ERP".to_string(), "Enterprise resource planning".to_string());
system.business_id = Some(business.id);

dinoco::insert_into::<System>().values(&system).execute(&client).await?;
```

`insert_many` applies the same rule independently to every item in the batch:

```rust
let mut systems = vec![
    System::new("CRM".to_string(), "Customer management".to_string()),
    System::new("BI".to_string(), "Business intelligence".to_string()),
];

for system in &mut systems {
    system.business_id = Some(business.id);
}

dinoco::insert_many::<System>().values(&systems).execute(&client).await?;
```

This also works with `.returning::<S>()` on either insert call — the returned virtual keys still come back `None`. A single virtual field can only stage one target ID at a time; to link an endpoint to *several* targets, insert it once and follow up with repeated `.connect(...)` calls. Assigning values into `business.systems` directly does nothing at the database level — only the virtual ID field creates pivot rows.

`insert_into`/`insert_many` with a populated virtual ID work inside the closure transaction API too, via `.execute(tx)`, making the endpoint insert and its pivot row atomic together. See [Transactions](/en-us/docs/orm/orm/transactions#supported-builders) for the full list of what runs inside a transaction.

### Upgrading generated models from v1.1.1

If you have a project generated before implicit many-to-many pivots became virtual-field-based, regenerating models removes the old pivot file and stops exporting the `BusinessSystem` entity. Replace any code still updating that entity directly:

```rust
// Before
dinoco::update::<BusinessSystem>()
    .where_(|pivot| pivot.business_id.eq(&business_id))
    .update(|pivot| pivot.system_id.connect(&system_id));

// v1.2.0 and later
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system_id));
```

The underlying SQL pivot table and its migration history don't change at all — only the public generated Rust API does.

## Many-to-many with extra fields

Reach for an explicit pivot model the moment the link itself needs to carry data — a role, a timestamp, a permission level:

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

This is no longer a many-to-many relation as far as the compiler is concerned — it's two ordinary one-to-many relations meeting at `AccountSystemAccess`. Insert that model exactly like any other entity; its foreign-key fields still generate as `Snowflake`, matching the models they reference.

## Repeated relations

When two models are related in more than one way, give each relation a matching `name` so Dinoco (and the compiler) can tell them apart:

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

Without the `name`, the compiler has no way to know that `authored_posts` pairs with `author` rather than `reviewer` — two relations between the same two models are ambiguous unless named.

## Self relations

A model can relate to itself the same way it relates to any other model — the only difference is the explicit `name`, required so Dinoco can distinguish the two "sides" of what would otherwise look like a single field relating to itself twice:

```dinoco
model Employee {
    id         String     @id @default(uuid())
    manager_id String?
    manager    Employee?  @relation(name: "Management", fields: [manager_id], references: [id], onDelete: SetNull)
    reports    Employee[] @relation(name: "Management", fields: [id], references: [manager_id])
}
```

## Referential actions

| Action | Effect |
| --- | --- |
| `Cascade` | Propagates the update or delete to dependent rows. |
| `Restrict` | Rejects the operation outright while dependent rows exist. |
| `NoAction` | Defers enforcement timing to the database itself. |
| `SetNull` | Sets the foreign key to `NULL`; requires both the key and the navigation field to be optional. |
| `SetDefault` | Falls back to the foreign key's declared `@default(...)`. |

## Relation checklist

1. Decide which model actually stores the foreign key — that's the "many"/owning side.
2. Use `String` for a UUID-backed key, `Integer` for a Snowflake-backed one.
3. Keep the foreign key's optionality and the navigation field's optionality as two independent, deliberate choices.
4. Add `@unique` on the foreign key for a one-to-one relation — without it, it's a many-to-one.
5. Map **both** sides of a one-to-many relation with matching `fields`/`references`.
6. Leave both list fields unmapped only when you actually want an implicit many-to-many.
7. Populate the generated virtual ID during `insert_into`/`insert_many` for new endpoints, or use `.connect(...)`/`.disconnect(...)` for existing ones — never assign directly to the navigation list.
8. Filter one side of an implicit many-to-many by the other with the same virtual ID in `where_(...)` (`system.business_id.eq(&business_id)`); it never reads back as anything but `None`.
9. Reach for an explicit pivot model the moment the link needs to store its own data.
10. Name every relation that's either repeated between two models, or a self relation.
11. Read the generated migration's constraints before applying it — a referential action decision is a data-integrity decision, not just a compiler-satisfying one.
