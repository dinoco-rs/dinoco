# Includes

`includes(...)` popula fields de relação em uma query. Sem include, relações mantêm o valor vazio gerado: `Vec::new()` para many e `None` para one.

## Inclua uma relação many

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

O Dinoco reúne as parent keys e carrega os children em uma query de batch.

## Inclua uma relação one

```rust
let session = dinoco::find_first::<AccountSession>()
    .where_(|session| session.id.eq(&session_id))
    .includes(|session| session.account())
    .execute(&client)
    .await?;
```

Relações one usam uma estratégia de left join e continuam opcionais quando não há correspondência.

## Filtre a relação

O builder da relação expõe os mesmos filtros gerados:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account
            .sessions()
            .where_(|session| session.revoked.eq(false))
            .order_by(|session| session.created_at.desc())
            .take(5)
            .skip(0)
    })
    .execute(&client)
    .await?;
```

Em uma relação many, `take(5)` vale por parent. O compiler usa window partition na query em batch.

## Use where complex e full-text

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account.sessions().where_complex(|session, m| {
            m.and([
                session.label.fulltext("mobile"),
                m.not(session.revoked.eq(true)),
            ])
        })
    })
    .execute(&client)
    .await?;
```

`where_complex` ignora todos os `where_` no mesmo builder de relação. `.fulltext(...)` só existe quando o field relacionado possui `@fulltext`.

## Faça includes aninhados

```rust
let projects = dinoco::find_many::<Project>()
    .includes(|project| project.owner())
    .includes(|project| {
        project
            .tasks()
            .order_by(|task| task.priority.desc())
            .take(10)
            .includes(|task| task.assignee())
    })
    .execute(&client)
    .await?;
```

Includes irmãos são aguardados em paralelo; cada nível aninhado repete a estratégia apropriada.

## Combine com select

O relation builder também aceita `select::<S>()`. A relation key é carregada separadamente da projeção para manter o agrupamento correto.

## Primary e transactions

`read_in_primary()` no find principal direciona o parent e todos os includes ao primary.

Includes não são suportados dentro de uma `Transaction` na v1.1.1. O builder retorna um erro antes de abrir a transaction.
