# Documentacao Dinoco

## Objetivo

Este documento descreve o que o Dinoco expoe para ser usado na aplicacao:

- metodos de consulta e escrita
- filtros e builders disponiveis
- carregamento de relacoes
- logger customizado
- linguagem do `schema.dinoco`

O foco aqui e a API publica e a forma de uso. Nao e uma documentacao da implementacao interna.

## O que o codegen gera

Para cada `model` do schema, o codegen gera estruturas e traits prontas para uso.

Exemplo conceitual para um model `User`:

- `User`
- `UserWhere`
- `UserUpdate`
- `UserInclude`
- `UserRelations`

O model gerado normalmente implementa:

- `Model`
- `Projection`
- `InsertModel`
- `UpdateModel`
- `FindAndUpdateModel`
- `Rowable`
- `serde::Serialize`
- `serde::Deserialize`

Quando houver relacoes suportadas para escrita, o model tambem implementa:

- `RelationMutationModel`

## Leitura

## `find_many::<M>()`

Usado para buscar uma lista de registros.

### O que voce pode fazer

- selecionar uma projecao com `.select::<T>()`
- filtrar com `.cond(...)`
- limitar com `.take(...)`
- paginar com `.skip(...)`
- ordenar com `.order_by(...)`
- carregar relacoes com `.includes(...)`
- contar relacoes com `.count(...)`
- forcar leitura no banco principal com `.read_in_primary()`
- executar com `.execute(&client)`

### Exemplo basico

```rust
let users = dinoco::find_many::<User>()
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
SELECT "id", "email", "name"
FROM "User"
```

### Filtrando

```rust
let users = dinoco::find_many::<User>()
    .cond(|w| w.email.eq("ana@acme.com"))
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
SELECT "id", "email", "name"
FROM "User"
WHERE "email" = ?1
```

### Com `take`, `skip` e `order_by`

```rust
let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .skip(20)
    .take(10)
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
SELECT "id", "email", "name"
FROM "User"
ORDER BY "name" ASC
LIMIT ?1 OFFSET ?2
```

### Selecionando uma projecao menor com `Extend`

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserListItem {
    id: i64,
    name: String,
}

let users = dinoco::find_many::<User>()
    .select::<UserListItem>()
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
SELECT "id", "name"
FROM "User"
```

## `find_first::<M>()`

Usado para buscar no maximo um registro.

### O que voce pode fazer

Ele expoe os mesmos metodos de `find_many`:

- `select`
- `cond`
- `take`
- `skip`
- `order_by`
- `includes`
- `count`
- `read_in_primary`
- `execute`

### Exemplo

```rust
let user = dinoco::find_first::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

Retorno:

```rust
DinocoResult<Option<User>>
```

SQL esperado:

```sql
SELECT "id", "email", "name"
FROM "User"
WHERE "id" = ?1
LIMIT ?2
```

## `count::<M>()`

Usado para contar registros.

### O que voce pode fazer

- `.cond(...)`
- `.execute(&client)`

### Exemplo

```rust
let total = dinoco::count::<User>()
    .cond(|w| w.active.eq(true))
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
SELECT COUNT(*) FROM (
  SELECT *
  FROM "User"
  WHERE "active" = ?1
) AS "__dinoco_count"
```

### Exemplo com filtro de texto

```rust
let total = dinoco::count::<User>()
    .cond(|w| w.name.includes("Ana"))
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
SELECT COUNT(*) FROM (
  SELECT *
  FROM "User"
  WHERE "name" LIKE ?1
) AS "__dinoco_count"
```

## Filtros disponiveis em `Where`

Os filtros sao expostos via `ScalarField<T>` dentro do `ModelWhere` gerado.

### Operadores gerais

- `eq(value)`
- `neq(value)`
- `gt(value)`
- `gte(value)`
- `lt(value)`
- `lte(value)`
- `in_values(values)`
- `not_in_values(values)`
- `is_null()`
- `is_not_null()`
- `asc()`
- `desc()`

### Operadores extras para `String`

- `includes(value)`
- `starts_with(value)`
- `ends_with(value)`

### Exemplos

```rust
let users = dinoco::find_many::<User>()
    .cond(|w| w.name.includes("Ana"))
    .execute(&client)
    .await?;
```

```rust
let users = dinoco::find_many::<User>()
    .cond(|w| w.id.in_values([1_i64, 2, 3]))
    .execute(&client)
    .await?;
```

```rust
let users = dinoco::find_many::<User>()
    .cond(|w| w.deletedAt.is_null())
    .execute(&client)
    .await?;
```

