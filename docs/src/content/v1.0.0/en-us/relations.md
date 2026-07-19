# Relations

Relations connect generated entities and database foreign keys. The most important distinction is between the scalar key column, which is stored in SQL, and the relation field, which Dinoco populates only when requested by an include.

## Relation anatomy

```dinoco
model Post {
    id        Integer @id @default(autoincrement())
    author_id String?
    author    User?   @relation(
        fields: [author_id],
        references: [id],
        onDelete: SetNull,
        onUpdate: Cascade
    )
}
```

`fields` names the local foreign-key field. `references` names the target field. The optional `author` relation matches the nullable `author_id` key.

## One-to-many and many-to-one

Define both navigable sides explicitly:

```dinoco
model User {
    id     String @id @default(uuid())
    email  String @unique
    tokens UserToken[] @relation(fields: [id], references: [user_id])
}

model UserToken {
    id      String  @id @default(uuid())
    user_id String?
    user    User?   @relation(fields: [user_id], references: [id], onDelete: SetNull)
}
```

`User.tokens` is the one-to-many side and becomes `Vec<UserToken>`. `UserToken.user` is the many-to-one side and becomes `Option<User>`. The explicit fields and references on the list side prevent it from being mistaken for an implicit many-to-many relation.

## One-to-one

Make the foreign key unique:

```dinoco
model User {
    id      Integer  @id @default(autoincrement())
    profile Profile?
}

model Profile {
    id      Integer @id @default(autoincrement())
    user_id Integer? @unique
    user    User?    @relation(fields: [user_id], references: [id], onDelete: Cascade)
}
```

The `@unique` constraint ensures that one user cannot be referenced by multiple profiles. A one-side include is loaded with a left join.

## Many-to-many

Lists on both sides without explicit key mappings define an implicit join relation:

```dinoco
model Post {
    id   String @id @default(uuid())
    tags Tag[]
}

model Tag {
    id    String @id @default(uuid())
    posts Post[]
}
```

Dinoco creates and manages the join table automatically. You do not need to declare an intermediate model in the schema. Use `connect` and `disconnect` in the update API to change its links.

## Repeated relations

When two models have more than one relation between them, assign the same unique name to corresponding sides:

```dinoco
model Follow {
    follower_id  String
    following_id String
    follower     User @relation(name: "follower", fields: [follower_id], references: [id])
    following    User @relation(name: "following", fields: [following_id], references: [id])
}
```

The relation name is metadata, not a column. It removes ambiguity for code generation, includes, counts, and migration foreign keys.

## Self relations

A self relation points back to the current model and also needs a name when more than one path could match:

```dinoco
model Employee {
    id         String      @id @default(uuid())
    manager_id String?
    manager    Employee?   @relation(name: "management", fields: [manager_id], references: [id], onDelete: SetNull)
    reports    Employee[]  @relation(name: "management", fields: [id], references: [manager_id])
}
```

The generated types are recursive through `Option<Employee>` and `Vec<Employee>`, while includes control when those values are loaded.

## Referential actions

- `Cascade` propagates an update or delete to related records.
- `Restrict` prevents the operation while dependent records exist.
- `NoAction` delegates enforcement timing and behavior to the database.
- `SetNull` clears the foreign key and requires an optional relation.
- `SetDefault` assigns the foreign key's declared default.

Declare them inside `@relation` as `onDelete` and `onUpdate`. Dinoco validates supported names and the `SetNull` optionality rule before generating SQL.

## Relation checklist

1. Add the scalar foreign-key field on the owning side.
2. Match optionality between a nullable key and its relation field.
3. Add `@unique` for a one-to-one key.
4. Add explicit `fields` and `references` to a one-to-many list.
5. Name repeated and self relation paths.
6. Choose referential actions based on real data ownership, not convenience.
7. Generate a migration and inspect the resulting constraints before applying it.
