# Relações

Relações têm duas partes diferentes:

- a chave escalar persistida no banco, como `account_id`;
- o field de navegação, como `account`, `sessions` ou `systems`.

Não misture as duas. A chave escalar participa de inserts, filtros e constraints. O field de navegação começa vazio e é preenchido por include quando a relação possui carregamento direto.

## Regra de tipos para UUID e Snowflake

No schema, UUID continua sendo declarado como `String` e Snowflake como `Integer`. O codegen da v1.0.9 acompanha a chave referenciada e preserva o wrapper Rust:

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

O Rust gerado usa:

```rust
pub struct Account {
    pub id: ::dinoco::Snowflake,
    pub sessions: Vec<Session>,
}

pub struct Session {
    pub id: ::dinoco::Uuid,
    pub account_id: ::dinoco::Snowflake,
    pub account: Account,
}
```

Portanto:

- `String @default(uuid())` vira `dinoco::Uuid`;
- uma FK `String` que referencia esse ID também vira `dinoco::Uuid`;
- `Integer @default(snowflake())` vira `dinoco::Snowflake`;
- uma FK `Integer` que referencia esse ID também vira `dinoco::Snowflake`;
- optionalidade é preservada: `String?` vira `Option<Uuid>` e `Integer?` vira `Option<Snowflake>`.

Não use `Float` para referenciar Snowflake. A chave Snowflake é inteira.

## Anatomia de @relation

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

- `fields` contém fields escalares do model atual.
- `references` contém fields escalares do model relacionado.
- As duas listas precisam ter o mesmo tamanho e tipos compatíveis.
- `author_id` é opcional, então `author` também precisa ser opcional.
- `onDelete` e `onUpdate` viram ações da foreign key.

Cada foreign key materializada recebe automaticamente um índice nas colunas de `fields`, preservando a ordem em relações compostas. Uma tabela pivô many-to-many implícita tem um índice para sua primary key composta e um para cada foreign key.

## One-to-many e many-to-one

Um `Account` possui várias `Session`; cada `Session` pertence a um `Account`:

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
        onDelete: Cascade,
        onUpdate: Cascade
    )
}
```

Crie o parent e um child:

```rust
let account = Account::new("ana@example.com".to_string());
dinoco::insert_into::<Account>()
    .values(&account)
    .execute(&client)
    .await?;

let session = Session::new(
    account.id,
    "token-seguro".to_string(),
);
dinoco::insert_into::<Session>()
    .values(&session)
    .execute(&client)
    .await?;
```

Leia os dois lados:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|x| {
        x.sessions()
            .where_(|session| session.token.starts_with("token-"))
            .order_by(|session| session.id.desc())
            .take(10)
    })
    .execute(&client)
    .await?;

let session = dinoco::find_first::<Session>()
    .where_(|x| x.token.eq("token-seguro"))
    .includes(|x| x.account())
    .execute(&client)
    .await?;
```

No lado lista, `take(10)` limita dez children por parent.

## One-to-one

One-to-one é uma foreign key com `@unique`:

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
    user    User   @relation(
        fields: [user_id],
        references: [id],
        onDelete: Cascade
    )
}
```

Sem `@unique`, vários profiles poderiam apontar para o mesmo user e a relação seria many-to-one.

Em uma foreign key one-to-one composta, declare a unicidade da tuple local completa com `@@uniques([field_a, field_b])`. Uma lista `references` composta pode apontar para `@@ids([...])` ou para um grupo `@@uniques([...])` correspondente no model relacionado.

```rust
let profile = dinoco::find_first::<Profile>()
    .where_(|x| x.user_id.eq(&user_id))
    .includes(|x| x.user())
    .execute(&client)
    .await?;
```

## Many-to-many implícito

Listas dos dois lados, sem `fields` e `references`, criam uma tabela pivô:

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

Para esse schema, o Dinoco gera:

- tabela SQL `_account_to_systems`;
- chave composta `(account_id, systems_id)`;
- entity Rust `AccountSystems`;
- `account_id: dinoco::Snowflake`;
- `systems_id: dinoco::Snowflake`.

O nome da entity usa os nomes dos models em ordem alfabética. `Post` + `Tag` gera `PostTag`. Uma relação nomeada adiciona o nome ao final.

### Conectar many-to-many, passo a passo

Primeiro insira os dois registros. `connect` cria apenas o vínculo; ele não cria `Account` nem `Systems`.

```rust
let account = Account::new("ana@example.com".to_string());
let system = Systems::new(
    "Backoffice".to_string(),
    "internal".to_string(),
    "Sistema administrativo".to_string(),
);

dinoco::insert_into::<Account>()
    .values(&account)
    .execute(&client)
    .await?;

dinoco::insert_into::<Systems>()
    .values(&system)
    .execute(&client)
    .await?;
```

Agora conecte:

```rust
dinoco::update::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .update(|pivot| pivot.systems_id.connect(system.id))
    .execute(&client)
    .await?;
