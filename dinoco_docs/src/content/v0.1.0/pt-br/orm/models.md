# Modelos

Os `modelo` definem as entidades centrais da sua aplicação no schema do Dinoco. Cada `modelo` normalmente representa uma tabela no banco de dados e serve como base para geração de código, queries tipadas e operações com a API do Dinoco.

---

## O que um modelo representa

Um `model` descreve:

- O nome da entidade.
- Os campos armazenados no banco.
- Quais campos são obrigatórios ou opcionais.
- Quais campos são únicos ou identificadores.
- Como esses dados serão usados pelo codegen e pela API.

Exemplo:

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique
	name  String?
}
```

Nesse exemplo:

- `User` é o model.
- `id`, `email` e `name` são campos escalares.
- `id` é o identificador principal.
- `email` tem restrição de unicidade.

## Exemplo completo

Um schema simples com modelo costuma se parecer com isto:

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

## Estrutura de um campo

Cada campo de um model é composto por:

- Nome
- Tipo
- Modificador opcional
- Atributos opcionais

Exemplo:

```dinoco
email String @unique
```

Nessa linha:

- `email` é o nome do campo.
- `String` é o tipo.
- `@unique` é um atributo.

## Tipos de campo

Os campos podem representar valores básicos do schema, como texto, número, booleano e datas.

### Campos escalares

São campos que armazenam valores diretos, como texto, número, booleano e datas.

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

## Modificadores de tipo

O Dinoco suporta dois modificadores principais:

| Modificador | Significado    | Exemplo         |
| :---------- | :------------- | :-------------- |
| `?`         | Campo opcional | `name String?`  |
| `[]`        | Lista          | `tags String[]` |

### Campo opcional

```dinoco
model User {
	id   Integer @id @default(autoincrement())
	name String?
}
```

`name` pode ser nulo ou ausente, dependendo do banco e da camada gerada.

### Campo em lista

```dinoco
model Article {
	id   Integer  @id @default(autoincrement())
	tags String[]
}
```

Esse formato representa uma lista de valores quando o banco e o fluxo suportam esse tipo de estrutura.

## Atributos mais comuns

Os atributos alteram o comportamento de campos e modelo.

| Atributo        | Uso                              |
| :-------------- | :------------------------------- |
| `@id`           | Define o identificador principal |
| `@default(...)` | Define um valor padrão           |
| `@unique`       | Garante unicidade                |
| `@virtual`      | Mantém o campo no model gerado sem criar coluna no banco |

### `@id`

Define o campo que identifica unicamente um registro.

```dinoco
id Integer @id @default(autoincrement())
```

Todo model deve ter um identificador claro para que a API gerada consiga operar com segurança.

### `@default(...)`

Define um valor padrão para o campo.

```dinoco
active    Boolean  @default(true)
createdAt DateTime @default(now())
id        Integer  @default(autoincrement())
```

Funções e valores comuns:

| Exemplo                     | Uso                 |
| :-------------------------- | :------------------ |
| `@default(false)`           | Booleano padrão     |
| `@default(now())`           | Data atual          |
| `@default(autoincrement())` | Inteiro incremental |
| `@default(uuid())`          | Identificador UUID  |

### `@virtual`

Define um campo que existe apenas no model Rust gerado.

Campos virtuais são úteis para dados calculados, valores preenchidos manualmente na aplicação ou informações temporárias que você quer carregar junto da struct sem persistir no banco.

```dinoco
model User {
	id          Integer @id @default(autoincrement())
	email       String
	displayName String? @virtual
	score       Integer @virtual @default(0)
}
```

Regras do `@virtual`:

- O campo aparece na struct gerada.
- O campo não é criado na tabela do banco.
- O campo não entra em `where`, `insert`, `update` nem no `SELECT` padrão.
- O campo precisa ser opcional (`?`) ou ter `@default(...)`.
- O campo não pode ser `@id`, `@unique` nem uma relação.

No exemplo acima, `displayName` começa como `None` e `score` começa com `0` quando o model é criado por default.

### `@unique`

Garante que o valor do campo não se repita.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique
}
```

Esse atributo é ideal para campos como email, username e códigos externos.

## Decorators de model

Além dos atributos em campos individuais, o Dinoco também suporta decorators aplicados ao bloco inteiro do model.

| Decorator             | Uso                                   |
| :-------------------- | :------------------------------------ |
| `@@ids([...])`        | Define chave primária composta        |
| `@@uniques([...])`    | Define restrições de unicidade compostas |
| `@@indexes([...])`    | Define índices compostos              |
| `@@table_name("...")` | Mapeia o nome real da tabela no banco |

