# Select

`select::<S>()` reduz as colunas escalares retornadas e troca o tipo de resultado por uma projeção gerada.

## 1. Declare uma projeção

Use `EntityExtend` e informe a entity de origem:

```rust
use dinoco::EntityExtend;

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

Os nomes e tipos precisam corresponder aos fields escalares da entity.

## 2. Selecione no find

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .order_by(|account| account.email.asc())
    .execute(&client)
    .await?;
```

O retorno é `Vec<AccountSummary>`. Em `find_first`, o retorno é `Option<AccountSummary>`.

O derive implementa a conversão das rows nativas de SQLite, PostgreSQL e MySQL; não há mapeamento manual.

## 3. Combine com filtros

`select` não altera o `EntityWhere`: os filtros continuam usando fields do model original.

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

Isso também vale para `where_complex` e `fulltext`.

## Selecione relações

Uma projeção pode declarar uma relation shape compatível e depois usar `includes(...)`. Se a projeção não declara a relação, não tente incluí-la nesse retorno.

Quando um child também usa `select`, o loader transporta a relation key separadamente. A projeção não precisa expor a foreign key apenas para o Dinoco agrupar as rows.

## Select em transactions

`find_first::<M>().select::<S>()` e `find_many::<M>().select::<S>()` preservam seus tipos dentro de uma transaction. Leia os resultados como `Option<S>` ou `Vec<S>`.

Includes continuam indisponíveis dentro da batch v1.0.9.
