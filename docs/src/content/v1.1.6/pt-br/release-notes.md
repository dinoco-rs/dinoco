# Dinoco v1.1.6

O Dinoco v1.1.6 corrige valores de enum em `find_and_update`, `update` e `update_many`, e usa o suporte nativo de cada banco: tipos nomeados no PostgreSQL, colunas `ENUM` inline no MySQL e colunas `TEXT` com `CHECK` no SQLite. Agora, `find_and_update` retorna erro quando nenhuma row é afetada.

Esta versão também preserva grupos de siglas nos nomes de tabela gerados (`BusinessCNAE` vira `business_cnae`, enquanto `BusinessOffice` continua `business_office`) e faz o `migrate generate` mostrar as mudanças detectadas e exigir confirmação com `Y` antes de criar ou aplicar a migration e regenerar os models. Ela inclui ainda as melhorias de workspaces, migrations em runtime, Serde, transactions, relações, índices e query builders introduzidas durante a série v1.1.

## Conversão bidirecional de enums e strings

Enums gerados a partir do schema continuam usando variantes Rust idiomáticas em PascalCase. `.to_string()` retorna exatamente o valor original, enquanto `TryFrom<&str>`, `TryFrom<String>` e `FromStr` convertem esse valor de volta ao enum:

```dinoco
enum PaymentState {
    waiting_payment
    paid
}
```

```rust
PaymentState::WaitingPayment.to_string() // "waiting_payment"
PaymentState::Paid.to_string()           // "paid"

PaymentState::try_from("waiting_payment")?             // PaymentState::WaitingPayment
PaymentState::try_from("waiting_payment".to_string())? // PaymentState::WaitingPayment
"waiting_payment".parse::<PaymentState>()?             // PaymentState::WaitingPayment
```

Valores desconhecidos retornam erro em vez de causar `panic`. O `DinocoEnum` implementa todas as conversões usando o mapeamento `#[dinoco(value = "...")]` de cada variante, então enums derivados manualmente recebem o mesmo comportamento.

## API de endpoints para many-to-many implícito

Uma relação implícita como `Business.systems System[]` e `System.business Business[]` continua criando `_business_to_system` no SQL, mas deixa de gerar uma entity Rust pública `BusinessSystem`. O codegen adiciona duas chaves virtuais write-only:

- `Business.system_id: Option<SystemId>`;
- `System.business_id: Option<BusinessId>`.

Reads mantêm esses fields como `None`. Eles nunca viram `SELECT business.system_id` ou `SELECT system.business_id`. A navegação continua por `Business.systems` e `System.business`, e o loader de includes passa a fazer join do target pela tabela pivô real. Includes aninhados, filtros no model relacionado, ordenação, paginação por parent e counts usam o mesmo caminho consciente da pivô.

Preencha a chave virtual antes de `insert_into` ou em cada payload de `insert_many` para criar uma row na pivô por endpoint inserido. O mesmo field funciona em `update`, `update_many`, `find_and_update` e writes com returning:

```rust
let mut system = System::new(
    "ERP".to_string(),
    "Planejamento de recursos empresariais".to_string(),
);
system.business_id = Some(business_id);

dinoco::insert_into::<System>()
    .values(&system)
    .execute(&client)
    .await?;

dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system_id))
    .execute(&client)
    .await?;
```

No `insert_many`, preencha a chave virtual separadamente em cada item; `None` insere aquele endpoint sem vínculo. `disconnect` remove apenas a row correspondente da pivô. As mesmas chaves virtuais de insert e operações relacionais `connect`/`disconnect` agora funcionam dentro de batches transacionais e sofrem rollback atômico junto com writes escalares. Inserts transacionais por chave virtual exigem um ID de endpoint conhecido antes da execução, como UUID, Snowflake ou um ID fornecido pelo caller. Projetos existentes devem regenerar os models e substituir usos diretos das entities de pivôs implícitas; a tabela SQL e o histórico de migrations permanecem inalterados.

## Workspaces nomeados

Um único schema agora pode definir configurações de banco independentes em `config.workspace`:

```dinoco
config {
    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database = "postgresql"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

Passe `--workspace dev` ou `-w dev` nos comandos de migration e models. Quando a opção não é informada, a CLI pergunta qual workspace usar. Os artefatos ficam isolados em `dinoco/migrations/<workspace>/`, e a geração de models limpa a saída do workspace selecionado anteriormente antes de regenerá-la.

## Migrations SQLite opcionais em runtime

O código gerado agora incorpora as migrations do workspace selecionado e exporta `dinoco::migrate(&client)`. A função `connect()` gerada apenas conecta ao SQLite e cria o arquivo quando necessário; ela nunca aplica migrations da aplicação automaticamente:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;
```

Aplicações que precisam de mais controle podem usar `dinoco::runtime::run_migrations` com seus próprios valores `Migration`. As migrations de runtime são ordenadas, transacionais, idempotentes e protegidas por checksums.

## Enums gerados serializáveis

Os enums emitidos pelo codegen agora derivam `serde::Serialize` e `serde::Deserialize` pelo reexport público do Dinoco. Assim, os models gerados podem ser usados diretamente em payloads JSON e outros formatos compatíveis com Serde, sem adicionar derives manualmente.

## Compatibilidade com Axum e Send

`transactions(transaction).execute(&client)` agora retorna um future `Send`. Os adapters de resultado das transactions são `Send` e `Sync`, permitindo que comandos PostgreSQL e MySQL permaneçam emprestados durante chamadas assíncronas ao banco sem tornar o future do handler não-`Send`.

A cobertura de regressão em tempo de compilação agora verifica todas as operações CRUD públicas junto com a execução de transactions. Assim, builders de `find`, `count`, `insert`, `update`, `delete` e transactions permanecem compatíveis com os requisitos multithread dos handlers do Axum.

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

Transactions aceitam finds, counts, inserts, updates, deletes, `connect`/`disconnect` many-to-many implícitos e chaves virtuais preenchidas em `insert_into` e `insert_many`. SQLite e PostgreSQL também suportam `returning` e `find_and_update` dentro da batch. As limitações detalhadas de includes, payloads aninhados que não sejam many-to-many, inserts virtuais com autoincrement e operações returning no MySQL estão na página Transações.

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

O comportamento da v1.1.6 é coberto por testes de parser, migrations, query builder, adapters e documentação. A suíte de integração com bancos exercita SQLite, PostgreSQL e MySQL; o PgBouncer usa o mesmo compiler PostgreSQL e o mesmo fluxo de execução transacional.
