# Relações

Uma relação conecta entities geradas e foreign keys. Separe mentalmente o field escalar persistido, como `author_id`, do field de relação, como `author`, que só é preenchido quando solicitado por include.

## Anatomia de uma relação

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

`fields` aponta para a chave local e `references` para a chave do model relacionado.

## One-to-many e many-to-one

```dinoco
model User {
    id     String @id @default(uuid())
    tokens UserToken[] @relation(fields: [id], references: [user_id])
}

model UserToken {
    id      String  @id @default(uuid())
    user_id String?
    user    User?   @relation(fields: [user_id], references: [id], onDelete: SetNull)
}
```

`tokens` vira `Vec<UserToken>` e `user` vira `Option<User>`. Declare `fields` e `references` na lista para ela não ser interpretada como many-to-many implícita.

## One-to-one

O foreign key precisa ser único:

```dinoco
model Profile {
    id      Integer @id @default(autoincrement())
    user_id Integer? @unique
    user    User?    @relation(fields: [user_id], references: [id], onDelete: Cascade)
}
```

`@unique` impede vários profiles para o mesmo user. Includes do lado `one` usam left join.

## Many-to-many

Listas dos dois lados sem mapeamento explícito formam uma relação implícita:

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

O Dinoco cria e gerencia a tabela pivot automaticamente. Não é necessário declarar um model intermediário no schema. Use `connect` e `disconnect` no update para alterar os vínculos.

## Relações repetidas

Duas relações entre os mesmos models precisam de nomes diferentes e consistentes:

```dinoco
model Follow {
    follower_id  String
    following_id String
    follower     User @relation(name: "follower", fields: [follower_id], references: [id])
    following    User @relation(name: "following", fields: [following_id], references: [id])
}
```

O nome elimina ambiguidades no codegen, includes, counts e foreign keys.

## Self relations

```dinoco
model Employee {
    id         String     @id @default(uuid())
    manager_id String?
    manager    Employee?  @relation(name: "management", fields: [manager_id], references: [id], onDelete: SetNull)
    reports    Employee[] @relation(name: "management", fields: [id], references: [manager_id])
}
```

O include controla quando os valores recursivos são carregados.

## Ações referenciais

- `Cascade` propaga update ou delete.
- `Restrict` impede a operação enquanto houver dependentes.
- `NoAction` delega detalhes de enforcement ao banco.
- `SetNull` limpa a foreign key e exige relação opcional.
- `SetDefault` aplica o default da foreign key.

## Checklist de relações

1. Declare a foreign key escalar no lado dono.
2. Mantenha a optionalidade da key e da relação coerentes.
3. Use `@unique` em one-to-one.
4. Mapeie a lista de one-to-many explicitamente.
5. Nomeie relações repetidas e self.
6. Escolha ações referenciais conforme a propriedade real dos dados.
7. Revise as constraints geradas antes de aplicar a migration.
