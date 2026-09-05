# Filtros

Todo tipo `Where` gerado só expõe methods para fields que realmente existem na entity — não existe um escape hatch tipo `.where("alguma_coluna", ...)` baseado em string. O tipo Rust de cada field também restringe quais valores compilam contra ele, então um filtro comparando um field `DateTime` contra uma `String` falha em tempo de compilação, em vez de virar um erro confuso de banco em runtime.

## Monte uma cláusula where

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.active.eq(true))
    .where_(|user| user.age.gte(18))
    .execute(&client)
    .await?;
```

Várias chamadas `where_` se combinam com `AND` — esse mesmo padrão funciona de forma idêntica em finds, includes, counts, updates e deletes, então uma vez que você aprendeu aqui, é o mesmo em todo lugar.

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

Todo valor passado para esses methods é enviado como parâmetro vinculado do adapter — nunca interpolado diretamente na string SQL, o que é o que mantém essa API imune a SQL injection por construção, não por convenção.

## Operadores de String

Fields `String` e `Option<String>` ganham mais três:

```rust
dinoco::find_many::<User>().where_(|user| user.email.like("dinoco"));
dinoco::find_many::<User>().where_(|user| user.email.starts_with("support"));
dinoco::find_many::<User>().where_(|user| user.email.ends_with("@example.com"));
```

`like` envolve seu valor com `%` dos dois lados, `starts_with` adiciona só à direita, e `ends_with` só à esquerda — não adicione os curingas `%` você mesmo, ou vai acabar procurando por um caractere `%` literal.

Um field declarado com `@fulltext` expõe adicionalmente `.fulltext(termo)` — um field `String` comum sem esse atributo simplesmente não tem esse method, então não tem como chamar busca full-text por acidente numa coluna sem índice. Veja [Busca full-text](/pt-br/docs/orm/orm/full-text-search).

## Ranges numéricos e temporais

`Integer`, `Float`, `DateTime`, `Date` e suas versões opcionais suportam uma checagem de range inclusivo:

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.age.between(18, 65))
    .execute(&client)
    .await?;
```

## Verificações de null

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|task| task.owner_id.null()) // SQL: owner_id IS NULL
    .execute(&client)
    .await?;
```

Use `not_null()` para o oposto — rows onde uma foreign key nullable (ou qualquer field nullable) está de fato preenchida.

## Valores em batch

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.id.batch(["user-a", "user-b"]))
    .execute(&client)
    .await?;
```

`batch` gera SQL `IN (...)`. É também o que você vai usar quando precisar fornecer várias keys de origem de uma vez para as operações `connect`/`disconnect` de uma pivô many-to-many.

## Combine condições

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.office.eq("engineering"))
    .where_(|user| user.age.lt(30))
    .where_(|user| user.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

> [!NOTE]
> Todo exemplo nesta página combina condições com um `AND` implícito — é a única precedência que `where_` entende. No momento em que você precisar de `OR` ou `NOT`, ou de `AND`/`OR` misturados com agrupamento explícito, essa API não consegue expressar isso; veja [Where complex](/pt-br/docs/orm/orm/where-complex) em vez disso.