### `@@ids([...])`

Use `@@ids` quando a identidade do registro depende de mais de um campo.

```dinoco
model Membership {
	userId Integer
	teamId Integer
	role   String

	@@ids([userId, teamId])
}
```

Esse formato é útil em tabelas associativas e cenários em que a unicidade natural já é composta.

### `@@uniques([...])`

Use `@@uniques` quando a regra de unicidade depender de mais de um campo, mas sem transformar essa combinação em chave primária.

```dinoco
model PostTranslation {
	id       Integer @id @default(autoincrement())
	postId   Integer
	locale   String
	slug     String
	title    String

	@@uniques([slug, locale])
}
```

Nesse exemplo, `slug` pode se repetir em outros idiomas, mas a combinação `slug + locale` não pode duplicar.

### `@@indexes([...])`

Use `@@indexes` para melhorar performance de consultas frequentes por múltiplos campos.

```dinoco
model Post {
	id       Integer @id @default(autoincrement())
	authorId Integer
	title    String

	@@indexes([title, authorId])
}
```

Esse decorator não impõe unicidade; ele cria apenas uma estrutura de indexação para acelerar filtros e ordenações.

### `@@table_name("...")`

Use `@@table_name()` quando você quiser manter um nome de model mais amigável no schema, mas mapear para outro nome físico no banco.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique

	@@table_name("users")
}
```

Nesse caso:

- O model continua se chamando `User` no schema e na API gerada.
- A tabela física no banco passa a ser `users`.

## Exemplo de model de usuário

```dinoco
model User {
	id        Integer  @id @default(autoincrement())
	email     String   @unique
	name      String?
	active    Boolean  @default(true)
	createdAt DateTime @default(now())
}
```

Depois do codegen, esse model pode ser usado diretamente com a API do Dinoco.

## Exemplo de busca de usuários com a API do Dinoco

### Buscar um único registro

```rust
let user = dinoco::find_first::<User>()
    .cond(|x| x.id.eq(1_i64))
    .execute(&client)
    .await?;
```

### Buscar vários registros

```rust
let users = dinoco::find_many::<User>()
    .cond(|x| x.name.includes("Ana"))
    .order_by(|x| x.id.asc())
    .take(10)
    .execute(&client)
    .await?;
```

## Exemplo de criação de usuário com a API do Dinoco

```rust
dinoco::insert_into::<User>()
    .values(User {
        id: 0,
        email: "bia@dinoco.rs".to_string(),
        name: Some("Bia".to_string()),
        active: true,
        createdAt: dinoco::Utc::now(),
    })
    .execute(&client)
    .await?;
```

## Exemplo de atualização de usuário com a API do Dinoco

```rust
dinoco::update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .update(|x| x.email.set("bia@dinoco.rs"))
    .update(|x| x.name.set(Some("Beatriz".to_string())))
    .execute(&client)
    .await?;
```

Se você quiser updates atômicos em um único campo, o fluxo `find_and_update` costuma ser ainda mais direto:

```rust
let user = dinoco::find_and_update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .update(|x| x.name.set("Beatriz"))
    .execute(&client)
    .await?;
```

## Exemplo de remoção de usuário com a API do Dinoco

```rust
dinoco::delete::<User>()
    .cond(|x| x.id.eq(1_i64))
    .execute(&client)
    .await?;
```

Para remoções em lote:

```rust
dinoco::delete_many::<User>()
    .cond(|x| x.active.eq(false))
    .execute(&client)
    .await?;
```

## Resumo rápido

| Conceito       | Exemplo                | Objetivo                    |
| :------------- | :--------------------- | :-------------------------- |
| Model          | `model User { ... }`   | Representar uma entidade    |
| Campo escalar  | `email String`         | Armazenar valor simples     |
| Campo opcional | `name String?`         | Permitir ausência de valor  |
| Campo em lista | `tags String[]`        | Armazenar múltiplos valores |
| ID             | `id Integer @id`       | Identificar unicamente      |
| Default        | `@default(now())`      | Preencher automaticamente   |
| Unique         | `email String @unique` | Evitar duplicidade          |
| Virtual        | `name String? @virtual` | Campo só na struct gerada   |

## Quando criar um novo model

Você normalmente cria um novo `model` quando uma entidade da sua aplicação precisa:

- Ser persistida no banco.
- Ter identidade própria.
- Ser consultada isoladamente.
- Ter regras próprias de leitura e escrita.

Exemplos comuns:

- `User`
- `Post`
- `Comment`
- `Category`
- `Order`
- `Invoice`
