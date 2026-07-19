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

## Fields opcionais e listas

`String?` vira `Option<String>` e uma relação `Profile?` vira `Option<Profile>`. O sufixo `[]` representa uma lista de relação e gera `Vec<T>`; ele não é uma coluna SQL de array.

```dinoco
display_name String?
posts        Post[]
```

## Atributos de field

- `@id` marca a primary key.
- `@unique` adiciona unicidade.
- `@default(...)` declara um literal, enum ou valor gerado.
- `@relation(...)` define chaves, nome e ações referenciais.

No model, `@@table_name("audit_users")` troca o nome da tabela e `@@ids([tenant_id, id])` define uma chave composta.

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

## Arquivos gerados

```text
dinoco/
  mod.rs
  models/
    mod.rs
    user.rs
    post.rs
```

`dinoco/mod.rs` exporta models e `connect()`. Enums ficam em `models/mod.rs`, e cada model tem seu próprio arquivo para manter diffs legíveis.