```

Isso equivale conceitualmente a:

```sql
INSERT INTO _account_to_systems (account_id, systems_id)
VALUES (?, ?);
```

Não use `.set()` para criar vínculo. Use `.connect()`.

### Desconectar many-to-many

```rust
dinoco::update::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .update(|pivot| pivot.systems_id.disconnect(system.id))
    .execute(&client)
    .await?;
```

Isso remove somente a row da pivô. `Account` e `Systems` continuam existindo.

### Conectar vários vínculos

Um account para dois systems:

```rust
dinoco::update::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .update(|pivot| pivot.systems_id.connect(first_system.id))
    .update(|pivot| pivot.systems_id.connect(second_system.id))
    .execute(&client)
    .await?;
```

Vários accounts para o mesmo system:

```rust
dinoco::update_many::<AccountSystems>()
    .where_(|pivot| pivot.account_id.batch([first_account.id, second_account.id]))
    .update(|pivot| pivot.systems_id.connect(system.id))
    .execute(&client)
    .await?;
```

A chave composta impede o mesmo par de ser inserido duas vezes. Se repetir um `connect`, o banco pode retornar violação de primary key.

### Consultar many-to-many sem mágica

Busque primeiro os vínculos e depois os registros relacionados:

```rust
let links = dinoco::find_many::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .execute(&client)
    .await?;

let system_ids = links
    .iter()
    .map(|link| link.systems_id)
    .collect::<Vec<_>>();

let systems = dinoco::find_many::<Systems>()
    .where_(|system| system.id.batch(system_ids))
    .order_by(|system| system.name.asc())
    .execute(&client)
    .await?;
```

Esse fluxo deixa explícito onde você filtra a pivô e onde filtra o model final.

### Remover todos os vínculos de um parent

```rust
dinoco::delete_many::<AccountSystems>()
    .where_(|pivot| pivot.account_id.eq(account.id))
    .execute(&client)
    .await?;
```

Novamente: isso não remove os systems.

### Erros comuns em many-to-many

1. Tentar conectar antes de inserir os dois lados.
2. Usar `update::<Account>()` em vez de `update::<AccountSystems>()`.
3. Usar `.set(system.id)` em vez de `.connect(system.id)`.
4. Trocar `account_id` por `systems_id`.
5. Inserir o mesmo par duas vezes.
6. Esperar que preencher `account.systems` faça insert automático na pivô.
7. Criar manualmente outro model para a mesma tabela implícita.

## Many-to-many com campos extras

Se o vínculo possui `role`, `created_at`, permissões ou qualquer outro dado, ele não é uma pivô implícita simples. Declare um model explícito:

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

Nesse caso, insira `AccountSystemAccess` normalmente. As FKs geradas continuam sendo `Snowflake` no Rust.

## Relações repetidas

Duas relações entre os mesmos models precisam de nomes iguais em seus respectivos lados:

```dinoco
model User {
    id              String @id @default(uuid())
    authored_posts  Post[] @relation(name: "PostAuthor", fields: [id], references: [author_id])
    reviewed_posts  Post[] @relation(name: "PostReviewer", fields: [id], references: [reviewer_id])
}

model Post {
    id          String @id @default(uuid())
    author_id   String
    reviewer_id String?

    author   User  @relation(name: "PostAuthor", fields: [author_id], references: [id])
    reviewer User? @relation(name: "PostReviewer", fields: [reviewer_id], references: [id])
}
```

Sem nomes, o compiler não consegue determinar quais lados formam cada par.

## Self relations

```dinoco
model Employee {
    id         String     @id @default(uuid())
    manager_id String?
    manager    Employee?  @relation(
        name: "Management",
        fields: [manager_id],
        references: [id],
        onDelete: SetNull
    )
    reports Employee[] @relation(
        name: "Management",
        fields: [id],
        references: [manager_id]
    )
}
```

Use um nome explícito e dois fields diferentes. Um único field não pode ser seu próprio lado oposto.

## Ações referenciais

| Ação | Efeito |
| --- | --- |
| `Cascade` | Propaga update ou delete aos dependentes. |
| `Restrict` | Impede a operação enquanto houver dependentes. |
| `NoAction` | Delega o momento do enforcement ao banco. |
| `SetNull` | Grava `NULL`; exige FK e relação opcionais. |
| `SetDefault` | Aplica o default declarado na FK. |

Use `Cascade` somente quando o child realmente não faz sentido sem o parent.

## Checklist de relações

1. Identifique qual model guarda a foreign key.
2. Use `String` para UUID e `Integer` para Snowflake no schema.
3. Mantenha optionalidade da FK e da relação coerentes.
4. Use `@unique` para one-to-one.
5. Declare `fields` e `references` nos dois lados de one-to-many.
6. Deixe ambas as listas sem keys somente para many-to-many implícito.
7. Use a entity pivô gerada para `connect` e `disconnect`.
8. Modele uma pivô explícita quando o vínculo tiver campos extras.
9. Nomeie relações repetidas e self relations.
10. Revise a migration antes de aplicá-la.
