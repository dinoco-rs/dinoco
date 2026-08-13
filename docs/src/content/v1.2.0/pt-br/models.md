# Models e fields

Um model descreve uma tabela e uma struct Rust gerada. Os nomes continuam visíveis na API, então o schema é o melhor ponto para entender o domínio.

## Declare um model

```dinoco
model User {
    id         String   @id @default(uuid())
    email      String   @unique
    display    String?
    active     Boolean  @default(true)
    score      Float
    created_at DateTime @default(now())
}
```

Isso gera uma `Entity` chamada `User`, com fields públicos e conversão de row para cada adapter.

## Regra da primary key

Todo model deve declarar exatamente uma primary key:

- use um único `@id` em field para uma chave de uma coluna; ou
- use um único `@@ids([...])` para uma chave composta.

Um `@@ids` composto conta como uma declaração, independentemente da quantidade de fields. Um model sem nenhuma das formas falha na compilação do schema. Dois `@id`, dois `@@ids` ou a combinação de `@id` com `@@ids` também falham.

Fields da primary key devem ser scalars ou enums obrigatórios. A ordem de `@@ids` é preservada na constraint e no índice automático do banco.

## Tipos escalares

| Dinoco | Rust | SQLite | PostgreSQL | MySQL |
| --- | --- | --- | --- | --- |
| `String` | `String` | `TEXT` | `TEXT` | `VARCHAR(255)` |
| `Boolean` | `bool` | `BOOLEAN` | `BOOLEAN` | `TINYINT(1)` |
| `Integer` | `i64` | `INTEGER` | `BIGINT` | `BIGINT` |
| `Float` | `f64` | `REAL` | `DOUBLE PRECISION` | `DOUBLE PRECISION` |
| `DateTime` | `DateTime<Utc>` | `DATETIME` | `TIMESTAMP` | `TIMESTAMP` |
| `Date` | `NaiveDate` | `DATE` | `DATE` | `DATE` |
| `Json` | `serde_json::Value` | `BLOB` | `JSONB` | `JSON` |

IDs gerados e suas foreign keys preservam wrappers descritivos:

| Declaração | Rust gerado |
| --- | --- |
| `String @default(uuid())` | `dinoco::Uuid` |
| `Integer @default(snowflake())` | `dinoco::Snowflake` |
| FK `String` referenciando UUID | `dinoco::Uuid` |
| FK `Integer` referenciando Snowflake | `dinoco::Snowflake` |

## Fields opcionais e listas

`String?` vira `Option<String>` e uma relação `Profile?` vira `Option<Profile>`. O sufixo `[]` representa uma lista de relação e gera `Vec<T>`; ele não é uma coluna SQL de array.

```dinoco
display_name String?
posts        Post[]
```

## Atributos de field

- `@id` marca a primary key.
- `@unique` adiciona unicidade.
- `@index` cria um índice não único para o field.
- `@fulltext` cria a capability e o índice full-text de um field String.
- `@default(...)` declara um literal, enum ou valor gerado.
- `@relation(...)` define chaves, nome e ações referenciais.

## Atributos do model

Atributos de model atuam sobre grupos ordenados de fields:

| Atributo | Finalidade |
| --- | --- |
| `@@ids([tenant_id, id])` | Primary key composta |
| `@@uniques([tenant_id, slug])` | Unicidade composta |
| `@@indexes([tenant_id, created_at])` | Índice comum composto |
| `@@fulltexts([title, body])` | Índice full-text composto |
| `@@table_name("audit_users")` | Nome físico da tabela |

`@@ids`, `@@uniques`, `@@indexes` e `@@fulltexts` recebem um array não vazio de fields existentes, scalars ou enums, sem repetição. Todos os fields de `@@fulltexts` devem ser `String` ou `String?`.

O formatter sempre move os atributos de model para depois de todos os fields, separados por uma linha vazia. Assim, todo model mantém uma estrutura estável com fields primeiro.

Consulte [Índices e constraints](/v1.2.0/guide/indexes) para índices simples e compostos, unicidade e índices automáticos de primary e foreign keys.

## A função new gerada

`pub fn new(...) -> Self` recebe somente escalares obrigatórios sem default nem auto-geração. Opcionais começam em `None`, listas em `Vec::new()` e defaults recebem seu valor inicial.

```dinoco
model User {
    id      String  @id @default(uuid())
    email   String
    name    String
    enabled Boolean @default(true)
    bio     String?
    posts   Post[]
}
```

```rust
let user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
```

## Fields gerados para many-to-many implícito

Uma relação many-to-many implícita mantém a tabela pivô interna e não gera uma entity pública para ela. Em vez disso, `Post` recebe `tag_id: Option<TagId>` e `Tag` recebe `post_id: Option<PostId>`, ambos write-only.

Preencher um desses fields antes de `insert_into` ou em cada item de `insert_many` cria o vínculo correspondente depois que o endpoint é inserido:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.post_id = Some(post.id.clone());

dinoco::insert_into::<Tag>()
    .values(&tag)
    .execute(&client)
    .await?;
```

O field virtual não é uma coluna de `tag` e sempre volta como `None`. Para endpoints existentes, use as operações de update `connect` e `disconnect` no mesmo field gerado. Consulte [Many-to-many implícito](/v1.2.0/guide/relations#many-to-many-implícito) para o contrato completo.

## Arquivos gerados

```text
dinoco/
  mod.rs
  models/
    mod.rs
    user.rs
    post.rs
```

`dinoco/mod.rs` exporta models e `connect()`. Enums ficam compactos em `models/mod.rs` usando `DinocoEnum`, e cada model tem seu próprio arquivo.
