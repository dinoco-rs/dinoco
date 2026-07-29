# Dinoco v1.0.8

O Dinoco v1.0.8 adiciona índices no schema, batches transacionais atômicas e filtros booleanos com agrupamento explícito. A release funciona em SQLite, PostgreSQL Direct, PgBouncer e MySQL, exceto quando uma limitação é indicada abaixo.

## Índices no schema

Adicione um índice não único a um field escalar ou enum com `@index` e, opcionalmente, escolha o nome físico com `map`:

```dinoco
model Post {
    id           Integer  @id @default(autoincrement())
    slug         String   @index
    published_at DateTime @index(map: "idx_post_publication")
}
```

Toda primary key e foreign key agora recebe um índice automático. A constraint da primary key fornece o índice físico, então o Dinoco não cria uma duplicata. Primary keys e relações compostas preservam a ordem das colunas, e tabelas pivô many-to-many implícitas recebem o índice da primary key composta mais um índice para cada foreign key.

`@@indexes([...])` e `@@uniques([...])` no model adicionam índices comuns e unique compostos, preservando a ordem. O compiler também exige exatamente uma declaração de primary key por model: um `@id` ou um `@@ids([...])`.

## Busca full-text

Fields String marcados com `@fulltext` expõem a condição gerada `.fulltext(termo)`. PostgreSQL cria e inspeciona um índice de expressão GIN, MySQL usa um índice `FULLTEXT` nativo e SQLite usa o fallback `LIKE '%termo%'` sem criar um índice B-tree ineficaz. Um model pode ter vários fields full-text, mas `@fulltext` não pode dividir o mesmo field com `@index`.

O engine de migrations planeja, aplica, reverte e inspeciona alterações de índices. Ele gera `CREATE INDEX` e `DROP INDEX`, detecta drift e mantém compatibilidade com snapshots criados antes de os índices serem registrados.

`@@fulltexts([...])` forma um documento pesquisável com vários fields String. PostgreSQL usa a mesma expressão concatenada no índice GIN e na query, MySQL usa a lista composta exata em `MATCH(...)` e SQLite une os fallbacks de substring com `OR`. Todo membro expõe `.fulltext(...)` e pesquisa o grupo completo.

O formatter move todas as declarações `@@...` para depois dos fields. A extensão do VS Code destaca e completa os novos atributos, completa fields dentro dos arrays, resolve definitions e references e aponta primary keys ausentes ou duplicadas.

## Transactions atômicas

Use `Transaction`, `transactions` ou o macro `transaction!` para executar builders heterogêneos, na ordem de inserção, em uma única conexão física com o primary:

```rust
let mut transaction = dinoco::Transaction::new();

transaction.push(
    dinoco::find_first::<Account>()
        .where_(|x| x.id.eq("account-1"))
);
transaction.push(
    dinoco::insert_into::<AccountSession>().values(&session)
);

let mut results = dinoco::transactions(transaction)
    .execute(&client)
    .await?;

let account: Option<Account> = results.take(0)?;
results.take::<()>(1)?;
```

A batch só faz commit depois que todas as operações terminam com sucesso e executa rollback em erros de SQL, constraint ou conversão de row. Os resultados mantêm seus tipos Rust normais e a ordem dos `push`. `Transcation` continua disponível como alias de compatibilidade com a grafia usada nos primeiros exemplos.

Transactions aceitam finds, counts, inserts planos, updates escalares e deletes. SQLite e PostgreSQL também suportam `returning` e `find_and_update` dentro da batch. As limitações detalhadas de includes, payloads de relações aninhadas, connect/disconnect e operações returning no MySQL estão na página Transações.

## Filtros complexos

`where_complex` monta expressões booleanas aninhadas com precedência explícita:

```rust
let account = dinoco::find_first::<Account>()
    .where_complex(|x, m| {
        m.or(
            m.and([
                x.id.eq("account-1"),
                x.name.eq("Matheus"),
            ]),
            m.not(x.disabled.eq(true)),
        )
    })
    .execute(&client)
    .await?;
```

O manipulador `m` oferece `and`, `or`, `or_many` e `not`; `x` é o `EntityWhere` gerado. O mesmo field gerado pode ser reutilizado em vários ramos. Quando um builder usa `where_complex`, todos os `where_` daquele builder são ignorados, independentemente da ordem das chamadas.

A API está disponível em `find_first`, `find_many`, `find_and_update` e nos find builders usados por relation includes. Os filtros complexos são preservados quando esses builders são executados dentro de uma transaction.

## Compatibilidade e verificação

O comportamento da v1.0.8 é coberto por testes de parser, migrations, query builder, adapters e documentação. A suíte de integração com bancos exercita SQLite, PostgreSQL e MySQL; o PgBouncer usa o mesmo compiler PostgreSQL e o mesmo fluxo de execução transacional.
