# Enums

Enums allow you to restrict a field to a fixed set of known values in the Dinoco schema.

They are useful when the value needs to be predictable, validated, and reused across models.

---

## What is an enum

An `enum` defines a closed list of possible values.

```dinoco
enum Role {
	USER
	ADMIN
}
```

In this case, `Role` can only be `USER` or `ADMIN`.

## Usage in models

Once defined, the enum can be used as a field type in any model.

```dinoco
enum Role {
	USER
	ADMIN
}

model User {
	id   Integer @id @default(autoincrement())
	role Role    @default(USER)
}
```

Here:

- `role` uses the `Role` enum.
- `@default(USER)` sets the default value for the field.

## When to use enums

Enums are useful for representing values such as:

- User roles
- Publication statuses
- Workflow stages
- Payment situations

Example:

```dinoco
enum PostStatus {
	DRAFT
	REVIEW
	PUBLISHED
	ARCHIVED
}

model Post {
	id     Integer    @id @default(autoincrement())
	title  String
	status PostStatus @default(DRAFT)
}
```

## Best practices

- Use enums when possible values are known and finite.
- Prefer PascalCase names for the enum and UPPER_CASE values.
- Use `@default(...)` when there is a natural initial state.

## Next steps

- [**Relations**](/v0.0.1/orm/relations): see `@relation`, `onDelete`, `onUpdate`, and relationship types.
- [**Models**](/v0.0.1/orm/models): see where enums fit into field definitions and the main schema.
