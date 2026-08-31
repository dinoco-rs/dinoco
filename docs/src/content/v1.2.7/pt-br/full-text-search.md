# Busca full-text

O Dinoco oferece busca full-text nativa no PostgreSQL e MySQL e um fallback por substring no SQLite. O method `.fulltext(...)` só é gerado para fields que ativam o recurso no `schema.dinoco`.

## 1. Marque os fields pesquisáveis

```dinoco
model Account {
    id        String  @id @default(uuid())
    name      String  @fulltext
    biography String? @fulltext
    email     String
    reviewed  Boolean @default(false)
}
```

Um model pode ter vários fields `@fulltext`. Cada declaração recebe seu próprio índice no PostgreSQL e MySQL.

As regras são:

- somente `String` e `String?`;
- nenhum argumento;
- não pode coexistir com `@index` no mesmo field;
- não pode ser aplicado a uma relação.

### Forme um documento com vários fields

Use `@@fulltexts` quando uma busca deve cobrir um grupo ordenado:

```dinoco
model Article {
    id       String  @id @default(uuid())
    title    String
    subtitle String?
    body     String

    @@fulltexts([title, subtitle, body])
}
```

Todos os membros recebem a capability `.fulltext(...)`. `article.title.fulltext("dinoco")`, `article.subtitle.fulltext("dinoco")` e `article.body.fulltext("dinoco")` pesquisam o mesmo documento combinado. Isso é essencial no MySQL: a query usa `MATCH(title, subtitle, body)`, exatamente como o índice `FULLTEXT` composto.

Use declarações `@fulltext` separadas quando os fields precisarem de índices independentes. Um field pode pertencer a apenas uma declaração full-text e não pode participar também de `@index` ou `@@indexes`.

## 2. Gere a migration e os models

```bash
dinoco migrate generate
dinoco migrate run
```

Além do índice nativo quando aplicável, o model Rust gerado recebe a capability que habilita o method:

```rust
account.name.fulltext("matheus");
```

`account.email.fulltext(...)` não compila, pois `email` não possui `@fulltext` nem participa de `@@fulltexts`.

## Use em find first

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

O retorno continua `Option<Account>`.

## Use em find many

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .order_by(|account| account.id.asc())
    .execute(&client)
    .await?;
```

O retorno continua `Vec<Account>`.

## Use em find and update

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.biography.fulltext("dinoco"))
    .update(|account| account.reviewed.set(true))
    .execute(&client)
    .await?;
```

A busca seleciona a row que será atualizada e retornada. Veja [Find and update](/v1.2.7/orm/find-and-update).

## Use em relation includes

O method funciona nos builders one e many gerados para includes:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account
            .sessions()
            .where_(|session| session.label.fulltext("mobile"))
    })
    .execute(&client)
    .await?;
```

O field da entity relacionada também precisa estar marcado com `@fulltext`.

## Combine com where complex

```rust
let accounts = dinoco::find_many::<Account>()
    .where_complex(|account, m| {
        m.and([
            account.biography.fulltext("dinoco"),
            m.not(account.biography.fulltext("deprecated")),
        ])
    })
    .execute(&client)
    .await?;
```

O full-text pode aparecer em qualquer grupo `and`, `or`, `or_many` ou `not`.

## Use em transactions

Condições full-text continuam fazendo parte de uma mutation executada pelo contexto transacional:

```rust
let account = dinoco::transaction(&client, |tx| async move {
    let account = dinoco::find_and_update::<Account>()
        .where_(|account| account.name.fulltext("matheus"))
        .update(|account| account.reviewed.set(true))
        .execute(tx)
        .await?;

    Ok(account)
})
.await?;
```

O adapter compila o mesmo predicado full-text no `UPDATE` executado pela conexão transacional.

## Comportamento por banco

| Adapter | Query | Índice |
| --- | --- | --- |
| PostgreSQL / PgBouncer | `to_tsvector('simple', ...) @@ plainto_tsquery('simple', ...)` | GIN de expressão |
| MySQL | `MATCH (...) AGAINST (... IN NATURAL LANGUAGE MODE)` | `FULLTEXT` |
| SQLite | `(field_a LIKE '%termo%' OR field_b LIKE '%termo%')` | Nenhum índice nativo |

Em declarações compostas, o PostgreSQL usa o mesmo documento concatenado no índice GIN e na query. O MySQL preserva a lista exata de colunas em `MATCH(...)`. O SQLite pesquisa substring em todos os membros do grupo e pode varrer a tabela em datasets grandes.

## Comportamento das migrations

PostgreSQL e MySQL usam `idx_<tabela>_<fields...>_fulltext`. O planner preserva a ordem declarada e cria, remove, introspecta e verifica drift desses índices separadamente dos índices comuns e unique.

O SQLite omite a migration de índice porque um B-tree comum não acelera um `LIKE` iniciado por wildcard.

## Limitações

- `@fulltext` cria um índice independente de um field; `@@fulltexts([...])` cria um documento combinado.
- Um field pode pertencer a somente uma declaração full-text e não pode sobrepor `@index` ou `@@indexes`.
- Todo membro do grupo deve ser `String` ou `String?`.
- Ranking, idioma configurável, frases e tokenizers customizados não fazem parte da v1.2.7.
- A semântica de substring do SQLite difere da busca por tokens nativa.
