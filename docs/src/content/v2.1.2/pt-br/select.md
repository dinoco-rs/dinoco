# Select

`select::<S>()` reduz a query a um conjunto menor de colunas escalares e troca o tipo do resultado por uma struct de projeção gerada, em vez da entity completa.

## 1. Declare uma projeção

Derive `EntityExtend` e aponte para a entity da qual a projeção vem:

```rust
use dinoco::EntityExtend;

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

O nome e o tipo de cada field precisam bater exatamente com um field escalar da entity de origem — uma projeção não é um lugar para renomear ou remodelar dados, só para escolher um subconjunto deles.

## 2. Selecione no find

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .order_by(|account| account.email.asc())
    .execute(&client)
    .await?;
```

O tipo de retorno vira `Vec<AccountSummary>`; em `find_first`, vira `Option<AccountSummary>`. O `EntityExtend` gera uma conversão de row nativa para SQLite, PostgreSQL e MySQL como parte do derive — não existe uma implementação manual de `From<Row>` para escrever numa projeção, assim como não existe para uma entity completa.

## 3. Combine com filtros

`select` reduz o que volta, não o que você pode filtrar — o `EntityWhere` não é afetado, então os filtros continuam usando fields do model original, não da projeção:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

O mesmo vale para `where_complex` e `.fulltext(...)` — os dois operam no model completo, independentemente do que você acabe fazendo `.select(...)`.

## Selecione relações

Uma projeção pode declarar uma relation shape compatível e depois carregá-la com `includes(...)`, do mesmo jeito que uma entity completa faz. Se uma projeção *não* declara uma relação, não tente `.includes(...)` nela na mesma query — não existe field na projeção para os dados carregados irem parar.

> [!TIP]
> Quando um model relacionado também usa `select`, o loader de include continua carregando a foreign key da relação internamente para agrupar as rows corretamente — a projeção em si não precisa expor essa chave como field só para o Dinoco conseguir agrupar. Mantenha suas projeções limitadas aos dados que você realmente quer ler.
