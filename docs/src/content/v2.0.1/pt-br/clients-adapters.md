# Clients e adapters

Um `DinocoClient` envolve exatamente um `Backend` primary e, opcionalmente, um conjunto de backends de réplica de leitura. Todo query builder recebe `&DinocoClient` por referência, então um client deve ser criado uma vez e compartilhado pela aplicação inteira — não reconstruído a cada request.

## Conexão gerada

O `dinoco/mod.rs` gerado exporta uma função `connect()` que lê as variáveis de ambiente nomeadas em `database_url` e `read_replicas`, e então constrói os adapters primary e de réplica do workspace ativo para você:

```rust
mod dinoco;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = dinoco::connect().await?;
    // Use &client em toda query.
    Ok(())
}
```

Use esse caminho para qualquer coisa que um config de schema comum já descreva, réplicas inclusive. Construir um client manualmente — o resto desta página — é para a minoria dos casos em que você precisa de controle explícito sobre a construção do adapter, ou de configuração de réplica que muda em runtime em vez de no deploy.

## PostgreSQL Direct

Direct usa o adapter PostgreSQL com pool:

```rust
use dinoco_engine::{Backend, DinocoClient, PostgresAdapter};

let adapter = PostgresAdapter::direct(
    "postgres://postgres:postgres@localhost:5432/app"
).await?;
let client = DinocoClient::new(Backend::Postgres(adapter));
```

O construtor é `async` porque ele estabelece e valida o pool de conexões imediatamente, em vez de fazer isso de forma preguiçosa no primeiro uso.

## PostgreSQL com PgBouncer

Use o adapter PgBouncer especificamente quando a URL que você está conectando é um endpoint do PgBouncer, não o PostgreSQL diretamente:

```rust
use dinoco_engine::{Backend, DinocoClient, PgBouncerAdapter};

let adapter = PgBouncerAdapter::new(
    "postgres://app:secret@pgbouncer:6432/app"
).await?;
let client = DinocoClient::new(Backend::PgBouncer(adapter));
```

Direct e PgBouncer compartilham exatamente o mesmo `SqlCompiler` PostgreSQL, então a semântica de queries e os tipos de migration gerados são idênticos entre os dois — só o tratamento de conexão muda.

## MySQL

```rust
use dinoco_engine::{Backend, DinocoClient, MySqlAdapter};

let adapter = MySqlAdapter::new("mysql://app:secret@localhost:3306/app");
let client = DinocoClient::new(Backend::Mysql(adapter));
```

Diferente dos adapters PostgreSQL, esse construtor não é `async` — o adapter conecta de forma preguiçosa, na primeira query que de fato executa.

## SQLite

```rust
use dinoco_engine::{Backend, DinocoClient, SqliteAdapter};

let adapter = SqliteAdapter::new("dinoco/database.sqlite")
    .await
    .map_err(anyhow::Error::msg)?;
let client = DinocoClient::new(Backend::Sqlite(adapter));
```

Use um caminho dentro de `dinoco/` para um banco local ao projeto — o adapter cria o arquivo se ele ainda não existir, ou abre se já existir.

## Réplicas de leitura

Construa cada réplica com a mesma família de adapter da primary, depois anexe todas juntas ao client:

```rust
use dinoco_engine::{Backend, DinocoClient, PostgresAdapter};

let primary = PostgresAdapter::direct(primary_url).await?;
let replica_a = PostgresAdapter::direct(replica_a_url).await?;
let replica_b = PostgresAdapter::direct(replica_b_url).await?;

let client = DinocoClient::new(Backend::Postgres(primary))
    .with_read_replicas(vec![
        Backend::Postgres(replica_a),
        Backend::Postgres(replica_b),
    ]);
```

Leituras se intercalam entre as réplicas anexadas usando um índice round-robin lock-free; com um vetor vazio, toda leitura simplesmente vai para a primary.

## Regras de execução

- Inserts, updates e deletes sempre rodam contra `client.backend` — a primary. Não existe configuração que roteie um write para uma réplica.
- `find_and_update` também sempre usa a primary, já que é fundamentalmente um write disfarçado de operação de ler-e-alterar.
- `find_first`, `find_many`, includes de relação e counts usam todos o caminho de leitura, que é elegível para réplica.
- `.read_in_primary()` força um find específico (e tudo que ele `.includes(...)`) para a primary, sobrescrevendo o roteamento de réplica só para essa chamada.
- A API de transaction por closure sempre se prende a uma única conexão física do backend primary durante toda sua duração.
- A decodificação de row é específica por adapter: SQLite, PostgreSQL com pool via deadpool, PostgreSQL nativo e MySQL implementam, cada um, sua própria conversão direta de row, então não existe uma camada de abstração genérica de "row" adicionando overhead no meio do caminho.

> [!WARNING]
> Construa um client uma vez e mantenha-o vivo durante o tempo de vida da sua aplicação — um `DinocoClient`, e o(s) pool(s) de conexão que ele possui, devem ter vida longa. Recriar adapters a cada request descarta o pooling completamente e coloca o setup de conexão direto no hot path.