## Includes e relacoes na leitura

As relacoes sao expostas em `ModelInclude`.

Cada relacao pode ser usada:

- diretamente
- com query customizada

## Include simples

```rust
let users = dinoco::find_many::<User>()
    .includes(|i| i.posts())
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
SELECT "id", "email", "name"
FROM "User";

SELECT "id", "title", "authorId"
FROM "Post"
WHERE "authorId" IN (?1, ?2, ?3)
```

## Include com filtro

```rust
let users = dinoco::find_many::<User>()
    .includes(|i| i.posts().cond(|w| w.published.eq(true)))
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
SELECT "id", "email", "name"
FROM "User";

SELECT "id", "title", "published", "authorId"
FROM "Post"
WHERE "published" = ?1
  AND "authorId" IN (?2, ?3, ?4)
```

## Include com ordenacao e limite

```rust
let users = dinoco::find_many::<User>()
    .includes(|i| i.posts().order_by(|w| w.createdAt.desc()).take(5))
    .execute(&client)
    .await?;
```

### `find_many` com relacoes aninhadas

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Comment)]
struct CommentListItem {
    id: i64,
    text: String,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Post)]
struct PostWithComments {
    id: i64,
    title: String,
    comments: Vec<CommentListItem>,
    commentsCount: usize,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<PostWithComments>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPosts>()
    .includes(|i| {
        i.posts()
            .select::<PostWithComments>()
            .includes(|post| post.comments().take(3).select::<CommentListItem>())
            .count(|post| post.comments())
    })
    .execute(&client)
    .await?;
```

## Count de relacao

```rust
let users = dinoco::find_many::<User>()
    .count(|i| i.posts())
    .execute(&client)
    .await?;
```

## O que uma relacao expõe

Uma relacao em `Include` normalmente permite:

- `select::<T>()`
- `cond(...)`
- `take(...)`
- `skip(...)`
- `order_by(...)`
- `includes(...)`
- `count(...)`

## Escrita

## `insert_into::<M>()`

Usado para inserir um registro.

### O que voce pode fazer

- `.values(item)`
- `.with_relation(related)`
- `.with_connection(connected)`
- `.execute(&client)`

## Insert simples

```rust
dinoco::insert_into::<User>()
    .values(User {
        id: "usr_1".to_string(),
        email: "ana@acme.com".to_string(),
        name: "Ana".to_string(),
    })
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
INSERT INTO "User" ("id", "email", "name")
VALUES (?1, ?2, ?3)
```

## Insert com relacao

Use `.with_relation(...)` quando o model gerado suporta inserir o pai e o relacionado juntos.

```rust
dinoco::insert_into::<User>()
    .values(user)
    .with_relation(profile)
    .execute(&client)
    .await?;
```

Exemplo de SQL esperado:

```sql
INSERT INTO "User" ("id", "email", "name")
VALUES (?1, ?2, ?3);

INSERT INTO "Profile" ("id", "bio", "userId")
VALUES (?4, ?5, ?6);
```

## Insert com conexao

Use `.with_connection(...)` quando voce quer inserir um item e conecta-lo a um registro ja existente.

```rust
dinoco::insert_into::<User>()
    .values(new_user)
    .with_connection(existing_team)
    .execute(&client)
    .await?;
```

Exemplo de SQL esperado:

```sql
INSERT INTO "User" ("id", "name")
VALUES (?1, ?2);

UPDATE "Team"
SET "ownerId" = ?3
WHERE "id" = ?4;
```

## `insert_many::<M>()`

Usado para insercao em lote.

### O que voce pode fazer

- `.values(Vec<M>)`
- `.with_relation(Vec<R>)`
- `.with_relations(Vec<Vec<R>>)`
- `.with_connections(Vec<Vec<R>>)`
- `.execute(&client)`

## Exemplo simples

```rust
dinoco::insert_many::<User>()
    .values(vec![
        User { id: "u1".into(), email: "a@acme.com".into(), name: "A".into() },
        User { id: "u2".into(), email: "b@acme.com".into(), name: "B".into() },
    ])
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
INSERT INTO "User" ("id", "email", "name")
VALUES (?1, ?2, ?3), (?4, ?5, ?6)
```

### `with_relations(...)`

`with_relations(...)` serve para passar varios relacionados para cada item pai.

Exemplo conceitual:

```rust
dinoco::insert_many::<Post>()
    .values(posts)
    .with_relations(comments_por_post)
    .execute(&client)
    .await?;
```

Observacao importante:

- `with_relations(...)` so funciona corretamente quando o relacionamento consegue ser montado com IDs que ja existem em memoria no momento da chamada.
- Em outras palavras: o item pai precisa ja ter o ID definido no proprio struct antes do `execute`.
- Isso funciona bem com IDs gerados na aplicacao, como `uuid()` e `snowflake()`.
- Nao e apropriado para cenarios em que o ID do pai so passa a existir depois do insert no banco, como `autoincrement()`.

## `update::<M>()`

Usado para atualizar registros filtrados.

### O que voce pode fazer

- `.cond(...)`
- `.values(item)`
- `.connect(...)`
- `.disconnect(...)`
- `.execute(&client)`

## Update de campos

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .values(User {
        id: 10,
        email: "novo@acme.com".to_string(),
        name: "Novo Nome".to_string(),
    })
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
UPDATE "User"
SET "email" = ?1, "name" = ?2
WHERE "id" = ?3
```

## `connect(...)`

Usado para conectar relacoes suportadas para escrita, normalmente N:M.

O closure recebe `ModelRelations`.

Exemplo:

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .connect(|r| r.roles().slug.eq("admin"))
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
INSERT INTO "_UserRoles" ("user_id", "role_id")
VALUES (?1, ?2)
```

## `disconnect(...)`

Usado para remover conexoes de relacao.

Exemplo:

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .disconnect(|r| r.roles().slug.eq("guest"))
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
DELETE FROM "_UserRoles"
WHERE "user_id" IN (?1) AND "role_id" IN (?2)
```

## Filtros de relacao em `connect` e `disconnect`

As relacoes expostas para escrita usam `RelationScalarField<T>`.

Operadores disponiveis:

- `eq`
- `neq`
- `gt`
- `gte`
- `lt`
- `lte`
- `in_values`
- `not_in_values`
- `is_null`
- `is_not_null`
- `includes`
- `starts_with`
- `ends_with`

Tambem e possivel combinar:

- `.and(...)`
- `.or(...)`

Exemplo:

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .connect(|r| {
        r.roles().slug.eq("admin").or(r.roles().slug.eq("manager"))
    })
    .execute(&client)
    .await?;
```

## `update_many::<M>()`

Usado para atualizar varios registros de uma vez.

### O que voce pode fazer

- `.cond(...)`
- `.values(Vec<M>)`
- `.execute(&client)`

### Exemplo

```rust
dinoco::update_many::<User>()
    .values(vec![
        User { id: 1, email: "a@acme.com".into(), name: "Ana".into() },
        User { id: 2, email: "b@acme.com".into(), name: "Bia".into() },
    ])
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
UPDATE "User" SET
  "email" = CASE
    WHEN "id" = ?1 THEN ?2
    WHEN "id" = ?3 THEN ?4
    ELSE "email"
  END,
  "name" = CASE
    WHEN "id" = ?5 THEN ?6
    WHEN "id" = ?7 THEN ?8
    ELSE "name"
  END
WHERE ("id" = ?9) OR ("id" = ?10)
```

## `find_and_update::<M>()`

Usado para localizar um unico registro, aplicar updates atomicos no banco e retornar o item atualizado.

### O que voce pode fazer

- `.cond(...)`
- `.update(...)`
- `.returning::<T>()`
- `.execute(&client)`

### Exemplo basico

```rust
let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

Retorno:

```rust
DinocoResult<Task>
```

### Exemplo com projecao customizada

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserScoreView {
    name: String,
    score: f64,
}

let user = dinoco::find_and_update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .update(|x| x.name.set("Ana Atomic"))
    .update(|x| x.score.increment(1.5_f64))
    .returning::<UserScoreView>()
    .execute(&client)
    .await?;
```

### Operacoes disponiveis em `ModelUpdate`

- `set(value)`
- `increment(value)`
- `decrement(value)`
- `multiply(value)`
- `division(value)`

### Observacoes

- o update e executado em um unico `UPDATE`
- se nenhuma linha bater na condicao, o retorno sera `DinocoError::RecordNotFound`
- a DSL de update nao expoe relacoes
- hoje o fluxo suporta chave primaria simples para localizar e retornar o registro atualizado

## `delete::<M>()`

Usado para deletar com filtro explicito.

### O que voce pode fazer

- `.cond(...)`
- `.execute(&client)`

### Exemplo

```rust
dinoco::delete::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
DELETE FROM "User"
WHERE "id" = ?1
```

## `delete_many::<M>()`

Usado para deletar varios registros com uma ou mais condicoes.

### O que voce pode fazer

- `.cond(...)`
- `.execute(&client)`

### Exemplo

```rust
dinoco::delete_many::<Session>()
    .cond(|w| w.expiresAt.lt(dinoco::Utc::now()))
    .execute(&client)
    .await?;
```

SQL esperado:

```sql
DELETE FROM "Session"
WHERE "expiresAt" < ?1
```

## Logger customizado

O logger de queries e configurado em `DinocoClientConfig`.

## Tipos publicos envolvidos

- `DinocoClientConfig`
- `DinocoQueryLogger`
- `DinocoQueryLoggerOptions`
- `DinocoQueryLog`
- `DinocoQueryLogWriter`

## Formas prontas de usar

### Desabilitado

```rust
let config = dinoco::DinocoClientConfig::default();
```

### Em stdout

```rust
let config = dinoco::DinocoClientConfig::default()
    .with_query_logger(dinoco::DinocoQueryLogger::stdout(
        dinoco::DinocoQueryLoggerOptions::verbose(),
    ));
```

### Em stderr

```rust
let config = dinoco::DinocoClientConfig::default()
    .with_query_logger(dinoco::DinocoQueryLogger::stderr(
        dinoco::DinocoQueryLoggerOptions::compact(),
    ));
```

## Passo a passo para criar um custom logger

### 1. Implemente `DinocoQueryLogWriter`

```rust
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct MemoryLogger {
    entries: Arc<Mutex<Vec<String>>>,
}

impl MemoryLogger {
    fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl dinoco::DinocoQueryLogWriter for MemoryLogger {
    fn write(&self, message: &str) {
        self.entries.lock().unwrap().push(message.to_string());
    }
}
```

### 2. Escolha as opcoes do logger

```rust
let options = dinoco::DinocoQueryLoggerOptions {
    include_adapter: true,
    include_duration: true,
    include_params: true,
    include_query: true,
    label: "Meu Logger".to_string(),
};
```

Ou use:

- `DinocoQueryLoggerOptions::compact()`
- `DinocoQueryLoggerOptions::verbose()`

### 3. Crie o `DinocoQueryLogger`

```rust
let writer = MemoryLogger::new();
let logger = dinoco::DinocoQueryLogger::custom(writer, options);
```

### 4. Injete no config

```rust
let config = dinoco::DinocoClientConfig::default()
    .with_query_logger(logger);
```

### 5. Crie o client com esse config

```rust
let client = dinoco::DinocoClient::<dinoco::SqliteAdapter>::new(
    "file:dev.db".to_string(),
    vec![],
    config,
)
.await?;
```

### 6. Use normalmente

```rust
let _users = dinoco::find_many::<User>()
    .take(5)
    .execute(&client)
    .await?;
```

## O que o logger recebe

Cada evento de query possui:

- `adapter`
- `duration`
- `params`
- `query`

Exemplo de saida:

```text
[Dinoco Query | adapter=sqlite | duration=2.1ms] query=SELECT "id" FROM "User" params=[]
```

## Schema `schema.dinoco`

O `schema.dinoco` define:

- configuracao do banco
- enums
- models
- relacoes

## Estrutura geral

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum UserRole {
    ADMIN
    MEMBER
}

model User {
    id Integer @id @default(autoincrement())
    email String @unique
    role UserRole @default(MEMBER)
}
```

## Configuracoes

O bloco `config` deve existir uma vez.

### Chaves suportadas

- `database`
- `database_url`
- `read_replicas`

### `database`

Valores aceitos:

- `"sqlite"`
- `"mysql"`
- `"postgresql"`

### `database_url`

Aceita:

- string literal valida
- `env("VAR")`

Exemplos:

```dinoco
database_url = "file:dev.db"
database_url = "mysql://root:root@localhost:3306/app"
database_url = "postgresql://postgres:postgres@localhost:5432/app"
database_url = env("DATABASE_URL")
```

### `read_replicas`

Aceita array de URLs ou `env(...)`.

Exemplo:

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_replicas = ["postgresql://replica-a", env("REPLICA_B")]
}
```

## Tipos suportados

### Escalares

- `String`
- `Boolean`
- `Integer`
- `Float`
- `Json`
- `DateTime`
- `Date`

### Customizados

Um tipo customizado pode ser:

- `enum`
- outro `model`

## Modificadores

### Opcional

```dinoco
nickname String?
```

### Lista

```dinoco
posts Post[]
```

## Decorators de campo

### `@id`

Chave primaria simples.

```dinoco
id Integer @id
```

### `@unique`

Campo unico.

```dinoco
email String @unique
```

### `@default(...)`

Valores suportados:

- string
- boolean
- integer
- float
- valor de enum
- `uuid()`
- `snowflake()`
- `autoincrement()`
- `now()`

Exemplos:

```dinoco
name String @default("Anonimo")
active Boolean @default(true)
age Integer @default(18)
score Float @default(9.5)
role UserRole @default(MEMBER)
id String @default(uuid())
id Integer @default(snowflake())
id Integer @default(autoincrement())
createdAt DateTime @default(now())
```

### `@relation(...)`

Parametros suportados:

- `name`
- `fields`
- `references`
- `onDelete`
- `onUpdate`

Exemplo:

```dinoco
author User @relation(fields: [authorId], references: [id], onDelete: Cascade)
```

## Decorators de model

### `@@ids([campoA, campoB])`

Chave primaria composta.

```dinoco
model Membership {
    userId Integer
    teamId Integer

    @@ids([userId, teamId])
}
```

### `@@table_name("nome_real")`

Mapeia o nome da tabela no banco.

```dinoco
model User {
    id Integer @id

    @@table_name("users")
}
```

## Funcoes suportadas

### Em `config`

- `env("VAR")`

### Em `@default(...)`

- `uuid()`
- `snowflake()`
- `autoincrement()`
- `now()`

## Enums

Exemplo:

```dinoco
enum UserRole {
    ADMIN
    MEMBER
}
```

Uso em model:

```dinoco
model User {
    id Integer @id
    role UserRole @default(MEMBER)
}
```

## Relacoes

## Um para muitos

```dinoco
model User {
    id Integer @id @default(autoincrement())
    name String
    posts Post[]
}

model Post {
    id Integer @id @default(autoincrement())
    title String
    authorId Integer
    author User @relation(fields: [authorId], references: [id], onDelete: Cascade)
}
```

## Um para um

```dinoco
model User {
    id Integer @id @default(autoincrement())
    profile Profile?
}

model Profile {
    id Integer @id @default(autoincrement())
    userId Integer @unique
    user User @relation(fields: [userId], references: [id])
}
```

## Muitos para muitos

```dinoco
model User {
    id Integer @id @default(autoincrement())
    roles Role[] @relation(name: "UserRoles")
}

model Role {
    id Integer @id @default(autoincrement())
    users User[] @relation(name: "UserRoles")
}
```

## Self relation

```dinoco
model User {
    id Integer @id @default(autoincrement())
    managerId Integer?
    manager User? @relation(name: "UserManager", fields: [managerId], references: [id])
    reports User[] @relation(name: "UserManager")
}
```

## Exemplo de schema sem relacoes

```dinoco
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

enum UserRole {
    ADMIN
    MEMBER
}

model User {
    id Integer @id @default(autoincrement())
    email String @unique
    name String
    active Boolean @default(true)
    role UserRole @default(MEMBER)
    metadata Json?
}
```

## Exemplo de schema com relacoes

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_replicas = [env("DATABASE_REPLICA_URL")]
}

enum PostStatus {
    DRAFT
    PUBLISHED
}

model User {
    id Integer @id @default(autoincrement())
    email String @unique
    name String
    profile Profile?
    posts Post[] @relation(name: "PostAuthor")
    likedPosts Post[] @relation(name: "PostLikes")
}

model Profile {
    id Integer @id @default(autoincrement())
    bio String?
    userId Integer @unique
    user User @relation(fields: [userId], references: [id], onDelete: Cascade)
}

model Post {
    id Integer @id @default(autoincrement())
    title String
    content String
    status PostStatus @default(DRAFT)
    authorId Integer
    author User @relation(name: "PostAuthor", fields: [authorId], references: [id], onDelete: Cascade)
    likes User[] @relation(name: "PostLikes")
    comments Comment[]
}

model Comment {
    id Integer @id @default(autoincrement())
    postId Integer
    post Post @relation(fields: [postId], references: [id], onDelete: Cascade)
    text String
}

## Resumo de uso

- Use `find_many` para listas.
- Use `find_first` para um unico item.
- Use `count` para contagens.
- Use `includes` para carregar relacoes.
- Use `insert_into` para insert simples.
- Use `insert_many` para insert em lote.
- Use `update` para update filtrado.
- Use `update_many` para lote.
- Use `connect` e `disconnect` para escrita de relacoes suportadas.
- Use `delete` e `delete_many` para remocao.
- Use `DinocoQueryLogger::custom(...)` quando quiser integrar logs com sua aplicacao.
