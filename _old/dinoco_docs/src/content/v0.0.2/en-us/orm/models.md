# Models

The `model` defines the central entities of your application in the Dinoco schema. Each `model` typically represents a table in the database and serves as the basis for code generation, typed queries, and operations with the Dinoco API.

---

## What a model represents

A `model` describes:

- The entity's name.
- The fields stored in the database.
- Which fields are mandatory or optional.
- Which fields are unique or identifiers.
- How this data will be used by codegen and the API.

Example:

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique
	name  String?
}
```

In this example:

- `User` is the model.
- `id`, `email`, and `name` are scalar fields.
- `id` is the primary identifier.
- `email` has a uniqueness constraint.

## Complete example

A simple schema with a model usually looks like this:

```dinoco
config {
	database = "postgresql"
	database_url = env("DATABASE_URL")
}

model User {
	id        Integer  @id @default(autoincrement())
	email     String   @unique
	name      String?
	active    Boolean  @default(true)
	createdAt DateTime @default(now())
}

model Post {
	id        Integer  @id @default(autoincrement())
	title     String
	content   String?
	published Boolean  @default(false)
	createdAt DateTime @default(now())
}
```

## Field structure

Each field of a model is composed of:

- Name
- Type
- Optional modifier
- Optional attributes

Example:

```dinoco
email String @unique
```

In this line:

- `email` is the field name.
- `String` is the type.
- `@unique` is an attribute.

## Field types

Fields can represent basic schema values, such as text, numbers, booleans, and dates.

### Scalar fields

These are fields that store direct values, such as text, numbers, booleans, and dates.

```dinoco
model Product {
	id          Integer  @id @default(autoincrement())
	name        String
	description String?
	price       Float
	active      Boolean  @default(true)
	createdAt   DateTime @default(now())
}
```

## Type modifiers

Dinoco supports two main modifiers:

| Modifier | Meaning        | Example         |
| :---------- | :------------- | :-------------- |
| `?`         | Optional field | `name String?`  |
| `[]`        | List           | `tags String[]` |

### Optional field

```dinoco
model User {
	id   Integer @id @default(autoincrement())
	name String?
}
```

`name` can be null or absent, depending on the database and the generated layer.

### List field

```dinoco
model Article {
	id   Integer  @id @default(autoincrement())
	tags String[]
}
```

This format represents a list of values when the database and workflow support this type of structure.

## Most common attributes

Attributes change the behavior of fields and models.

| Attribute        | Usage                              |
| :-------------- | :------------------------------- |
| `@id`           | Defines the primary identifier   |
| `@default(...)` | Defines a default value          |
| `@unique`       | Ensures uniqueness               |

### `@id`

Defines the field that uniquely identifies a record.

```dinoco
id Integer @id @default(autoincrement())
```

Every model must have a clear identifier for the generated API to operate safely.

### `@default(...)`

Defines a default value for the field.

```dinoco
active    Boolean  @default(true)
createdAt DateTime @default(now())
id        Integer  @default(autoincrement())
```

Common functions and values:

| Example                     | Usage                 |
| :-------------------------- | :------------------ |
| `@default(false)`           | Default boolean       |
| `@default(now())`           | Current date          |
| `@default(autoincrement())` | Incremental integer   |
| `@default(uuid())`          | UUID identifier       |

### `@unique`

Ensures that the field's value does not repeat.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique
}
```

This attribute is ideal for fields like email, username, and external codes.

## Model decorators

In addition to attributes on individual fields, Dinoco also supports decorators applied to the entire model block.

| Decorator             | Usage                                   |
| :-------------------- | :------------------------------------ |
| `@@ids([...])`        | Defines a composite primary key       |
| `@@table_name("...")` | Maps the actual table name in the database |

### `@@ids([...])`

Use `@@ids` when the record's identity depends on more than one field.

```dinoco
model Membership {
	userId Integer
	teamId Integer
	role   String

	@@ids([userId, teamId])
}
```

This format is useful in associative tables and scenarios where natural uniqueness is already composite.

### `@@table_name("...")`

Use `@@table_name()` when you want to keep a more friendly model name in the schema, but map it to a different physical name in the database.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique

	@@table_name("users")
}
```

In this case:

- The model continues to be called `User` in the schema and the generated API.
- The physical table in the database becomes `users`.

## User model example

```dinoco
model User {
	id        Integer  @id @default(autoincrement())
	email     String   @unique
	name      String?
	active    Boolean  @default(true)
	createdAt DateTime @default(now())
}
```

After codegen, this model can be used directly with the Dinoco API.

## Example of fetching users with the Dinoco API

### Fetch a single record

```rust
let user = dinoco::find_first::<User>()
    .cond(|x| x.id.eq(1_i64))
    .execute(&client)
    .await?;
```

### Fetch multiple records

```rust
let users = dinoco::find_many::<User>()
    .cond(|x| x.name.includes("Ana"))
    .order_by(|x| x.id.asc())
    .take(10)
    .execute(&client)
    .await?;
```

## Example of creating a user with the Dinoco API

```rust
dinoco::insert_into::<User>()
    .values(User {
        id: 0,
        email: "bea@dinoco.rs".to_string(),
        name: Some("Bea".to_string()),
        active: true,
        createdAt: dinoco::Utc::now(),
    })
    .execute(&client)
    .await?;
```

## Example of updating a user with the Dinoco API

```rust
dinoco::update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .values(User {
        id: 1,
        email: "bea@dinoco.rs".to_string(),
        name: Some("Beatrice".to_string()),
        active: true,
        createdAt: dinoco::Utc::now(),
    })
    .execute(&client)
    .await?;
```

If you want atomic updates on a single field, the `find_and_update` flow is often even more straightforward:

```rust
let user = dinoco::find_and_update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .update(|x| x.name.set("Beatrice"))
    .execute(&client)
    .await?;
```

## Example of deleting a user with the Dinoco API

```rust
dinoco::delete::<User>()
    .cond(|x| x.id.eq(1_i64))
    .execute(&client)
    .await?;
```

For batch deletions:

```rust
dinoco::delete_many::<User>()
    .cond(|x| x.active.eq(false))
    .execute(&client)
    .await?;
```

## Quick summary

| Concept        | Example                | Goal                        |
| :------------- | :--------------------- | :-------------------------- |
| Model          | `model User { ... }`   | Represent an entity         |
| Scalar field   | `email String`         | Store a simple value        |
| Optional field | `name String?`         | Allow absence of value      |
| List field     | `tags String[]`        | Store multiple values       |
| ID             | `id Integer @id`       | Uniquely identify           |
| Default        | `@default(now())`      | Automatically populate      |
| Unique         | `email String @unique` | Prevent duplication         |

## When to create a new model

You typically create a new `model` when an entity in your application needs to:

- Be persisted in the database.
- Have its own identity.
- Be queried in isolation.
- Have its own read and write rules.

Common examples:

- `User`
- `Post`
- `Comment`
- `Category`
- `Order`
- `Invoice`
