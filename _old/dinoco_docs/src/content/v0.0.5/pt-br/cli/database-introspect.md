# Como usar?

O comando `dinoco database introspect` lê o estado atual do banco e gera o arquivo `dinoco/schema.dinoco`.

Ele é ideal para iniciar projetos em bancos já existentes ou sincronizar o schema Dinoco com estruturas criadas fora da CLI.

---

## O que o comando faz

Ao executar o comando, a CLI tenta:

- Conectar no banco configurado
- Ler tabelas, colunas, foreign keys e enums
- Inferir tipos (`Boolean`, `Integer`, `Float`, `DateTime`, `Enum`, etc.)
- Inferir relações (`one to many`, `one to one`, `many to many`)
- Gerar `dinoco/schema.dinoco`

## Heurísticas de tipagem

A introspecção tenta ser fiel ao banco.

Exemplo de heurística importante:

- Campos numéricos com valores apenas `0/1` podem ser mapeados como `Boolean`

## Exemplo rápido

```bash
dinoco database introspect
```

Se `database_url` estiver como `env("DATABASE_URL")`, garanta a variável definida:

```bash
export DATABASE_URL="postgres://postgres:root@localhost:5432/dinoco"
dinoco database introspect
```

## Exemplo complexo (índices e relações)

Considere um banco com:

- Índice em `post.title`
- Relação `one to many` entre `User` e `Post`
- Self relation `one to one` em `User.manager_id`
- Relação `many to many` entre `Post` e `Tag` via tabela de junção

A introspecção tenta gerar algo próximo de:

```dinoco
model User {
    id         Integer @id
    email      String  @unique
    manager_id Integer?

    manager    User?   @relation(name: UserManager, fields: [manager_id], references: [id])
    managerOf  User?   @relation(name: UserManager)

    posts      Post[]
}

model Post {
    id        Integer @id
    author_id Integer
    title     String

    author    User    @relation(fields: [author_id], references: [id])
    tags      Tag[]

    @@indexes([title, author_id])
}

model PostTranslation {
    id      Integer @id
    post_id Integer
    locale  String
    slug    String

    post    Post    @relation(fields: [post_id], references: [id])

    @@uniques([slug, locale])
}

model Tag {
    id    Integer @id
    name  String  @unique

    posts Post[]
}
```

## Quando usar

Use este comando quando:

- Você já possui um banco legado
- Você quer reconstruir `schema.dinoco` a partir do banco real
- Você precisa validar se o schema local está alinhado com o ambiente

## Próximos passos

Após introspectar:

```bash
dinoco models generate
```

ou, se fizer alterações no schema depois:

```bash
dinoco migrate generate
```
