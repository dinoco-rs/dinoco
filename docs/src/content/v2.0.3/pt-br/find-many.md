# Find many

`find_many` retorna todas as rows compatíveis como `Vec<M>`. Zero correspondências é um resultado perfeitamente normal aqui — você recebe um vetor vazio de volta, nunca um erro.

## Consulta básica

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .execute(&client)
    .await?;
```

Várias chamadas `where_` se combinam com `AND`. Use [Where complex](/pt-br/docs/orm/orm/where-complex) no momento em que precisar de precedência explícita entre `AND`, `OR` e `NOT`.

## Ordene os resultados

```rust
let accounts = dinoco::find_many::<Account>()
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

O builder aceita uma ordenação tipada, via `asc()` ou `desc()` num field.

## Paginação

`take` limita quantas rows voltam; `skip` define quantas pular antes de começar a contar:

```rust
let page = dinoco::find_many::<Account>()
    .order_by(|account| account.id.asc())
    .take(25)
    .skip(50)
    .execute(&client)
    .await?;
```

> [!WARNING]
> Paginação por offset (`take`/`skip`) só produz uma sequência de páginas estável e sem sobreposição quando combinada com um `order_by` estável. Pagine sem ordenar — ou ordene por uma coluna com empates, tipo um field de status — e rows podem aparecer em duas páginas ou sumir entre elas conforme os dados por baixo mudam.

## Busca full-text

Todo field marcado com `@fulltext` expõe um method correspondente:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .execute(&client)
    .await?;
```

Veja [Busca full-text](/pt-br/docs/orm/orm/full-text-search) para como isso se comporta diferente em cada adapter.

## Selecione e inclua

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

O tipo de retorno vira `Vec<AccountSummary>` em vez de `Vec<Account>`. `includes(...)` funciona junto com `select`, e pode filtrar, ordenar e paginar as rows relacionadas independentemente da query pai — veja [Select](/pt-br/docs/orm/orm/select) e [Includes](/pt-br/docs/orm/orm/includes).

## Pluck em uma única coluna

`pluck` projeta sobre uma coluna e retorna seus valores diretamente, em vez de um struct de row-model:

```rust
let ids = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .pluck(|account| account.id)
    .execute(&client)
    .await?; // Vec<i64>
```

Diferente de `select::<S>()`, `pluck` nunca constrói `S` — a coluna volta como `Vec<T>` (um `String`, `bool`, `i64`, `f64`, data, ou valor JSON, de acordo com o tipo do field). Ele substitui `select`/`includes` em vez de se combinar com eles.

## Transforme os resultados

`transform` roda uma closure Rust simples sobre cada row depois que a query (e qualquer `.includes(...)`) já rodou — é um mapeamento pós-query, não uma projeção SQL:

```rust
let summaries = dinoco::find_many::<Account>()
    .transform(|account| serde_json::json!({
        "id": account.id,
        "email": account.email,
    }))
    .execute(&client)
    .await?; // Vec<serde_json::Value>
```

Como a closure recebe a row já totalmente materializada, ela pode ler fields vindos de `.includes(...)`, remodelar dados para uma resposta de API, ou converter para um tipo completamente diferente.

## Leia no primary

`read_in_primary()` roteia essa query — e toda relação que ela `.includes(...)` — para longe das réplicas e direto para a primary. Use isso especificamente quando a leitura depende de um write que acabou de acontecer e não pode tolerar lag de replicação. Assim como os outros builders de leitura, `find_many` não faz parte da API transacional por closure; rode ele direto pelo `&client`, fora de qualquer closure `transaction(...)`.
