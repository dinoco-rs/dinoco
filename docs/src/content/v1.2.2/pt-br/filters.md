# Filtros

Os tipos `Where` gerados expõem somente fields reais da entity. O tipo Rust do field limita os valores aceitos, evitando conversões frágeis em runtime.

## Monte uma cláusula where

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.active.eq(true))
    .where_(|user| user.age.gte(18))
    .execute(&client)
    .await?;
```

Vários `where_` são combinados com `AND`. A mesma sintaxe funciona em finds, includes, count, update e delete.

## Operadores comuns

| Method | SQL |
| --- | --- |
| `eq(value)` | `field = value` |
| `neq(value)` | `field <> value` |
| `gt(value)` | `field > value` |
| `gte(value)` | `field >= value` |
| `lt(value)` | `field < value` |
| `lte(value)` | `field <= value` |
| `batch(values)` | `field IN (...)` |
| `null()` | `field IS NULL` |
| `not_null()` | `field IS NOT NULL` |

Os valores são enviados como parâmetros do adapter, sem interpolação no SQL.

## Operadores de String

`String` e `Option<String>` adicionam:

```rust
dinoco::find_many::<User>().where_(|user| user.email.like("dinoco"));
dinoco::find_many::<User>().where_(|user| user.email.starts_with("support"));
dinoco::find_many::<User>().where_(|user| user.email.ends_with("@example.com"));
```

`like` coloca `%` dos dois lados, `starts_with` à direita e `ends_with` à esquerda. Não inclua os curingas manualmente.

Fields declarados com `@fulltext` também expõem `.fulltext(termo)`. Strings comuns não possuem esse method. Veja [Busca full-text](/v1.2.1/orm/full-text-search).

## Ranges numéricos e temporais

Inteiros, floats, `DateTime`, `Date` e suas versões opcionais suportam range inclusivo:

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.age.between(18, 65))
    .execute(&client)
    .await?;
```

## Verificações de null

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|task| task.owner_id.null())
    .execute(&client)
    .await?;
```

Use `not_null()` quando a row precisa ter uma foreign key preenchida.

## Valores em batch

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.id.batch(["user-a", "user-b"]))
    .execute(&client)
    .await?;
```

`batch` gera `IN (...)` e também fornece várias keys de origem para `connect` e `disconnect` em pivots.

## Combine condições

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.office.eq("engineering"))
    .where_(|user| user.age.lt(30))
    .where_(|user| user.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Para precedência com `AND`, `OR` e `NOT`, consulte a página dedicada [Where complex](/v1.2.1/orm/where-complex).
