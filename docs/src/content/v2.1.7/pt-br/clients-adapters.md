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

Quando o crate é compilado para os próprios testes (`cfg(test)`), `connect()` devolve um banco de teste na memória, montado a partir do schema. `connect_database()` sempre conecta no banco real, e `connect_test()` sempre devolve o banco de teste. Veja [Testes](/pt-br/docs/orm/orm/testing#o-connect-gerado-nos-testes).

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

## TLS no PostgreSQL

Os dois adapters PostgreSQL criptografam a conexão de acordo com o `sslmode` da URL, com o significado que o libpq e o `psql` dão a ele:

| `sslmode` | Criptografada | Certificado do servidor |
| --- | --- | --- |
| `disable` | Nunca. | Não é verificado. |
| `prefer` (padrão) | Quando o servidor suporta TLS; sem TLS caso contrário. | Não é verificado. |
| `require` | Sempre. Falha quando o servidor não tem TLS. | Não é verificado. |
| `verify-ca` | Sempre. | Precisa ser assinado por uma raiz confiável. |
| `verify-full` | Sempre. | Precisa ser assinado por uma raiz confiável e emitido para o host. |

```rust
use dinoco_engine::PostgresAdapter;

let adapter = PostgresAdapter::direct(
    "postgres://app:secret@ep-example.us-east-2.aws.neon.tech/app?sslmode=verify-full&channel_binding=require"
).await?;
```

Bancos hospedados que só aceitam TLS, como o Neon, conectam com qualquer modo exceto `disable`. As raízes confiáveis são as da Mozilla, compiladas no binário via `webpki-roots`, então o mesmo build conecta no macOS, no Linux e em containers mínimos sem um repositório de certificados do sistema. Um servidor cujo certificado vem de uma CA privada, como Amazon RDS, Supabase ou um servidor local autoassinado, falha em `verify-ca` e `verify-full`: conecte nele com `require`. `sslrootcert` e certificados de cliente não são suportados.

A CLI conecta pelo mesmo adapter, então o `dinoco migrate` também segue o `sslmode` do `database_url`. O TLS roda sobre o `rustls`, com o provider de criptografia que a aplicação instalou como padrão do processo, ou o `ring` quando não há nenhum. `sslnegotiation=direct` (PostgreSQL 17+) também funciona.

> [!WARNING]
> `prefer` e `require` impedem que alguém leia o tráfego, mas não que um servidor se passe pelo seu. Use `verify-full` quando o certificado do servidor vem de uma CA pública. Caso contrário, adicione `channel_binding=require` com autenticação por senha (SCRAM): ele amarra o login à sessão TLS, então um servidor impostor não consegue completá-lo.

## MySQL

```rust
use dinoco_engine::{Backend, DinocoClient, MySqlAdapter};

let adapter = MySqlAdapter::new("mysql://app:secret@localhost:3306/app");
let client = DinocoClient::new(Backend::Mysql(adapter));
```

Diferente dos adapters PostgreSQL, esse construtor não é `async` — o adapter conecta de forma preguiçosa, na primeira query que de fato executa.

Conexões do pool mantêm seus prepared statements: elas não são resetadas ao voltar para o pool, a menos que a URL defina `reset_connection`. Veja [Configuração do MySQL](/pt-br/docs/orm/guide/configuration#mysql).

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

## Features do Cargo

Cada adapter fica atrás de uma feature do Cargo do `dinoco`, e todas vêm habilitadas por padrão. Uma aplicação que fala com um único banco pode desligar as outras para não compilar os drivers delas:

```toml
[dependencies]
dinoco = { version = "2.1.7", default-features = false, features = ["postgres"] }
```

| Feature | Habilita |
| --- | --- |
| `sqlite` | `SqliteAdapter` (SQLite embutido via `rusqlite`). |
| `postgres` | `PostgresAdapter` e `PgBouncerAdapter` (`tokio-postgres`). |
| `mysql` | `MySqlAdapter` (`mysql_async`). |
| `cli` | A ferramenta de linha de comando `dinoco`, o `TestAmbient` e o `dinoco::codegen`. Habilita todos os adapters. |

Sem `cli`, `TestAmbient::create` retorna um erro em vez de montar o banco de teste em memória, então o código `connect_test()` gerado continua compilando. Se você também depende do `dinoco_engine` diretamente, desligue as features padrão dele também e habilite os mesmos adapters.

## Regras de execução

- Inserts, updates e deletes sempre rodam contra `client.backend` — a primary. Não existe configuração que roteie um write para uma réplica.
- `find_and_update` também sempre usa a primary, já que é fundamentalmente um write disfarçado de operação de ler-e-alterar.
- `find_first`, `find_many`, includes de relação e counts usam todos o caminho de leitura, que é elegível para réplica.
- `.read_in_primary()` força um find específico (e tudo que ele `.includes(...)`) para a primary, sobrescrevendo o roteamento de réplica só para essa chamada.
- A API de transaction por closure sempre se prende a uma única conexão física do backend primary durante toda sua duração.
- A decodificação de row é específica por adapter: SQLite, PostgreSQL com pool via deadpool, PostgreSQL nativo e MySQL implementam, cada um, sua própria conversão direta de row, então não existe uma camada de abstração genérica de "row" adicionando overhead no meio do caminho.

> [!WARNING]
> Construa um client uma vez e mantenha-o vivo durante o tempo de vida da sua aplicação — um `DinocoClient`, e o(s) pool(s) de conexão que ele possui, devem ter vida longa. Recriar adapters a cada request descarta o pooling completamente e coloca o setup de conexão direto no hot path.
