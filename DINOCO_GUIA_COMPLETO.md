# Dinoco — guia completo

> Guia monolítico do Dinoco **v1.3.3**, baseado no código e na documentação deste repositório. Ele explica o que o projeto é, como suas partes trabalham juntas, todas as funcionalidades públicas atuais e como usá-las. A pasta `_old/` é histórico e não descreve a API vigente.

## Sumário

- [Leitura rápida: o que é o Dinoco](#leitura-rápida-o-que-é-o-dinoco)
- [Como o Dinoco funciona internamente](#como-o-dinoco-funciona-internamente)
- [Mapa do repositório](#mapa-do-repositório)
- [O que é gerado no seu projeto](#o-que-é-gerado-no-seu-projeto)
- [Fluxo de trabalho recomendado](#fluxo-de-trabalho-recomendado)
- [Matriz de funcionalidades](#matriz-de-funcionalidades)
- [Limites e decisões importantes](#limites-e-decisões-importantes)
- [Desenvolvendo o próprio Dinoco](#desenvolvendo-o-próprio-dinoco)
- [Documentação funcional completa](#documentação-funcional-completa)
  - [Introdução](#cap-introduction)
  - [Início rápido](#cap-quickstart)
  - [Configuração](#cap-configuration)
  - [Organização do schema](#cap-schema-organization)
  - [Models e fields](#cap-models)
  - [Defaults e enums](#cap-defaults-enums)
  - [Índices e constraints](#cap-indexes)
  - [Relações](#cap-relations)
  - [Clients e adapters](#cap-clients-adapters)
  - [Transactions](#cap-transactions)
  - [Visão geral de queries](#cap-find)
  - [Find first](#cap-find-first)
  - [Find many](#cap-find-many)
  - [Select](#cap-select)
  - [Includes](#cap-includes)
  - [Count](#cap-count)
  - [Filtros](#cap-filters)
  - [Where complex](#cap-where-complex)
  - [Busca full-text](#cap-full-text-search)
  - [Insert](#cap-insert)
  - [Update](#cap-update)
  - [Find and update](#cap-find-and-update)
  - [Delete](#cap-delete)
  - [Migrations](#cap-migrations)
  - [Referência da CLI](#cap-cli)
  - [Extensão do VS Code](#cap-vscode)
  - [Exemplo completo](#cap-cookbook)
  - [Referência da API](#cap-api-reference)
  - [Notas da versão](#cap-release-notes)

## Leitura rápida: o que é o Dinoco

O Dinoco é um ORM assíncrono e orientado por schema para Rust. Em vez de escrever manualmente structs, nomes de colunas e relações em vários lugares, a aplicação descreve seu domínio em `dinoco/schema.dinoco`. A toolchain valida esse schema, compara o estado desejado com o banco, produz migrations SQL revisáveis e gera uma API Rust tipada.

O projeto trabalha com três bancos:

- PostgreSQL, em conexão Direct ou via PgBouncer;
- MySQL;
- SQLite.

No runtime, builders como `find_many::<User>()`, `insert_into::<User>()` e `update::<User>()` recebem helpers gerados para cada model. Isso faz o compilador Rust detectar nomes de fields inválidos, tipos incompatíveis e projeções incorretas antes da aplicação rodar. O SQL não fica escondido: ele é compilado pelo adapter selecionado e pode ser exibido com o logger.

A versão documentada neste repositório é a **1.3.3**. Todos os crates publicados, a extensão VS Code e o site de documentação compartilham essa versão.

## Como o Dinoco funciona internamente

Há dois fluxos complementares: o fluxo de desenvolvimento, que transforma o schema em migrations e código Rust, e o fluxo de runtime, que transforma builders tipados em SQL.

```mermaid
flowchart LR
    A[dinoco/schema.dinoco] --> B[Compiler]
    B --> C[AST validada]
    C --> D[CLI e planner]
    D --> E[Migrations up.sql/down.sql]
    D --> F[Codegen]
    F --> G[Models e connect/migrate em Rust]
    E --> H[(Banco)]
    G --> I[Builders tipados]
    I --> J[Query IR do engine]
    J --> K[Compiler SQL do adapter]
    K --> H
    H --> L[Row nativa]
    L --> M[Entity ou projeção]
```

O caminho completo é:

1. O parser, construído sobre Pest, lê a linguagem `.dinoco` e cria uma AST.
2. O resolver percorre imports, consolida os arquivos alcançáveis e valida config, models, enums, fields, índices, defaults e relações.
3. A CLI escolhe a configuração ou workspace, resolve variáveis de ambiente e conecta somente ao banco primary.
4. O planner introspecta o banco e compara o estado atual com o estado desejado. Para SQLite, usa também um banco shadow isolado para materializar e inspecionar o resultado pretendido.
5. A CLI mostra o plano, exige confirmação nas alterações e publica uma pasta de migration com `up.sql` e `down.sql`. Os artefatos gerados carregam checksum e o histórico é registrado no banco.
6. O codegen converte a AST em enums, models, helpers tipados e no módulo de conexão em `dinoco/`.
7. No runtime, os builders públicos viram uma representação de query independente de banco (`FindQuery`, `InsertQuery`, `UpdateQuery`, `DeleteQuery` e queries de relação/count).
8. O compiler SQL do adapter escolhido produz SQL e parâmetros próprios para PostgreSQL, MySQL ou SQLite.
9. O adapter executa a operação no pool correto. Reads podem alternar entre réplicas; writes e transactions usam o primary.
10. A row nativa do driver é decodificada diretamente no model ou na projeção gerada. Includes `one` usam join; includes `many` usam carregamento em batch e remontam as relações, evitando uma query por parent.

### O que é verificado em cada fase

| Fase | Responsabilidade principal |
| --- | --- |
| Compiler do schema | Sintaxe, nomes, tipos, imports, config, primary keys, relações, defaults, índices e regras específicas de cada recurso |
| Compiler Rust | Uso correto dos helpers gerados, tipos de filtros/updates, projections e estados obrigatórios de builders como `delete` |
| Planner de migration | Diferença entre schema atual e desejado, drift, histórico, compatibilidade e operações perigosas |
| Banco | Constraints, integridade referencial, atomicidade e semântica real do dialeto |
| Runtime | Seleção de primary/réplica, execução, decodificação, includes, returning, transaction e classificação portátil de erros comuns |

## Mapa do repositório

| Caminho | Papel |
| --- | --- |
| `crates/dinoco` | Fachada pública. Reexporta engine e derives, contém builders de CRUD, filtros, includes, counts, updates relacionais, IDs e API de transaction. O binário `dinoco` delega para a CLI. |
| `crates/dinoco_compiler` | Parser, AST, resolução multi-arquivo e validação semântica do schema. Também expõe `parse`, `compile` e `compile_file`. |
| `crates/dinoco_formatter` | Formatação canônica de config, enums, models, atributos e comentários. É usado pela extensão e pode ser usado como biblioteca/binário. |
| `crates/dinoco_codegen` | Geração de `dinoco/models/`, enums, helpers, módulo raiz, conexão e migrations SQLite embutidas. |
| `crates/dinoco_engine` | Representação de queries, valores, traits, adapters, compilers SQL, pools, introspecção, migrations, transactions e migrations SQLite em runtime. |
| `crates/dinoco_derives` | Procedural macros `Entity`, `EntityExtend`, `Extend` e `DinocoEnum`. Elas implementam os metadados e conversores usados pela API tipada. |
| `crates/dinoco_cli` | Comandos `init`, `migrate generate`, `migrate run` e `models generate`; seleção de workspace, UI interativa e gerenciamento de artefatos/histórico. |
| `vscode` | Extensão TypeScript e language server Rust para `.dinoco`: highlighting, diagnostics, completion, hover, navegação, rename, formatter e comandos da CLI. |
| `docs` | Site Next.js e conteúdo oficial em inglês e português. Os capítulos em português são consolidados neste arquivo. |
| `examples/axum` | CRUD Todo completo com Axum, SQLite e models gerados. |
| `examples/actix-web` | O mesmo CRUD com Actix Web. |
| `tests` | Testes unitários, de integração, compilação, adapters, migrations, codegen, formatter, LSP e cenários Docker. |
| `_old` | Implementações históricas preservadas para referência; não devem orientar código novo. |

## O que é gerado no seu projeto

Depois de `dinoco migrate generate` ou `dinoco models generate`, a estrutura normal é:

```text
dinoco/
├── schema.dinoco              # fonte de verdade escrita pela aplicação
├── mod.rs                     # connect() e, no SQLite, migrate()
├── models/
│   ├── mod.rs                 # exports e enums
│   └── <model>.rs             # entity e helpers tipados por model
└── migrations/
    └── <timestamp>_<nome>/
        ├── up.sql             # aplica a mudança
        └── down.sql           # reversão conservadora/revisável
```

Com workspaces, as migrations são isoladas em `dinoco/migrations/<workspace>/`. `schema.dinoco` e outros arquivos-fonte `.dinoco` pertencem à aplicação; `dinoco/models/` e `dinoco/mod.rs` são saída regenerável e não devem ser editados manualmente.

Os models gerados incluem:

- a struct persistida e seu `new()`;
- traits de entity, insert e decodificação dos três drivers;
- helpers `Where`, `OrderBy`, `Update`, `Include` e `Count`;
- conversão e Serde para enums;
- metadata de relations, includes e pivôs many-to-many;
- aliases de UUID e Snowflake coerentes também nas foreign keys;
- imports e derives extras declarados em `custom_derives`.

## Fluxo de trabalho recomendado

### Criar um projeto consumidor

```bash
cargo install dinoco --version 1.3.3
cargo add dinoco@1.3.3 dinoco_engine@1.3.3 anyhow
cargo add tokio --features macros,rt-multi-thread
dinoco init
```

Defina `DATABASE_URL`, edite `dinoco/schema.dinoco` e então execute:

```bash
dinoco migrate generate
```

Esse comando não significa apenas “gerar um arquivo”: ele valida, detecta drift e mudanças, apresenta o plano, pede confirmação, cria os artefatos, aplica a migration e regenera os models. Em outro ambiente, onde os artefatos já existem, use:

```bash
dinoco migrate run
```

Se somente o Rust gerado precisa ser atualizado:

```bash
dinoco models generate
```

Quando o schema possui workspaces, acrescente `--workspace <nome>` ou `-w <nome>` a esses três comandos.

### Usar na aplicação

```rust
mod dinoco;

use ::dinoco::{find_many, insert_into};
use dinoco::{connect, User};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new(
        "ana@example.com".to_string(),
        "Ana".to_string(),
    );
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>()
        .where_(|user| user.email.ends_with("@example.com"))
        .order_by(|user| user.name.asc())
        .execute(&client)
        .await?;

    println!("{} usuário(s)", users.len());
    Ok(())
}
```

Em SQLite, `connect()` só abre o banco. A aplicação decide se executa as migrations embutidas:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;
```

## Matriz de funcionalidades

| Área | Funcionalidades atuais |
| --- | --- |
| Schema | Config, workspaces, imports globais e nomeados, models, enums, scalars, optionalidade, defaults, IDs, relações, tabela customizada e custom derives |
| Banco | PostgreSQL Direct, PgBouncer, MySQL e SQLite; primary mais réplicas de leitura |
| Leitura | `find_first`, `find_many`, projeções, ordenação, paginação, primary forçado, includes aninhados e count |
| Filtros | Igualdade, desigualdade, comparações, batch, null, texto, range, full-text e árvores AND/OR/NOT |
| Escrita | Insert único/em lote, update único/em lote, update atômico com retorno, operações numéricas, delete único/em lote e returning tipado |
| Relações | One-to-one, one-to-many, many-to-one, many-to-many implícito ou explícito, relações repetidas/nomeadas, self relations e ações referenciais |
| Consistência | Transactions em uma conexão primary, rollback automático e erros tipados/classificação de constraints |
| Migrations | Introspecção, diff, shadow SQLite, drift, baseline legado, checksums, `up.sql`/`down.sql`, execução pendente e runtime SQLite opcional |
| Ferramentas | CLI, formatter, codegen, language server e extensão VS Code |

## Limites e decisões importantes

- A API desta versão não inclui as APIs experimentais antigas `#[insertable]`, create structs separadas, `with_relation`, reset do banco, restore de schema, filas ou cache.
- Fields de navegação singular são sempre opcionais (`Relation?`) porque `None` também representa “relação não carregada”; a foreign key escalar local pode continuar obrigatória.
- Uma pivô many-to-many implícita existe no banco, mas não vira entity Rust pública. IDs virtuais servem apenas para escrita e permanecem `None` em leituras.
- Includes `many` evitam N+1 com batch; `take` e `skip` são aplicados por parent. Includes `one` usam left join.
- `where_complex` substitui os `where_` simples já definidos no mesmo builder; não os combina.
- Writes, `find_and_update` e transactions sempre usam o primary. Finds comuns alternam réplicas em round-robin quando configuradas.
- Migrations devem ser revisadas. Operações destrutivas ou reversões podem exigir tratamento manual; o planner prefere recusar cenários ambíguos a apagar dados silenciosamente.
- Migrations embutidas em runtime são suportadas atualmente apenas no SQLite.
- `connect()` não aplica migrations automaticamente.
- Credenciais de banco e node ID Snowflake devem vir de `env(...)`; valores literais são rejeitados pelo compiler do schema.
- O logger imprime SQL e parâmetros, que podem conter dados sensíveis.

## Desenvolvendo o próprio Dinoco

O workspace usa Rust edition 2024. Para validar os crates e a suíte principal:

```bash
cargo check --workspace
cargo test -p dinoco_tests
```

Alguns testes de adapters dependem de serviços PostgreSQL/MySQL e infraestrutura Docker/Podman; execute-os somente com esses serviços disponíveis. Para trabalhar no site:

```bash
cd docs
yarn install
yarn dev
yarn check
```

Para a extensão:

```bash
cd vscode
npm install
npm run compile
```

`npm run compile` valida TypeScript, empacota `extension.ts` e gera os binários do language server. Também existem os scripts `build:binaries:linux`, `build:binaries:macos`, `build:binaries:windows` e `build:binaries:all`.

Os exemplos web expõem o mesmo CRUD de Todo. Depois de instalar a CLI:

```bash
cd examples/axum
export DATABASE_URL=axum-example.sqlite
dinoco migrate generate
cargo run
```

O Axum sobe em `127.0.0.1:3000`. Para Actix, use `examples/actix-web`, o arquivo `actix-example.sqlite` e a porta `3001`.

## Documentação funcional completa

Daqui em diante estão todos os capítulos funcionais oficiais em português da v1.3.3, reunidos em ordem de aprendizado. Links entre páginas apontam para o site oficial quando necessário.

<a id="cap-introduction"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/introduction.md -->

## Dinoco v1.3.3

O Dinoco é um ORM orientado por schema para Rust. Você descreve o banco uma vez em `dinoco/schema.dinoco`; a ferramenta valida essa descrição, produz migrations específicas para cada adapter, gera as entities em Rust e oferece uma API tipada para consultá-las.

A proposta é manter o comportamento do SQL visível, mas levar nomes de tabelas, colunas, tipos e caminhos de relações para o compilador do Rust verificar.

### O que o Dinoco entrega

- Um schema para PostgreSQL, MySQL e SQLite.
- Models gerados com `Entity`, fields tipados, relações e um construtor `new()` prático.
- Builders assíncronos de find, count, insert, update, update atômico e delete.
- Projeções customizadas com `EntityExtend` e `.select::<T>()`.
- Includes `many` em batch e includes `one` por left join, evitando N+1.
- PostgreSQL Direct, PgBouncer, MySQL, SQLite e réplicas de leitura.
- Planejamento de migrations por introspecção e tabelas de validação isoladas.
- Formatter e extensão inteligente para schemas `.dinoco` no VS Code.

Cada adapter converte sua row nativa diretamente para as entities. O SQL também é produzido pelo `SqlCompiler` do adapter, sem uma row genérica no meio do caminho.

### Como o fluxo se encaixa

O ciclo normal tem quatro passos:

1. Edite `dinoco/schema.dinoco`.
2. Rode `dinoco migrate generate` para validar e planejar a alteração.
3. Revise `up.sql` e `down.sql`; a CLI aplica a migration confirmada.
4. Importe os models em `dinoco/models/` e use a API tipada.

```rust
mod dinoco;

use ::dinoco::{find_many, insert_into};
use dinoco::{connect, User};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new(
        "ana@example.com".to_string(),
        "Ana".to_string(),
    );
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>()
        .includes(|x| x.posts())
        .execute(&client)
        .await?;

    println!("{} users", users.len());
    Ok(())
}
```

### A superfície da v1.3.3

A v1.3.3 mantém um fluxo público pequeno e previsível. A CLI inicializa o projeto, gera e executa migrations e regenera models. O runtime expõe builders como `find_many`, `insert_into`, `update_many` e `delete`.

APIs experimentais antigas, como `#[insertable]`, structs separadas de create, `with_relation`, reset de banco, restore de schema, filas e cache, não fazem parte desta versão. O próprio model é o payload de insert, inclusive com suas relações.

---

<a id="cap-quickstart"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/quickstart.md -->

## Início rápido

Este guia começa em um crate binário vazio e termina com um banco migrado, um insert e um find tipado.

### Pré-requisitos

Use uma versão stable atual do Rust. Você também precisa de PostgreSQL, MySQL ou um diretório gravável para SQLite. O exemplo usa PostgreSQL Direct.

### 1. Instale o Dinoco

```bash
cargo install dinoco --version 1.3.3
cargo add dinoco@1.3.3 dinoco_engine@1.3.3 anyhow
cargo add tokio --features macros,rt-multi-thread
```

`dinoco` contém os derives e methods. O módulo de conexão gerado usa `dinoco_engine`. `tokio` e `anyhow` completam o fluxo assíncrono.

### 2. Inicialize o projeto

Na raiz do projeto Cargo, execute:

```bash
dinoco init
```

Escolha `postgresql` e depois `direct`. A CLI cria `dinoco/schema.dinoco` e `dinoco/migrations/`. Coloque a URL no ambiente:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/my_app"
```

URLs literais são rejeitadas pelo compiler do schema para que credenciais não acabem no repositório.

### 3. Defina o schema

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = []
}

enum Role {
    member
    admin
}

model User {
    id         String   @id @default(uuid())
    email      String   @unique
    name       String
    role       Role     @default(member)
    created_at DateTime @default(now())
}
```

O `User::new` gerado pede somente `email` e `name`. Os outros fields têm default ou geração automática.

### 4. Gere e execute a migration

```bash
dinoco migrate generate
```

A CLI compila e valida o schema, inspeciona o banco atual, verifica o resultado pretendido em tabelas isoladas `dinoco_migration_test_*` e cria:

```text
dinoco/migrations/<timestamp>_<nome>/
  up.sql
  down.sql
```

Ela aplica o `up.sql` confirmado e gera `dinoco/models/` e `dinoco/mod.rs`. Em outro ambiente, execute migrations pendentes com:

```bash
dinoco migrate run
```

### 5. Use o client gerado

```rust
mod dinoco;

use ::dinoco::{find_first, insert_into};
use dinoco::{connect, User};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;
    let user = User::new(
        "ana@example.com".to_string(),
        "Ana".to_string(),
    );

    insert_into::<User>()
        .values(&user)
        .execute(&client)
        .await?;

    let saved = find_first::<User>()
        .where_(|x| x.email.eq("ana@example.com"))
        .execute(&client)
        .await?;

    println!("{saved:#?}");
    Ok(())
}
```

`.values(&user)` usa empréstimo e não move `user`. `find_first` retorna `Result<Option<User>>`; não encontrar uma row é `None`, não um erro.

---

<a id="cap-configuration"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/configuration.md -->

## Configuração

O bloco `config` informa à CLI e ao codegen qual dialeto SQL e estratégia de conexão o projeto usa.

### Bloco de configuração

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = [env("DATABASE_REPLICA_1")]
}
```

`database` aceita `postgresql`, `mysql` ou `sqlite`. Para PostgreSQL, `connection` aceita `direct` ou `pgbouncer` e assume Direct quando não informado.

### Imports de arquivos do schema

O arquivo principal `dinoco/schema.dinoco` pode carregar arquivos completos do schema sem listar o nome de cada model e enum. Declare os caminhos no bloco `config` principal:

```dinoco
config {
    imports = ["entities/accounts.dinoco", "entities/businesses.dinoco"]
    database = "postgresql"
    database_url = env("DATABASE_URL")
}
```

Todos os models e enums declarados diretamente em cada arquivo listado ficam visíveis para o `schema.dinoco`. Os caminhos são relativos ao schema principal, devem apontar para arquivos `.dinoco` e não podem estar duplicados nem formar ciclos. `imports` é uma propriedade global: ao usar workspaces, mantenha-a diretamente em `config`, fora dos blocos de cada workspace.

Somente o arquivo principal `schema.dinoco` aceita `config.imports`. Arquivos filhos continuam usando imports explícitos e nomeados quando dependem de declarações de outro arquivo:

```dinoco
import { AccountStatus } from "../enums.dinoco"

model Account {
    id     String        @id @default(uuid())
    status AccountStatus
}
```

Um arquivo filho enxerga suas próprias declarações e os símbolos nomeados em seus próprios imports; ele não herda o escopo de imports do arquivo principal. Assim, o arquivo de entrada fica compacto sem tornar implícitas as dependências entre os arquivos filhos.

Consulte [Organização do schema](https://docs.dinoco.io/v1.3.3/guide/schema-organization) para estruturas de projeto, imports nomeados, regras de escopo, validação de caminhos e um exemplo multi-arquivo completo.

### Custom derives

`custom_derives` adiciona macros derive Rust globais a todos os enums ou structs de model gerados:

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    custom_derives = [
        {
            into   = "enum"
            derive = "ZodSchema"
            import = "use zod_rs::prelude::*;"
        },
        {
            into   = "struct"
            derive = "Validate"
            import = "use validator::Validate;"
        }
    ]
}
```

Cada objeto exige `into`, `derive` e `import`. O alvo deve ser `"enum"` ou `"struct"`, o derive deve ser um caminho Rust válido e o import deve ser uma declaração `use ...` em uma única linha. Mantenha `custom_derives` no nível principal de `config`, fora dos workspaces. As crates que fornecem as macros continuam sendo dependências da aplicação. Consulte [Organização do schema](https://docs.dinoco.io/v1.3.3/guide/schema-organization#custom-derives) para a saída gerada e todas as regras de validação.

### Workspaces

Use `workspace` quando o mesmo schema de models precisa operar com mais de uma configuração de banco:

```dinoco
config {
    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database = "postgresql"
            connection = "pgbouncer"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

Cada workspace contém uma configuração completa, incluindo seus próprios `read_replicas` opcionais. Não misture as propriedades de banco no nível principal de `config` com o bloco `workspace`. Selecione o ambiente com `--workspace nome` ou `-w nome`; sem essa opção, a CLI abre uma seleção interativa. As migrations ficam isoladas em `dinoco/migrations/<workspace>/`.

O bloco `workspace` deve conter pelo menos um workspace nomeado, e cada workspace deve declarar `database` e `database_url`.

### Variáveis de ambiente

`database_url`, cada item de `read_replicas` e `snowflake_node_id` aceitam somente `env("NOME")`. O compiler rejeita valores literais.

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
export DATABASE_REPLICA_1="postgres://reader:secret@replica:5432/app"
```

O `database_url` é resolvido pela CLI e pelo `connect()` gerado. O `connect()` também constrói automaticamente os adapters de réplica declarados no workspace selecionado.

### PostgreSQL

Direct usa o pool PostgreSQL comum:

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

PgBouncer aponta para o endpoint do proxy:

```dinoco
connection = "pgbouncer"
```

Os dois usam o mesmo compiler PostgreSQL; muda a estratégia de conexão, não o schema.

### Logger de queries e pool Direct

`with_logger` habilita a exibição do SQL e dos parâmetros nos clients gerados. O default é `false`, e a opção pode ser declarada em uma configuração comum ou dentro de cada workspace:

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    with_logger = true
    min_connection = 2
    max_connection = 10
}
```

`min_connection` e `max_connection` estão disponíveis somente para PostgreSQL Direct. Os defaults são `2` e `10`; ambos devem ser inteiros positivos, e o mínimo não pode ser maior que o máximo. O Dinoco abre imediatamente o mínimo configurado e limita o pool ao máximo. Os parâmetros podem conter dados da aplicação, então habilite o logger somente onde essa saída for apropriada.

### MySQL

```dinoco
config {
    database = "mysql"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

Uma URL típica é `mysql://usuario:senha@localhost:3306/banco`.

### SQLite

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

Para SQLite, a URL é o caminho do arquivo. Caminhos relativos partem da pasta do projeto Dinoco.

```bash
export DATABASE_URL="database.sqlite"
```

### Réplicas de leitura

O `connect()` gerado resolve e constrói todos os adapters de réplica declarados pelo workspace selecionado. O `DinocoClient` intercala finds entre eles em round-robin. Sem réplicas, lê no primary. Writes sempre usam o primary, `.read_in_primary()` força um find e seus includes a lerem nele, e `find_and_update` sempre executa no primary.

Os comandos de migration da CLI nunca usam réplicas. `migrate generate` e `migrate run` conectam somente ao `database_url` primary do workspace selecionado; as réplicas devem acompanhar o primary pelo mecanismo de replicação do próprio banco.

### IDs Snowflake

`snowflake()` exige um node ID vindo do ambiente:

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_replicas = []
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}
```

Cada processo concorrente deve usar um ID distinto; repetir o mesmo node ID pode quebrar a garantia de unicidade.

---

<a id="cap-schema-organization"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/schema-organization.md -->

## Organização do schema

Schemas Dinoco podem ser divididos em vários arquivos `.dinoco`. O `dinoco/schema.dinoco` continua sendo o ponto de entrada do projeto: ele contém `config`, carrega arquivos completos com `config.imports` e pode configurar derives Rust adicionais. Os arquivos filhos mantêm suas dependências explícitas com declarações nomeadas `import { ... } from "..."`.

### Estrutura de projeto recomendada

```text
dinoco/
  schema.dinoco
  entities/
    account.dinoco
    business.dinoco
  shared/
    enums.dinoco
```

Somente `schema.dinoco` pode declarar `config`. Models e enums podem ser declarados no arquivo principal ou em qualquer arquivo filho alcançável.

`entities/` e `shared/` são diretórios de código-fonte escolhidos pela aplicação. Não armazene arquivos-fonte `.dinoco` em `dinoco/models/` nem em `dinoco/migrations/`: `models/` é substituído pelo codegen, enquanto `migrations/` é reservado para o histórico gerenciado de migrations SQL.

### Imports do arquivo principal

Use `config.imports` no arquivo principal quando ele precisar carregar todos os models e enums declarados diretamente em outro arquivo:

```dinoco
config {
    imports = [
        "entities/account.dinoco",
        "entities/business.dinoco",
        "shared/enums.dinoco"
    ]

    database = "postgresql"
    database_url = env("DATABASE_URL")
}
```

Não é necessário listar símbolos. Assim, o ponto de entrada permanece pequeno mesmo com muitas declarações. `imports` é uma propriedade global do projeto; ao usar `workspace`, declare-a diretamente dentro de `config`, nunca dentro de um workspace individual:

O valor de `imports` deve ser um array. O array pode estar vazio, mas cada item presente deve ser um caminho string entre aspas e não vazio. Identificadores, números, booleanos, objetos, arrays aninhados e valores `env(...)` são rejeitados.

```dinoco
config {
    imports = ["entities/account.dinoco"]

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

`config.imports` está disponível somente no `schema.dinoco` principal. Um arquivo filho não pode declarar seu próprio bloco `config`.

### Imports nomeados

Arquivos filhos usam imports nomeados quando referenciam models ou enums declarados em outro arquivo:

```dinoco
import { AccountType, BusinessStatus } from "../shared/enums.dinoco"

model Account {
    id           String      @id @default(uuid())
    account_type AccountType
}

model Business {
    id     String         @id @default(uuid())
    status BusinessStatus
}
```

Cada símbolo nomeado deve estar declarado diretamente no arquivo de destino. Separe vários símbolos por vírgulas; uma vírgula final também é aceita. Imports nomeados também funcionam no arquivo principal, embora `config.imports` normalmente seja mais conciso nele.

### Escopo de cada arquivo

Cada arquivo possui um escopo de tipos independente:

| Arquivo | Declarações visíveis |
| --- | --- |
| `schema.dinoco` principal | Suas declarações, imports nomeados e todas as declarações diretas dos arquivos em `config.imports` |
| Arquivo filho `.dinoco` | Suas declarações e somente os símbolos presentes em seus imports nomeados |
| Arquivo importado por um filho | Não é reexportado automaticamente para o parent do filho nem para o arquivo principal |

Por exemplo, se `entities/business.dinoco` importar `BusinessStatus`, esse enum fica visível dentro de `business.dinoco`. Um model declarado em `schema.dinoco` só pode usar o enum quando `shared/enums.dinoco` também estiver em `config.imports`, ou quando o enum for importado nominalmente no arquivo principal.

O compiler ainda consolida toda a árvore de imports alcançável para validação, migrations e codegen. Os escopos isolados impedem que um arquivo compile apenas porque outro arquivo não relacionado importou o tipo ausente.

### Validação dos imports

As duas formas de import seguem as mesmas regras de caminho:

- caminhos são relativos ao arquivo que declara o import;
- caminhos devem ser relativos e terminar em `.dinoco`;
- segmentos `.` e `..` são normalizados antes da detecção de duplicidade;
- arquivos ausentes geram erro de compilação;
- imports circulares são permitidos: cada arquivo é parseado e consolidado uma única vez, inclusive quando models em arquivos diferentes se relacionam entre si;
- importar o mesmo arquivo resolvido duas vezes em um arquivo é rejeitado;
- símbolos duplicados, símbolos nomeados inexistentes e conflitos com declarações locais são rejeitados;
- sempre que possível, os diagnósticos indicam o arquivo e a linha de origem.

A CLI inicia a compilação em `dinoco/schema.dinoco`. A API de compilação que recebe apenas uma string rejeita imports porque não possui um caminho-base para resolver os arquivos.

### Custom derives

Use `config.custom_derives` para adicionar macros derive a todos os enums gerados ou a todos os structs de model gerados:

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")

    custom_derives = [
        {
            into   = "enum"
            derive = "ZodSchema"
            import = "use zod_rs::prelude::*;"
        },
        {
            into   = "struct"
            derive = "Validate"
            import = "use validator::Validate;"
        }
    ]
}
```

Assim como `imports`, `custom_derives` é global. Ele fica diretamente no bloco `config` principal e fora dos blocos de workspace individuais.

### Campos de custom derive

Cada item de `custom_derives` é um objeto com três propriedades string obrigatórias:

| Propriedade | Valor aceito | Efeito |
| --- | --- | --- |
| `into` | `"enum"` ou `"struct"` | Seleciona todos os enums gerados ou todos os structs de model gerados |
| `derive` | Um caminho Rust como `ZodSchema` ou `crate::ZodSchema` | Adiciona a macro ao `#[derive(...)]` gerado |
| `import` | Uma única declaração Rust `use ...`, em uma linha | Adiciona o import da macro ao módulo Rust gerado |

As três chaves são obrigatórias em cada objeto. Um `{}` vazio, um objeto com apenas uma ou duas chaves ou um objeto com valor vazio/não-string é rejeitado; o Dinoco nunca aplica um custom derive parcialmente especificado. Propriedades desconhecidas ou repetidas, caminhos Rust inválidos e imports que não sejam uma declaração `use` também são rejeitados durante a compilação do schema.

A crate que fornece o custom derive deve estar nas dependências da aplicação. O Dinoco não instala essa crate nem verifica se todos os fields gerados implementam os traits exigidos pela macro. Como cada alvo é global, use um derive somente quando ele for válido para todos os enums gerados ou para todos os models gerados.

### Saída Rust gerada

Imports e derives de enum são emitidos em `dinoco/models/mod.rs`. Imports e derives de struct são emitidos em cada arquivo de model gerado. Declarações de import repetidas aparecem uma única vez por alvo, e derives com o mesmo segmento final do caminho Rust são deduplicados, inclusive derives já fornecidos pelo Dinoco, como `Clone` ou `Debug`.

Por exemplo, uma configuração de enum com `derive = "ZodSchema"` produz uma saída equivalente a:

```rust
use zod_rs::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, ZodSchema)]
pub enum BusinessStatus {
    Active,
    Inactive,
}
```

Os arquivos gerados são substituídos quando os models são gerados novamente. Por isso, configure os derives no schema em vez de editar o Rust gerado. O `dinoco/mod.rs` gerado também começa com `#![allow(unused)]`, suprimindo warnings apenas nesse módulo e nos arquivos importados por ele.

### Exemplo completo

O `dinoco/schema.dinoco` principal fica focado nas configurações globais do projeto:

```dinoco
config {
    imports = ["entities/account.dinoco", "shared/enums.dinoco"]

    database = "sqlite"
    database_url = env("DATABASE_URL")

    custom_derives = [
        {
            into   = "enum"
            derive = "ZodSchema"
            import = "use zod_rs::prelude::*;"
        }
    ]
}
```

`dinoco/entities/account.dinoco` declara sua dependência explicitamente:

```dinoco
import { AccountType } from "../shared/enums.dinoco"

model Account {
    id           String      @id @default(uuid())
    email        String      @unique
    account_type AccountType @default(OWNER)
}
```

`dinoco/shared/enums.dinoco` contém o enum:

```dinoco
enum AccountType {
    OWNER
    MEMBER
}
```

Execute `dinoco models generate` ou o fluxo normal de migrations depois de alterar qualquer arquivo da árvore de imports.

---

<a id="cap-models"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/models.md -->

## Models e fields

Um model descreve uma tabela e uma struct Rust gerada. Os nomes continuam visíveis na API, então o schema é o melhor ponto para entender o domínio.

### Declare um model

```dinoco
model User {
    id         String   @id @default(uuid())
    email      String   @unique
    display    String?
    active     Boolean  @default(true)
    score      Float
    created_at DateTime @default(now())
}
```

Isso gera uma `Entity` chamada `User`, com fields públicos e conversão de row para cada adapter.

### Regra da primary key

Todo model deve declarar exatamente uma primary key:

- use um único `@id` em field para uma chave de uma coluna; ou
- use um único `@@ids([...])` para uma chave composta.

Um `@@ids` composto conta como uma declaração, independentemente da quantidade de fields. Um model sem nenhuma das formas falha na compilação do schema. Dois `@id`, dois `@@ids` ou a combinação de `@id` com `@@ids` também falham.

Fields da primary key devem ser scalars ou enums obrigatórios. A ordem de `@@ids` é preservada na constraint e no índice automático do banco.

### Tipos escalares

| Dinoco | Rust | SQLite | PostgreSQL | MySQL |
| --- | --- | --- | --- | --- |
| `String` | `String` | `TEXT` | `TEXT` | `VARCHAR(255)` |
| `Boolean` | `bool` | `BOOLEAN` | `BOOLEAN` | `TINYINT(1)` |
| `Integer` | `i64` | `INTEGER` | `BIGINT` | `BIGINT` |
| `Float` | `f64` | `REAL` | `DOUBLE PRECISION` | `DOUBLE PRECISION` |
| `DateTime` | `DateTime<Utc>` | `DATETIME` | `TIMESTAMP` | `TIMESTAMP` |
| `Date` | `NaiveDate` | `DATE` | `DATE` | `DATE` |
| `Json` | `serde_json::Value` | `BLOB` | `JSONB` | `JSON` |

IDs gerados e suas foreign keys preservam wrappers descritivos:

| Declaração | Rust gerado |
| --- | --- |
| `String @default(uuid())` | `dinoco::Uuid` |
| `Integer @default(snowflake())` | `dinoco::Snowflake` |
| FK `String` referenciando UUID | `dinoco::Uuid` |
| FK `Integer` referenciando Snowflake | `dinoco::Snowflake` |

### Fields opcionais e listas

`String?` vira `Option<String>` e uma relação `Profile?` vira `Option<Profile>`. O sufixo `[]` representa uma lista de relação e gera `Vec<T>`; ele não é uma coluna SQL de array.

```dinoco
display_name String?
posts        Post[]
```

### Atributos de field

- `@id` marca a primary key.
- `@unique` adiciona unicidade.
- `@index` cria um índice não único para o field.
- `@fulltext` cria a capability e o índice full-text de um field String.
- `@default(...)` declara um literal, enum ou valor gerado.
- `@relation(...)` define chaves, nome e ações referenciais.

### Atributos do model

Atributos de model atuam sobre grupos ordenados de fields:

| Atributo | Finalidade |
| --- | --- |
| `@@ids([tenant_id, id])` | Primary key composta |
| `@@uniques([tenant_id, slug])` | Unicidade composta |
| `@@indexes([tenant_id, created_at])` | Índice comum composto |
| `@@fulltexts([title, body])` | Índice full-text composto |
| `@@table_name("audit_users")` | Nome físico da tabela |

`@@ids`, `@@uniques`, `@@indexes` e `@@fulltexts` recebem um array não vazio de fields existentes, scalars ou enums, sem repetição. Todos os fields de `@@fulltexts` devem ser `String` ou `String?`.

O formatter sempre move os atributos de model para depois de todos os fields, separados por uma linha vazia. Assim, todo model mantém uma estrutura estável com fields primeiro.

Consulte [Índices e constraints](https://docs.dinoco.io/v1.3.3/guide/indexes) para índices simples e compostos, unicidade e índices automáticos de primary e foreign keys.

### A função new gerada

`pub fn new(...) -> Self` recebe somente escalares obrigatórios sem default nem auto-geração. Opcionais começam em `None`, listas em `Vec::new()` e defaults recebem seu valor inicial.

```dinoco
model User {
    id      String  @id @default(uuid())
    email   String
    name    String
    enabled Boolean @default(true)
    bio     String?
    posts   Post[]
}
```

```rust
let user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
```

### Fields gerados para many-to-many implícito

Uma relação many-to-many implícita mantém a tabela pivô interna e não gera uma entity pública para ela. Em vez disso, `Post` recebe `tag_id: Option<TagId>` e `Tag` recebe `post_id: Option<PostId>`, ambos write-only.

Preencher um desses fields antes de `insert_into` ou em cada item de `insert_many` cria o vínculo correspondente depois que o endpoint é inserido:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.post_id = Some(post.id.clone());

dinoco::insert_into::<Tag>()
    .values(&tag)
    .execute(&client)
    .await?;
```

O field virtual não é uma coluna de `tag` e sempre volta como `None`. Para endpoints existentes, use as operações de update `connect` e `disconnect` no mesmo field gerado. Consulte [Many-to-many implícito](https://docs.dinoco.io/v1.3.3/guide/relations#many-to-many-implícito) para o contrato completo.

### Arquivos gerados

```text
dinoco/
  mod.rs
  models/
    mod.rs
    user.rs
    post.rs
```

`dinoco/mod.rs` exporta models e `connect()`. Enums ficam compactos em `models/mod.rs` usando `DinocoEnum`, e cada model tem seu próprio arquivo.

O módulo raiz gerado começa com `#![allow(unused)]`, suprimindo warnings do módulo gerado e dos arquivos que ele importa. Derives adicionais e seus imports devem ser configurados em `config.custom_derives`; consulte [Organização do schema](https://docs.dinoco.io/v1.3.3/guide/schema-organization#custom-derives).

---

<a id="cap-defaults-enums"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/defaults-enums.md -->

## Defaults e enums

Use defaults no schema para regras do banco e identificadores gerenciados pela lib. Esses fields também deixam de aparecer nos parâmetros de `new()`.

### Defaults literais

```dinoco
model Feature {
    id      Integer @id @default(autoincrement())
    name    String
    enabled Boolean @default(false)
    weight  Float   @default(1.0)
}
```

O valor deve ser compatível com o tipo. A migration conserva o default no banco, inclusive para inserts feitos fora do Dinoco.

### Valores gerados

As funções suportadas são `autoincrement()` em `Integer`, `uuid()` em `String`, `snowflake()` em `Integer` e `now()` em `DateTime` ou `Date`. Usar uma combinação inválida falha na compilação do schema.

### UUID

```dinoco
id String @id @default(uuid())
```

O Rust usa `dinoco::Uuid`. O valor é criado antes dos inserts relacionados, permitindo propagar a chave para relações aninhadas.

### Snowflake

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_replicas = []
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

model Event {
    id Integer @id @default(snowflake())
}
```

O field vira `dinoco::Snowflake`, baseado em `i64`.

### Autoincremento

```dinoco
id Integer @id @default(autoincrement())
```

O banco cria o inteiro. O adapter recupera a chave quando o insert precisa retorná-la ou vinculá-la a uma relação aninhada.

### Enums

```dinoco
enum Role {
    USER
    ADMIN
}

model User {
    id   Integer @id @default(autoincrement())
    role Role    @default(USER)
}
```

As variantes Rust usam PascalCase, enquanto o valor persistido mantém a grafia do schema. O codegen gera um enum compacto, compatível com Serde, usando `DinocoEnum`:

```rust
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    dinoco::serde::Serialize,
    dinoco::serde::Deserialize,
    dinoco::DinocoEnum,
)]
#[serde(crate = "::dinoco::serde")]
pub enum Role {
    #[default]
    #[dinoco(value = "USER")]
    #[serde(rename = "USER")]
    User,

    #[dinoco(value = "ADMIN")]
    #[serde(rename = "ADMIN")]
    Admin,
}
```

`DinocoEnum` gera as conversões usadas por `DinocoValue`, SQLite, PostgreSQL e MySQL. O `mod.rs` não precisa mais conter implementações manuais repetidas para cada adapter.

Cada variante também recebe `#[serde(rename = "...")]`, então o Serde serializa e desserializa exatamente o valor do banco, em vez do nome PascalCase da variante Rust. O enum gerado implementa `Display`, e `.to_string()` segue o mesmo mapeamento. `TryFrom<&str>`, `TryFrom<String>` e `FromStr` fazem a conversão inversa e retornam erro para valores desconhecidos. Por exemplo, `waiting_payment` vira `PaymentState::WaitingPayment`; `.to_string()` retorna `"waiting_payment"`, e `PaymentState::try_from("waiting_payment")` reconstrói a mesma variante.

Se você declarar um enum Rust manualmente, derive o mesmo macro e informe o valor persistido com `#[dinoco(value = "...")]`. Somente variantes sem dados são aceitas:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, dinoco::DinocoEnum)]
enum PaymentState {
    #[default]
    #[dinoco(value = "waiting-payment")]
    Waiting,

    #[dinoco(value = "paid")]
    Paid,
}
```

PostgreSQL usa enum nativo, MySQL usa sua representação de enum e SQLite guarda uma representação escalar compatível. A API Rust permanece igual.

### Alterando enums com segurança

O planner detecta enums criados, alterados e removidos. Adicionar um valor costuma ser aditivo; remover ou renomear pode invalidar rows existentes. Quando há risco de perda, a CLI mostra o impacto e pede confirmação. Revise o `down.sql`, pois nem toda alteração de enum é reversível sem reconstrução ou backup.

---

<a id="cap-indexes"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/indexes.md -->

## Índices e constraints

Índices no Dinoco são declarados no `schema.dinoco` e passam pelo mesmo fluxo de migration, introspecção e detecção de drift das tabelas. Esta página separa os três casos: índices explícitos, índices automáticos e busca full-text.

### Escolha o tipo de índice

| Necessidade | Declaração | Resultado |
| --- | --- | --- |
| Acelerar igualdade, ordenação ou range | `@index` | Índice B-tree não único |
| Acelerar um grupo ordenado de fields | `@@indexes([...])` | Índice composto não único |
| Impedir duplicatas em um grupo | `@@uniques([...])` | Índice unique composto |
| Pesquisar tokens em texto | `@fulltext` | GIN no PostgreSQL, `FULLTEXT` no MySQL e fallback no SQLite |
| Pesquisar um documento formado por vários fields | `@@fulltexts([...])` | Full-text composto nativo ou fallback SQLite |
| Primary key | `@id` ou `@@ids` | Índice automático da constraint |
| Foreign key | `@relation(fields: [...])` | Índice automático nas colunas locais |

Declarações comuns e full-text representam estratégias diferentes. Um field não pode participar ao mesmo tempo de `@index`/`@@indexes` e `@fulltext`/`@@fulltexts`.

### Declare um índice explícito

Adicione `@index` a um field escalar ou enum:

```dinoco
model Post {
    id           String   @id @default(uuid())
    slug         String   @index
    published_at DateTime @index(map: "idx_post_publication")
}
```

Sem `map`, o nome segue `idx_<tabela>_<field>`. `map` permite definir o nome físico. O índice não adiciona unicidade; use `@unique` quando duplicatas não forem válidas.

### Declare índices e unicidade compostos

Coloque os grupos ordenados no final do model:

```dinoco
model Article {
    tenant_id String
    id        String
    slug      String
    category  String
    title     String
    body      String?

    @@ids([tenant_id, id])
    @@uniques([tenant_id, slug])
    @@indexes([tenant_id, category])
    @@fulltexts([title, body])
}
```

`@@uniques` rejeita tuples duplicadas e gera um índice unique composto. `@@indexes` é não único. Os dois preservam a ordem dos fields em migration, introspecção e comparação de drift. As declarações podem ser repetidas quando o model precisa de grupos diferentes.

O formatter move toda declaração `@@...` para depois dos fields. Cada array deve ser não vazio, referenciar scalars ou enums existentes e não repetir nenhum field.

### Primary keys são indexadas

Todo field `@id` é indexado pela própria constraint `PRIMARY KEY`. O Dinoco representa esse índice no schema desejado para comparação, mas não gera um `CREATE INDEX` duplicado.

Em uma primary key composta, a ordem declarada é preservada:

```dinoco
model Membership {
    tenant_id String
    user_id   String

    @@ids([tenant_id, user_id])
}
```

O banco mantém um índice composto em `(tenant_id, user_id)`.

### Foreign keys são indexadas

Toda relação materializada recebe um índice automático sobre `fields`:

```dinoco
model Session {
    id         String  @id @default(uuid())
    account_id String
    account    Account? @relation(
        fields: [account_id],
        references: [id]
    )
}
```

O índice gerado é `idx_session_account_id`. Relações compostas recebem um único índice composto, na mesma ordem de `fields`.

Tabelas pivô many-to-many implícitas recebem:

1. o índice da primary key composta;
2. um índice para a foreign key do primeiro lado;
3. um índice para a foreign key do segundo lado.

Não repita `@index` apenas porque o field já participa de `@id` ou `@relation`.

### Índices full-text

Use `@fulltext` somente em `String` ou `String?`:

```dinoco
model Article {
    id      String  @id @default(uuid())
    title   String  @fulltext
    summary String? @fulltext
}
```

Um model pode ter vários fields `@fulltext` independentes. Já `@@fulltexts([title, summary])` cria um único documento ordenado e um único índice nativo. Chamar `.fulltext(...)` em qualquer field gerado pesquisa o grupo completo. O SQLite não cria um B-tree ineficaz e pesquisa cada field do grupo com `LIKE '%termo%'`, unido por `OR`.

Consulte [Busca full-text](https://docs.dinoco.io/v1.3.3/orm/full-text-search) para a API `.fulltext(...)` em todos os finds.

### Fluxo de migration

Depois de alterar um índice:

```bash
dinoco migrate generate
dinoco migrate run
```

O planner gera `CREATE INDEX`, `DROP INDEX` ou a variante full-text do adapter. A introspecção compara nome, colunas, ordem e tipo do índice. Snapshots criados antes da v1.2.0 continuam compatíveis: a ausência histórica da propriedade `indexes` não é tratada como remoção intencional.

### Regras de validação

- `@index` aceita somente `map: "nome"`.
- `@index` funciona em fields escalares e enums, não em fields de relação.
- `@fulltext` não aceita argumentos.
- `@fulltext` funciona somente em `String` e `String?`.
- Um field não pode pertencer ao mesmo tempo a uma declaração comum e uma full-text, inclusive nas formas compostas.
- Vários `@fulltext` são permitidos no mesmo model.
- Todo membro de `@@fulltexts` deve ser `String` ou `String?` e pode pertencer a apenas uma declaração full-text.
- `@@uniques`, `@@indexes` e `@@fulltexts` podem ser repetidos para grupos ordenados diferentes.

---

<a id="cap-relations"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/relations.md -->

## Relações

Relações têm duas partes diferentes:

- a chave escalar persistida no banco, como `account_id`;
- o field de navegação, como `account`, `sessions` ou `systems`.

Não misture as duas. A chave escalar participa de inserts, filtros e constraints. O field de navegação começa vazio e é preenchido por include quando a relação possui carregamento direto. Por isso, todo field de navegação singular deve usar `?`, independentemente de sua foreign key local ser obrigatória ou nullable.

### Regra de tipos para UUID e Snowflake

No schema, UUID continua sendo declarado como `String` e Snowflake como `Integer`. O codegen da v1.3.3 acompanha a chave referenciada e preserva o wrapper Rust:

```dinoco
model Account {
    id       Integer   @id @default(snowflake())
    sessions Session[] @relation(fields: [id], references: [account_id])
}

model Session {
    id         String  @id @default(uuid())
    account_id Integer
    account    Account? @relation(fields: [account_id], references: [id])
}
```

O Rust gerado usa:

```rust
pub struct Account {
    pub id: ::dinoco::Snowflake,
    pub sessions: Vec<Session>,
}

pub struct Session {
    pub id: ::dinoco::Uuid,
    pub account_id: ::dinoco::Snowflake,
    pub account: Option<Account>,
}
```

Portanto:

- `String @default(uuid())` vira `dinoco::Uuid`;
- uma FK `String` que referencia esse ID também vira `dinoco::Uuid`;
- `Integer @default(snowflake())` vira `dinoco::Snowflake`;
- uma FK `Integer` que referencia esse ID também vira `dinoco::Snowflake`;
- optionalidade é preservada: `String?` vira `Option<Uuid>` e `Integer?` vira `Option<Snowflake>`.

Não use `Float` para referenciar Snowflake. A chave Snowflake é inteira.

### Anatomia de @relation

```dinoco
model Post {
    id        String @id @default(uuid())
    author_id String?
    author    User?  @relation(
        fields: [author_id],
        references: [id],
        onDelete: SetNull,
        onUpdate: Cascade
    )
}
```

- `fields` contém fields escalares do model atual.
- `references` contém fields escalares do model relacionado.
- As duas listas precisam ter o mesmo tamanho e tipos compatíveis.
- Relações singulares são sempre opcionais: tanto `author_id String` com `author User?` quanto `author_id String?` com `author User?` são válidos.
- A chave define a constraint do banco; o field de relação representa se os dados de navegação foram carregados.
- `onDelete` e `onUpdate` viram ações da foreign key.

`SetNull` é a exceção entre as ações referenciais: todos os fields da foreign key local precisam ser nullable, pois o banco precisa gravar `NULL` quando a row referenciada for alterada ou removida.

Cada foreign key materializada recebe automaticamente um índice nas colunas de `fields`, preservando a ordem em relações compostas. Uma tabela pivô many-to-many implícita tem um índice para sua primary key composta e um para cada foreign key.

### One-to-many e many-to-one

Um `Account` possui várias `Session`; cada `Session` pertence a um `Account`:

```dinoco
model Account {
    id       Integer   @id @default(snowflake())
    email    String    @unique
    sessions Session[] @relation(fields: [id], references: [account_id])
}

model Session {
    id         String  @id @default(uuid())
    account_id Integer
    token      String  @unique
    account    Account? @relation(
        fields: [account_id],
        references: [id],
        onDelete: Cascade,
        onUpdate: Cascade
    )
}
```

Crie o parent e um child:

```rust
let account = Account::new("ana@example.com".to_string());
dinoco::insert_into::<Account>()
    .values(&account)
    .execute(&client)
    .await?;

let session = Session::new(
    account.id,
    "token-seguro".to_string(),
);
dinoco::insert_into::<Session>()
    .values(&session)
    .execute(&client)
    .await?;
```

Leia os dois lados:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|x| {
        x.sessions()
            .where_(|session| session.token.starts_with("token-"))
            .order_by(|session| session.id.desc())
            .take(10)
    })
    .execute(&client)
    .await?;

let session = dinoco::find_first::<Session>()
    .where_(|x| x.token.eq("token-seguro"))
    .includes(|x| x.account())
    .execute(&client)
    .await?;
```

No lado lista, `take(10)` limita dez children por parent.

### One-to-one

One-to-one é uma foreign key com `@unique`:

```dinoco
model User {
    id      String   @id @default(uuid())
    email   String   @unique
    profile Profile?
}

model Profile {
    id      String @id @default(uuid())
    user_id String @unique
    bio     String
    user    User?   @relation(
        fields: [user_id],
        references: [id],
        onDelete: Cascade
    )
}
```

Sem `@unique`, vários profiles poderiam apontar para o mesmo user e a relação seria many-to-one.

Em uma foreign key one-to-one composta, declare a unicidade da tuple local completa com `@@uniques([field_a, field_b])`. Uma lista `references` composta pode apontar para `@@ids([...])` ou para um grupo `@@uniques([...])` correspondente no model relacionado.

```rust
let profile = dinoco::find_first::<Profile>()
    .where_(|x| x.user_id.eq(&user_id))
    .includes(|x| x.user())
    .execute(&client)
    .await?;
```

### Many-to-many implícito

Listas nos dois lados, sem `fields` e `references`, definem uma relação many-to-many implícita:

```dinoco
model Business {
    id      Integer  @id @default(snowflake())
    name    String
    systems System[]
}

model System {
    id           Integer    @id @default(snowflake())
    name         String
    description  String
    business     Business[]
}
```

#### O que o Dinoco gera

O banco continua tendo uma tabela pivô real:

```text
_business_to_system
├── business_id  -> business.id
└── system_id    -> system.id
```

`(business_id, system_id)` é a primary key composta. O planner de migrations cria foreign keys e índices para as duas colunas.

Os endpoints Rust gerados são, conceitualmente:

```rust
pub struct Business {
    pub id: dinoco::Snowflake,
    pub name: String,
    pub systems: Vec<System>,

    // Chave virtual de escrita; não é coluna de `business`.
    pub system_id: Option<dinoco::Snowflake>,
}

pub struct System {
    pub id: dinoco::Snowflake,
    pub name: String,
    pub description: String,
    pub business: Vec<Business>,

    // Chave virtual de escrita; não é coluna de `system`.
    pub business_id: Option<dinoco::Snowflake>,
}
```

O Dinoco não gera uma entity Rust `BusinessSystem` para uma pivô implícita. `system_id` e `business_id` são fields virtuais `Option<Id>` com duas regras:

- são aceitos como entrada de escrita para a pivô;
- reads sempre os inicializam com `None` e nunca tentam selecioná-los nas tabelas dos endpoints.

Os fields de navegação (`systems` e `business`) formam o lado de leitura. Os accessors gerados mantêm exatamente o nome declarado no schema; por isso este exemplo usa `business()`, mesmo sendo uma lista. Carregue-os com `includes`; não use o ID virtual para descobrir vínculos.

#### Carregar nos dois sentidos

O loader de include atravessa `_business_to_system`; ele nunca procura uma coluna `business_id` inexistente em `system`:

```rust
let business = dinoco::find_first::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .includes(|business| {
        business.systems()
            .where_(|system| system.name.starts_with("Back"))
            .order_by(|system| system.name.asc())
            .take(20)
            .includes(|system| system.business())
    })
    .execute(&client)
    .await?;
```

Filtros e ordenação são aplicados ao model relacionado. `take` e `skip` funcionam por parent, e includes aninhados podem atravessar a mesma pivô no sentido contrário.

O sentido inverso usa o outro field de navegação:

```rust
let systems = dinoco::find_many::<System>()
    .includes(|system| system.business())
    .execute(&client)
    .await?;
```

#### Contar registros relacionados

Counts de relação também atravessam a pivô e aceitam filtros no model relacionado:

```rust
let result = dinoco::count::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .includes(|business| {
        business.systems().where_(|system| system.name.starts_with("Back"))
    })
    .execute(&client)
    .await?;

assert_eq!(result.systems, Some(1));
```

#### Conectar endpoints existentes

Insira os dois endpoints e chame `connect` no ID virtual do target:

```rust
let business = Business::new("Dinoco".to_string());
let system = System::new(
    "Backoffice".to_string(),
    "Sistema administrativo".to_string(),
);

dinoco::insert_into::<Business>().values(&business).execute(&client).await?;
dinoco::insert_into::<System>().values(&system).execute(&client).await?;

let business_id = business.id;
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system.id))
    .execute(&client)
    .await?;
```

Isso insere `(business.id, system.id)` em `_business_to_system`. O retorno mantém `business.system_id` como `None`, pois o field é write-only. O `connect` não cria endpoints e não atualiza uma coluna em `business` ou `system`.

A API inversa cria o mesmo par:

```rust
dinoco::update::<System>()
    .where_(|system| system.id.eq(&system_id))
    .update(|system| system.business_id.connect(&business_id))
    .execute(&client)
    .await?;
```

#### Desconectar endpoints

Use o mesmo field virtual com `disconnect`:

```rust
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.disconnect(&system_id))
    .execute(&client)
    .await?;
```

Somente a row correspondente da pivô é removida. Os dois endpoints continuam intactos.

#### Builders de update suportados

Chaves virtuais many-to-many aceitam `connect` e `disconnect` em:

- `update::<M>()`;
- `update_many::<M>()`;
- `find_and_update::<M>()`;
- `update` e `update_many` com `.returning::<S>()`.

O filtro do endpoint pode usar fields comuns, não apenas a primary key. Antes de alterar a pivô, o Dinoco resolve os IDs de todos os endpoints correspondentes na conexão primary. Por exemplo, um system pode ser ligado a vários businesses:

```rust
dinoco::update_many::<Business>()
    .where_(|business| business.name.starts_with("Dinoco"))
    .update(|business| business.system_id.connect(&system_id))
    .execute(&client)
    .await?;
```

Essas operações relacionais também aceitam o contexto transacional baseado em closure. Execute-as com `.execute(tx)` para que a alteração da pivô e o update escalar do builder usem a mesma conexão física transacional e uma falha posterior desfaça os dois writes.

#### Conectar durante o insert

Preencha um ID virtual antes de inserir o endpoint. O `insert_into` insere o endpoint sem tratar o field virtual como coluna física e, depois, cria a row da pivô:

```rust
let mut system = System::new(
    "ERP".to_string(),
    "Planejamento de recursos empresariais".to_string(),
);
system.business_id = Some(business.id);

dinoco::insert_into::<System>()
    .values(&system)
    .execute(&client)
    .await?;
```

O `insert_many` aplica a mesma regra separadamente para cada payload:

```rust
let mut systems = vec![
    System::new("CRM".to_string(), "Gestão de clientes".to_string()),
    System::new("BI".to_string(), "Inteligência de negócios".to_string()),
];

for system in &mut systems {
    system.business_id = Some(business.id);
}

dinoco::insert_many::<System>()
    .values(&systems)
    .execute(&client)
    .await?;
```

O comportamento também funciona com `.returning::<S>()` em `insert_into` e `insert_many`; os IDs virtuais retornados permanecem `None`. Um field virtual armazena um target ID. Para conectar vários targets, insira o endpoint e faça updates `connect` repetidos. Preencher `business.systems` não cria rows na pivô implícita.

`insert_into` e `insert_many` com IDs virtuais preenchidos também podem executar com `.execute(tx)` dentro da API transacional baseada em closure, tornando atômicas as rows do endpoint e da pivô. Veja [Transações](https://docs.dinoco.io/v1.3.3/orm/transactions#builders-suportados) para o fluxo de mutations suportado.

#### Vínculos duplicados e endpoints ausentes

A primary key composta rejeita pares duplicados. Repetir `connect` pode retornar erro de constraint do banco. As foreign keys também rejeitam vínculos com endpoints inexistentes.

#### Atualizar models gerados da v1.1.1

Gere os models novamente depois do upgrade. O Dinoco remove o arquivo antigo da pivô e deixa de exportar `BusinessSystem`. Substitua código que atualizava a entity pivô:

```rust
// Antes
dinoco::update::<BusinessSystem>()
    .where_(|pivot| pivot.business_id.eq(&business_id))
    .update(|pivot| pivot.system_id.connect(&system_id));

// v1.2.0
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system_id));
```

A pivô SQL e seu histórico de migrations permanecem intactos; somente a API Rust pública gerada muda.

#### Erros comuns

1. Usar `set` em vez de `connect` ou `disconnect` no field virtual.
2. Esperar que `business.system_id` venha preenchido em um read.
3. Conectar antes de os dois endpoints existirem.
4. Conectar o mesmo par duas vezes.
5. Esperar que `business.systems` preenchido crie rows na pivô.

### Many-to-many com campos extras

Se o vínculo possui `role`, `created_at`, permissões ou qualquer outro dado, ele não é uma pivô implícita simples. Declare um model explícito:

```dinoco
model Account {
    id            Integer               @id @default(snowflake())
    system_access AccountSystemAccess[] @relation(fields: [id], references: [account_id])
}

model Systems {
    id             Integer               @id @default(snowflake())
    account_access AccountSystemAccess[] @relation(fields: [id], references: [systems_id])
}

model AccountSystemAccess {
    account_id Integer
    systems_id Integer
    role       String
    created_at DateTime @default(now())

    account Account? @relation(fields: [account_id], references: [id], onDelete: Cascade)
    system  Systems? @relation(fields: [systems_id], references: [id], onDelete: Cascade)
}
```

Nesse caso, insira `AccountSystemAccess` normalmente. As FKs geradas continuam sendo `Snowflake` no Rust.

### Relações repetidas

Duas relações entre os mesmos models precisam de nomes iguais em seus respectivos lados:

```dinoco
model User {
    id              String @id @default(uuid())
    authored_posts  Post[] @relation(name: "PostAuthor", fields: [id], references: [author_id])
    reviewed_posts  Post[] @relation(name: "PostReviewer", fields: [id], references: [reviewer_id])
}

model Post {
    id          String @id @default(uuid())
    author_id   String
    reviewer_id String?

    author   User?  @relation(name: "PostAuthor", fields: [author_id], references: [id])
    reviewer User? @relation(name: "PostReviewer", fields: [reviewer_id], references: [id])
}
```

Sem nomes, o compiler não consegue determinar quais lados formam cada par.

### Self relations

```dinoco
model Employee {
    id         String     @id @default(uuid())
    manager_id String?
    manager    Employee?  @relation(
        name: "Management",
        fields: [manager_id],
        references: [id],
        onDelete: SetNull
    )
    reports Employee[] @relation(
        name: "Management",
        fields: [id],
        references: [manager_id]
    )
}
```

Use um nome explícito e dois fields diferentes. Um único field não pode ser seu próprio lado oposto.

### Ações referenciais

| Ação | Efeito |
| --- | --- |
| `Cascade` | Propaga update ou delete aos dependentes. |
| `Restrict` | Impede a operação enquanto houver dependentes. |
| `NoAction` | Delega o momento do enforcement ao banco. |
| `SetNull` | Grava `NULL`; exige FK e relação opcionais. |
| `SetDefault` | Aplica o default declarado na FK. |

Use `Cascade` somente quando o child realmente não faz sentido sem o parent.

### Checklist de relações

1. Identifique qual model guarda a foreign key.
2. Use `String` para UUID e `Integer` para Snowflake no schema.
3. Mantenha optionalidade da FK e da relação coerentes.
4. Use `@unique` para one-to-one.
5. Declare `fields` e `references` nos dois lados de one-to-many.
6. Deixe ambas as listas sem keys somente para many-to-many implícito.
7. Preencha o ID virtual gerado em `insert_into`/`insert_many` ou use-o com `connect`/`disconnect` para endpoints existentes.
8. Modele uma pivô explícita quando o vínculo tiver campos extras.
9. Nomeie relações repetidas e self relations.
10. Revise a migration antes de aplicá-la.

---

<a id="cap-clients-adapters"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/clients-adapters.md -->

## Clients e adapters

`DinocoClient` possui um `Backend` primary e, opcionalmente, vários backends de réplica. Os builders recebem `&DinocoClient`, então as queries não tomam a posse do client.

### Conexão gerada

`dinoco/mod.rs` exporta `connect()`, que lê as variáveis configuradas em `database_url` e `read_replicas` e cria os adapters primary e de réplica do workspace selecionado:

```rust
mod dinoco;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = dinoco::connect().await?;
    Ok(())
}
```

Use esse fluxo para configurações geradas, inclusive workspaces com réplicas. Construa o client manualmente somente para controlar adapters explicitamente ou definir réplicas dinamicamente.

### PostgreSQL Direct

```rust
use dinoco_engine::{Backend, DinocoClient, PostgresAdapter};

let adapter = PostgresAdapter::direct(
    "postgres://postgres:postgres@localhost:5432/app"
).await?;
let client = DinocoClient::new(Backend::Postgres(adapter));
```

Direct usa o pool PostgreSQL e tem construtor async.

### PostgreSQL com PgBouncer

```rust
use dinoco_engine::{Backend, DinocoClient, PgBouncerAdapter};

let adapter = PgBouncerAdapter::new(
    "postgres://app:secret@pgbouncer:6432/app"
).await?;
let client = DinocoClient::new(Backend::PgBouncer(adapter));
```

Direct e PgBouncer compartilham o mesmo `SqlCompiler` PostgreSQL.

### MySQL

```rust
use dinoco_engine::{Backend, DinocoClient, MySqlAdapter};

let adapter = MySqlAdapter::new(
    "mysql://app:secret@localhost:3306/app"
);
let client = DinocoClient::new(Backend::Mysql(adapter));
```

### SQLite

```rust
use dinoco_engine::{Backend, DinocoClient, SqliteAdapter};

let adapter = SqliteAdapter::new("dinoco/database.sqlite")
    .await
    .map_err(anyhow::Error::msg)?;
let client = DinocoClient::new(Backend::Sqlite(adapter));
```

### Réplicas de leitura

```rust
let primary = PostgresAdapter::direct(primary_url).await?;
let replica_a = PostgresAdapter::direct(replica_a_url).await?;
let replica_b = PostgresAdapter::direct(replica_b_url).await?;

let client = DinocoClient::new(Backend::Postgres(primary))
    .with_read_replicas(vec![
        Backend::Postgres(replica_a),
        Backend::Postgres(replica_b),
    ]);
```

As leituras alternam entre réplicas com um índice round-robin lock-free. Um vetor vazio faz as leituras usarem o primary.

### Regras de execução

- Insert, update e delete sempre usam o primary.
- `find_and_update` sempre usa o primary porque é uma operação de write.
- Finds, includes e counts usam o caminho de leitura.
- `.read_in_primary()` força um find e seus includes no primary.
- A API transacional baseada em closure sempre usa uma única conexão física do primary.
- Cada adapter converte sua row nativa diretamente.

Crie o client uma vez e reutilize-o durante a aplicação. Recriar pools por request adiciona conexão ao hot path.

---

<a id="cap-transactions"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/transactions.md -->

## Transações

`transaction` executa uma closure assíncrona em uma transaction nativa e em uma única conexão física com o primary. Toda mutation executada com `tx` enxerga os writes anteriores da closure. O Dinoco faz commit somente quando a closure retorna `Ok`; qualquer `Err` causa rollback.

### Crie uma transaction

Passe o contexto transacional a cada mutation com `.execute(tx)`. O contexto é copiável e pode ser reutilizado durante toda a closure:

```rust
use dinoco::{find_and_update, insert_into, transaction};

let result = transaction(&client, |tx| async move {
    let business = find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    insert_into::<BusinessTransaction>()
        .value(&movement)
        .execute(tx)
        .await?;

    Ok(business)
})
.await;
```

A closure pode retornar qualquer valor em `Ok(value)`. O resultado externo é `Result<T, TransactionError>`.

### Erros tipados

`TransactionError` preserva a categoria da operação e o erro original do driver. Updates atômicos continuam identificáveis por `AtomicUpdateError`:

```rust
use dinoco::{AtomicUpdateError, DatabaseConstraintError, TransactionError};

match result {
    Ok(business) => use_updated_business(business),
    Err(TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected)) => {
        // O registro não existe ou deixou de satisfazer o WHERE.
    }
    Err(TransactionError::Create(dinoco::CreateError::Constraint {
        kind: DatabaseConstraintError::UniqueViolation,
        ..
    })) => {
        // Violação estruturada de constraint unique.
    }
    Err(TransactionError::Update(error)) => handle_update(error),
    Err(TransactionError::Delete(error)) => handle_delete(error),
    Err(error) => return Err(error.into()),
}
```

Create, update, delete, decode e as categorias portáveis de constraint permanecem separados. `DatabaseError::original()` expõe o erro original de `rusqlite`, `tokio-postgres` ou `mysql_async` quando detalhes do driver forem necessários. A classificação de constraints usa códigos dos drivers, sem interpretar mensagens.

### Rollback automático

Todo erro retornado pela closure causa rollback, inclusive `AtomicUpdateError::RowNotAffected` e erros da aplicação:

```rust
let result = transaction(&client, |tx| async move {
    insert_into::<AuditEntry>().value(&entry).execute(tx).await?;

    find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    Ok(())
})
.await;
```

Se o update não afetar nenhuma row, o insert anterior do audit também sofre rollback.

### Falhas de commit e rollback

Um erro retornado por `COMMIT` é `TransactionError::Commit(DatabaseError)`. Nesse momento o Dinoco informa o resultado do driver sem afirmar se o servidor fez commit, pois esse estado pode ser ambíguo depois de uma falha de conexão.

Se uma operação falhar e o `ROLLBACK` também falhar, o Dinoco retorna:

```rust
TransactionError::RollbackFailed {
    source,         // erro original da operação
    rollback_error, // erro original do driver no rollback
}
```

A falha original nunca é substituída pela falha do rollback.

### Atomicidade e conexão

- SQLite, PostgreSQL Direct, PgBouncer e MySQL usam uma transaction nativa.
- Todos os comandos usam a mesma conexão física com o primary; réplicas de leitura não participam.
- Cada comando executa imediatamente quando seu future é aguardado, permitindo usar controle de fluxo Rust com um resultado anterior.
- Operações numéricas e seus predicados `WHERE` permanecem no `UPDATE` do banco; não existe cálculo com leitura antes do write.
- Usar `tx` fora da closure é rejeitado pelo contexto transacional.

### Builders suportados

A API por closure suporta `insert_into`, `insert_many`, `update`, `update_many`, `delete`, `delete_many`, `find_and_update` e os helpers gerados para mutations aninhadas ou many-to-many. Writes com returning usam o suporte nativo do SQLite e PostgreSQL. No MySQL, `find_and_update` possui um fallback dedicado que executa primeiro o `UPDATE` condicional e recarrega depois por `id` ou pelas condições originais; ele nunca faz uma leitura de existência antes do update.

Builders de leitura que não aceitam um executor de mutation devem continuar usando o client fora desta API por closure. Mantenha dentro da closure todos os writes que precisam receber commit ou rollback juntos.

---

<a id="cap-find"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/find.md -->

## Visão geral de queries

Os builders de leitura do Dinoco são lazy: encadear methods descreve a consulta, enquanto `.execute(&client).await` compila o SQL no adapter e faz o I/O.

### Escolha o builder

| Objetivo | Builder | Retorno padrão |
| --- | --- | --- |
| Buscar zero ou uma row | `find_first::<M>()` | `Option<M>` |
| Buscar várias rows | `find_many::<M>()` | `Vec<M>` |
| Atualizar e retornar uma row | `find_and_update::<M>()` | `M` |
| Contar rows | `count::<M>()` | `MCount` |

Comece pela página específica:

- [Find first](https://docs.dinoco.io/v1.3.3/orm/find-first)
- [Find many](https://docs.dinoco.io/v1.3.3/orm/find-many)
- [Find and update](https://docs.dinoco.io/v1.3.3/orm/find-and-update)
- [Count](https://docs.dinoco.io/v1.3.3/orm/count)

### Etapas de uma query

Uma leitura normalmente segue quatro etapas:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true)) // 1. filtro
    .order_by(|account| account.id.asc())      // 2. ordem
    .take(25)                                  // 3. limite
    .execute(&client)                          // 4. execução
    .await?;
```

Somente `execute` acessa o banco.

### Recursos compartilhados

`find_first` e `find_many` compartilham:

- filtros tipados com `where_`;
- grupos booleanos com `where_complex`;
- busca `.fulltext(...)` em fields configurados;
- `select::<S>()`;
- `includes(...)`;
- `order_by(...)`;
- `read_in_primary()`.

`find_many` acrescenta `take` e `skip`.

### Próximos passos

Depois de escolher o builder, consulte:

- [Filtros](https://docs.dinoco.io/v1.3.3/orm/filters) para operadores simples;
- [Where complex](https://docs.dinoco.io/v1.3.3/orm/where-complex) para `AND`, `OR` e `NOT`;
- [Busca full-text](https://docs.dinoco.io/v1.3.3/orm/full-text-search);
- [Select](https://docs.dinoco.io/v1.3.3/orm/select);
- [Includes](https://docs.dinoco.io/v1.3.3/orm/includes);
- [Transactions](https://docs.dinoco.io/v1.3.3/orm/transactions).

---

<a id="cap-find-first"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/find-first.md -->

## Find first

`find_first` busca no máximo uma row e retorna `Option<M>`. Use-o quando não encontrar um registro for um resultado esperado.

### Consulta básica

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .execute(&client)
    .await?;
```

O tipo é `anyhow::Result<Option<Account>>`. Converta `None` em erro somente na camada em que a ausência é inválida:

```rust
let account = account
    .ok_or_else(|| anyhow::anyhow!("account not found"))?;
```

### Filtre o resultado

Vários `where_` usam `AND`:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .where_(|account| account.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Para grupos explícitos, use [Where complex](https://docs.dinoco.io/v1.3.3/orm/where-complex). Fields `@fulltext` também funcionam:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

### Ordene antes de escolher

`find_first` aceita um `order_by`:

```rust
let newest = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

Sem ordenação, o banco pode escolher qualquer row compatível.

### Selecione e inclua

`select::<S>()` muda o retorno para `Option<S>`. `includes(...)` carrega relações:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

Consulte [Select](https://docs.dinoco.io/v1.3.3/orm/select) e [Includes](https://docs.dinoco.io/v1.3.3/orm/includes).

### Leia no primary

Com réplicas configuradas, use `read_in_primary()` quando a consulta precisa enxergar um write recente:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .read_in_primary()
    .execute(&client)
    .await?;
```

Todos os includes dessa consulta seguem o primary. A API transacional baseada em closure aceita builders de mutation; faça reads comuns com `find_first` pelo client antes ou depois da closure.

---

<a id="cap-find-many"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/find-many.md -->

## Find many

`find_many` retorna todas as rows compatíveis como `Vec<M>`. Sem correspondências, o retorno é um vetor vazio.

### Consulta básica

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .execute(&client)
    .await?;
```

Vários `where_` são combinados com `AND`. Use [Where complex](https://docs.dinoco.io/v1.3.3/orm/where-complex) quando a precedência precisar de grupos `AND`, `OR` e `NOT`.

### Ordene os resultados

```rust
let accounts = dinoco::find_many::<Account>()
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

O builder aceita uma ordenação tipada com `asc()` ou `desc()`.

### Paginação

`take` limita a quantidade e `skip` define o offset:

```rust
let page = dinoco::find_many::<Account>()
    .order_by(|account| account.id.asc())
    .take(25)
    .skip(50)
    .execute(&client)
    .await?;
```

Sempre combine paginação por offset com uma ordenação estável.

### Busca full-text

Fields marcados com `@fulltext` expõem o method:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .execute(&client)
    .await?;
```

Consulte [Busca full-text](https://docs.dinoco.io/v1.3.3/orm/full-text-search) para as diferenças entre adapters.

### Selecione e inclua

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

O retorno passa a ser `Vec<AccountSummary>`. `includes(...)` pode carregar relações, filtrar children e aplicar paginação por parent. Veja [Select](https://docs.dinoco.io/v1.3.3/orm/select) e [Includes](https://docs.dinoco.io/v1.3.3/orm/includes).

### Leia no primary

`read_in_primary()` ignora réplicas nessa consulta e em todos os includes. Use-o em leituras dependentes de um write recente. A API transacional baseada em closure aceita builders de mutation; faça reads comuns com `find_many` pelo client antes ou depois da closure.

---

<a id="cap-select"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/select.md -->

## Select

`select::<S>()` reduz as colunas escalares retornadas e troca o tipo de resultado por uma projeção gerada.

### 1. Declare uma projeção

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

### 2. Selecione no find

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .order_by(|account| account.email.asc())
    .execute(&client)
    .await?;
```

O retorno é `Vec<AccountSummary>`. Em `find_first`, o retorno é `Option<AccountSummary>`.

O derive implementa a conversão das rows nativas de SQLite, PostgreSQL e MySQL; não há mapeamento manual.

### 3. Combine com filtros

`select` não altera o `EntityWhere`: os filtros continuam usando fields do model original.

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

Isso também vale para `where_complex` e `fulltext`.

### Selecione relações

Uma projeção pode declarar uma relation shape compatível e depois usar `includes(...)`. Se a projeção não declara a relação, não tente incluí-la nesse retorno.

Quando um child também usa `select`, o loader transporta a relation key separadamente. A projeção não precisa expor a foreign key apenas para o Dinoco agrupar as rows.

---

<a id="cap-includes"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/includes.md -->

## Includes

`includes(...)` popula fields de relação em uma query. Sem include, relações mantêm o valor vazio gerado: `Vec::new()` para many e `None` para one.

### Inclua uma relação many

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

O Dinoco reúne as parent keys e carrega os children em uma query de batch.

### Inclua uma relação one

```rust
let session = dinoco::find_first::<AccountSession>()
    .where_(|session| session.id.eq(&session_id))
    .includes(|session| session.account())
    .execute(&client)
    .await?;
```

Relações one usam uma estratégia de left join e continuam opcionais quando não há correspondência.

### Filtre a relação

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

### Use where complex e full-text

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

### Faça includes aninhados

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

### Combine com select

O relation builder também aceita `select::<S>()`. A relation key é carregada separadamente da projeção para manter o agrupamento correto.

### Primary e transactions

`read_in_primary()` no find principal direciona o parent e todos os includes ao primary.

A API transacional baseada em closure aceita builders de mutation; execute reads com includes pelo client antes ou depois da closure.

---

<a id="cap-count"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/count.md -->

## Count

`count::<M>()` retorna uma struct gerada, não apenas um inteiro. `total` sempre é preenchido; counts de relação só aparecem quando solicitados.

### Conte um model

```rust
let count = dinoco::count::<User>()
    .execute(&client)
    .await?;

println!("{} users", count.total);
```

O tipo retornado é `UserCount`, e `total` é `i64`.

### Filtre o count

```rust
let active = dinoco::count::<User>()
    .where_(|x| x.active.eq(true))
    .where_(|x| x.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

### Conte relações

```rust
let result = dinoco::count::<User>()
    .includes(|x| {
        x.tokens()
            .where_(|token| token.is_expired.eq(false))
    })
    .execute(&client)
    .await?;

println!("users: {}", result.total);
println!("tokens: {}", result.tokens.unwrap());
```

O builder da relação aceita filtros tipados. Assim, você pode contar somente os registros relacionados que interessam:

```rust
let result = dinoco::count::<Orixa>()
    .where_(|orixa| orixa.is_show.eq(true))
    .includes(|orixa| {
        orixa.questions()
            .where_(|question| question.age.gt(3))
    })
    .execute(&client)
    .await?;
```

Sem `.includes(|x| x.tokens())`, `result.tokens` fica `None`. Relações não são contadas implicitamente.

### Tipos de count gerados

O derive gera uma forma equivalente a:

```rust
pub struct UserCount {
    pub total: i64,
    pub tokens: Option<i64>,
    pub posts: Option<i64>,
}
```

O derive também cria internamente um seletor `UserCountInclude` para o callback de `.includes(...)`. Os métodos de relação ficam nesse seletor, não no `UserCount` retornado. Cada relação solicitada recebe `Some(total)`; as omitidas permanecem `None`.

---

<a id="cap-filters"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/filters.md -->

## Filtros

Os tipos `Where` gerados expõem somente fields reais da entity. O tipo Rust do field limita os valores aceitos, evitando conversões frágeis em runtime.

### Monte uma cláusula where

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.active.eq(true))
    .where_(|user| user.age.gte(18))
    .execute(&client)
    .await?;
```

Vários `where_` são combinados com `AND`. A mesma sintaxe funciona em finds, includes, count, update e delete.

### Operadores comuns

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

### Operadores de String

`String` e `Option<String>` adicionam:

```rust
dinoco::find_many::<User>().where_(|user| user.email.like("dinoco"));
dinoco::find_many::<User>().where_(|user| user.email.starts_with("support"));
dinoco::find_many::<User>().where_(|user| user.email.ends_with("@example.com"));
```

`like` coloca `%` dos dois lados, `starts_with` à direita e `ends_with` à esquerda. Não inclua os curingas manualmente.

Fields declarados com `@fulltext` também expõem `.fulltext(termo)`. Strings comuns não possuem esse method. Veja [Busca full-text](https://docs.dinoco.io/v1.3.3/orm/full-text-search).

### Ranges numéricos e temporais

Inteiros, floats, `DateTime`, `Date` e suas versões opcionais suportam range inclusivo:

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.age.between(18, 65))
    .execute(&client)
    .await?;
```

### Verificações de null

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|task| task.owner_id.null()) // SQL: owner_id IS NULL
    .execute(&client)
    .await?;
```

Use `not_null()` quando a row precisa ter uma foreign key preenchida.

### Valores em batch

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.id.batch(["user-a", "user-b"]))
    .execute(&client)
    .await?;
```

`batch` gera `IN (...)` e também fornece várias keys de origem para `connect` e `disconnect` em pivots.

### Combine condições

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.office.eq("engineering"))
    .where_(|user| user.age.lt(30))
    .where_(|user| user.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Para precedência com `AND`, `OR` e `NOT`, consulte a página dedicada [Where complex](https://docs.dinoco.io/v1.3.3/orm/where-complex).

---

<a id="cap-where-complex"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/where-complex.md -->

## Where complex

`where_complex` cria uma árvore booleana com parênteses explícitos. Use-o quando uma sequência de `where_` unidos por `AND` não representa a regra da consulta.

### Entenda x e m

```rust
.where_complex(|x, m| {
    // x: AccountWhere
    // m: manipulador de grupos
})
```

`x` é o `EntityWhere` gerado para o model. Ele expõe os fields e seus operadores tipados.

`m` monta a estrutura lógica:

| Method | Uso |
| --- | --- |
| `m.and([a, b, ...])` | Todas as condições |
| `m.or(a, b)` | Uma das duas condições |
| `m.or_many([a, b, ...])` | Uma das várias condições |
| `m.not(a)` | Negação de uma condição ou grupo |

### Monte grupos aninhados

```rust
let account = dinoco::find_first::<Account>()
    .where_complex(|x, m| {
        m.or(
            m.and([
                x.id.eq("id"),
                x.name.eq("matheus"),
            ]),
            m.or(
                m.and([
                    x.id.eq("second-id"),
                    x.name.eq("ana"),
                ]),
                m.and([
                    x.id.eq("third-id"),
                    m.not(x.name.eq("blocked")),
                ]),
            ),
        )
    })
    .execute(&client)
    .await?;
```

Cada grupo mantém seus parênteses no SQL. Os valores continuam parâmetros do adapter e seguem a ordem da árvore.

### Where complex substitui where

Ao usar `where_complex`, todo `where_` do mesmo builder é ignorado, independentemente da ordem:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|x| x.active.eq(false)) // ignorado
    .where_complex(|x, m| {
        m.or(x.role.eq(Role::ADMIN), x.active.eq(true))
    })
    .where_(|x| x.deleted_at.not_null()) // ignorado
    .execute(&client)
    .await?;
```

Uma segunda chamada de `where_complex` substitui a árvore anterior.

### Builders suportados

`where_complex` funciona em:

- `find_first`;
- `find_many`;
- `find_and_update`;
- relações `one` e `many` dentro de `includes`.

Quando `find_and_update` executa com `tx`, a mesma condição complexa é compilada no `UPDATE` transacional.

O mesmo comportamento de ignorar `where_` vale nos builders de relação.

### Combine com full-text

Fields `@fulltext` podem participar de qualquer grupo:

```rust
let articles = dinoco::find_many::<Article>()
    .where_complex(|article, m| {
        m.and([
            article.body.fulltext("dinoco"),
            m.not(article.body.fulltext("deprecated")),
        ])
    })
    .execute(&client)
    .await?;
```

### Grupos vazios

Evite montar `and([])` ou `or_many([])` a partir de input vazio. Construa a lista de condições na aplicação e escolha explicitamente entre não filtrar, retornar vazio ou executar um grupo não vazio.

---

<a id="cap-full-text-search"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/full-text-search.md -->

## Busca full-text

O Dinoco oferece busca full-text nativa no PostgreSQL e MySQL e um fallback por substring no SQLite. O method `.fulltext(...)` só é gerado para fields que ativam o recurso no `schema.dinoco`.

### 1. Marque os fields pesquisáveis

```dinoco
model Account {
    id        String  @id @default(uuid())
    name      String  @fulltext
    biography String? @fulltext
    email     String
    reviewed  Boolean @default(false)
}
```

Um model pode ter vários fields `@fulltext`. Cada declaração recebe seu próprio índice no PostgreSQL e MySQL.

As regras são:

- somente `String` e `String?`;
- nenhum argumento;
- não pode coexistir com `@index` no mesmo field;
- não pode ser aplicado a uma relação.

#### Forme um documento com vários fields

Use `@@fulltexts` quando uma busca deve cobrir um grupo ordenado:

```dinoco
model Article {
    id       String  @id @default(uuid())
    title    String
    subtitle String?
    body     String

    @@fulltexts([title, subtitle, body])
}
```

Todos os membros recebem a capability `.fulltext(...)`. `article.title.fulltext("dinoco")`, `article.subtitle.fulltext("dinoco")` e `article.body.fulltext("dinoco")` pesquisam o mesmo documento combinado. Isso é essencial no MySQL: a query usa `MATCH(title, subtitle, body)`, exatamente como o índice `FULLTEXT` composto.

Use declarações `@fulltext` separadas quando os fields precisarem de índices independentes. Um field pode pertencer a apenas uma declaração full-text e não pode participar também de `@index` ou `@@indexes`.

### 2. Gere a migration e os models

```bash
dinoco migrate generate
dinoco migrate run
```

Além do índice nativo quando aplicável, o model Rust gerado recebe a capability que habilita o method:

```rust
account.name.fulltext("matheus");
```

`account.email.fulltext(...)` não compila, pois `email` não possui `@fulltext` nem participa de `@@fulltexts`.

### Use em find first

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

O retorno continua `Option<Account>`.

### Use em find many

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .order_by(|account| account.id.asc())
    .execute(&client)
    .await?;
```

O retorno continua `Vec<Account>`.

### Use em find and update

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.biography.fulltext("dinoco"))
    .update(|account| account.reviewed.set(true))
    .execute(&client)
    .await?;
```

A busca seleciona a row que será atualizada e retornada. Veja [Find and update](https://docs.dinoco.io/v1.3.3/orm/find-and-update).

### Use em relation includes

O method funciona nos builders one e many gerados para includes:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account
            .sessions()
            .where_(|session| session.label.fulltext("mobile"))
    })
    .execute(&client)
    .await?;
```

O field da entity relacionada também precisa estar marcado com `@fulltext`.

### Combine com where complex

```rust
let accounts = dinoco::find_many::<Account>()
    .where_complex(|account, m| {
        m.and([
            account.biography.fulltext("dinoco"),
            m.not(account.biography.fulltext("deprecated")),
        ])
    })
    .execute(&client)
    .await?;
```

O full-text pode aparecer em qualquer grupo `and`, `or`, `or_many` ou `not`.

### Use em transactions

Condições full-text continuam fazendo parte de uma mutation executada pelo contexto transacional:

```rust
let account = dinoco::transaction(&client, |tx| async move {
    let account = dinoco::find_and_update::<Account>()
        .where_(|account| account.name.fulltext("matheus"))
        .update(|account| account.reviewed.set(true))
        .execute(tx)
        .await?;

    Ok(account)
})
.await?;
```

O adapter compila o mesmo predicado full-text no `UPDATE` executado pela conexão transacional.

### Comportamento por banco

| Adapter | Query | Índice |
| --- | --- | --- |
| PostgreSQL / PgBouncer | `to_tsvector('simple', ...) @@ plainto_tsquery('simple', ...)` | GIN de expressão |
| MySQL | `MATCH (...) AGAINST (... IN NATURAL LANGUAGE MODE)` | `FULLTEXT` |
| SQLite | `(field_a LIKE '%termo%' OR field_b LIKE '%termo%')` | Nenhum índice nativo |

Em declarações compostas, o PostgreSQL usa o mesmo documento concatenado no índice GIN e na query. O MySQL preserva a lista exata de colunas em `MATCH(...)`. O SQLite pesquisa substring em todos os membros do grupo e pode varrer a tabela em datasets grandes.

### Comportamento das migrations

PostgreSQL e MySQL usam `idx_<tabela>_<fields...>_fulltext`. O planner preserva a ordem declarada e cria, remove, introspecta e verifica drift desses índices separadamente dos índices comuns e unique.

O SQLite omite a migration de índice porque um B-tree comum não acelera um `LIKE` iniciado por wildcard.

### Limitações

- `@fulltext` cria um índice independente de um field; `@@fulltexts([...])` cria um documento combinado.
- Um field pode pertencer a somente uma declaração full-text e não pode sobrepor `@index` ou `@@indexes`.
- Todo membro do grupo deve ser `String` ou `String?`.
- Ranking, idioma configurável, frases e tokenizers customizados não fazem parte da v1.3.3.
- A semântica de substring do SQLite difere da busca por tokens nativa.

---

<a id="cap-insert"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/insert.md -->

## Insert

O próprio model gerado é o payload de criação. Não existe `#[insertable]`, create struct separada nem `with_relation`.

### Crie uma entity

```rust
let mut user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
user.bio = Some("Rust developer".to_string());
```

`new()` recebe apenas fields escalares obrigatórios sem default ou generator.

### Insira uma entity

```rust
dinoco::insert_into::<User>()
    .values(&user)
    .execute(&client)
    .await?;
```

O empréstimo mantém `user` disponível após o insert. Um valor owned também é aceito.

### Insira várias entities

```rust
let users = vec![
    User::new("ana@example.com".to_string(), "Ana".to_string()),
    User::new("leo@example.com".to_string(), "Leo".to_string()),
];

dinoco::insert_many::<User>()
    .values(&users)
    .execute(&client)
    .await?;
```

Rows escalares são agrupadas em operações multi-row do adapter.

### Insira relações

Preencha diretamente o `Vec` ou `Option` da relação:

```rust
let mut user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
user.tokens = vec![UserToken::new(), UserToken::new()];
user.profile = Some(Profile::new());

dinoco::insert_into::<User>()
    .values(&user)
    .execute(&client)
    .await?;
```

One-to-many, many-to-one e one-to-one usam os metadados da relation. Em many-to-many, preencha o ID virtual antes de `insert_into` ou em cada item enviado para `insert_many`. O Dinoco não inclui esse field nas colunas SQL do endpoint; depois do insert, cria um vínculo na pivô para cada payload preenchido. Outra opção é inserir os endpoints e usar `connect`/`disconnect` nos próprios models. O ID virtual sempre volta como `None` em reads e retornos.

#### Conecte many-to-many durante o insert

Em uma relação many-to-many implícita, cada endpoint recebe um `Option<Id>` virtual do model oposto. Preencha-o antes de `insert_into` para inserir o endpoint e criar o vínculo na pivô durante a mesma execução do builder:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>()
    .values(&tag)
    .execute(&client)
    .await?;
```

O mesmo comportamento é aplicado separadamente para cada payload de `insert_many`:

```rust
let mut tags = vec![
    Tag::new("rust".to_string()),
    Tag::new("database".to_string()),
];

for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>()
    .values(&tags)
    .execute(&client)
    .await?;
```

O Dinoco exclui `task_id` das colunas SQL da tabela `tag`. Depois de inserir cada `Tag`, usa o valor virtual para inserir `(task.id, tag.id)` na pivô. `None` insere o endpoint sem criar vínculo. Outra opção é inserir os dois endpoints primeiro e usar `connect` na API de update.

O ID virtual é write-only e continua `None` em reads e retornos, inclusive com `.returning::<S>()`.

Os mesmos payloads podem ser executados pelo contexto de uma transaction. O insert do endpoint e seu vínculo na pivô recebem commit ou rollback juntos:

```rust
dinoco::transaction(&client, |tx| async move {
    dinoco::insert_into::<Tag>().value(&tag).execute(tx).await?;
    dinoco::insert_many::<Tag>().values(&tags).execute(tx).await?;
    Ok(())
})
.await?;
```

Consulte [Many-to-many implícito](https://docs.dinoco.io/v1.3.3/guide/relations#many-to-many-implícito) para entender os fields gerados, o comportamento da pivô, includes nos dois sentidos, counts, connect/disconnect e a migração de código existente.

### Identificadores gerados

UUID e Snowflake são criados pela lib antes de montar children. Autoincrement é recuperado do banco. Assim, relações funcionam mesmo quando o ID não era parâmetro de `new()`.

### Retorne uma projeção

```rust
let inserted = dinoco::insert_into::<User>()
    .values(&user)
    .returning::<UserSummary>()
    .execute(&client)
    .await?;

let inserted_many = dinoco::insert_many::<User>()
    .values(&users)
    .returning::<User>()
    .execute(&client)
    .await?;
```

Sem `returning`, o resultado é `()`. Com ele, insert retorna `S` e insert_many retorna `Vec<S>`.

---

<a id="cap-update"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/update.md -->

## Update

Os builders separam filtros dos fields alterados. `.set(...)` aceita o mesmo tipo Rust do field gerado.

### Atualize um registro

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva".to_string()))
    .update(|x| x.active.set(true))
    .execute(&client)
    .await?;
```

É obrigatório ter pelo menos um `.update(...)`. Relações não possuem `.set(...)`.

### Atualize vários registros

```rust
dinoco::update_many::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .execute(&client)
    .await?;
```

Um `update_many` sem filtro altera toda a tabela; revise esse tipo de chamada explicitamente.

### Defina fields opcionais

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.bio.set(Some("Maintainer".to_string())))
    .execute(&client)
    .await?;

dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.bio.set(None::<String>))
    .execute(&client)
    .await?;
```

`None` grava SQL `NULL`; fields obrigatórios não aceitam esse valor.

### Retorne uma projeção

```rust
let changed = dinoco::update::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

O retorno é `Vec<UserSummary>`, pois o filtro pode atingir várias rows.

### Conecte e desconecte many-to-many

O Dinoco gera um ID virtual do model oposto em cada endpoint. Filtre o endpoint normalmente e conecte por esse campo virtual:

```rust
dinoco::update::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.connect(&tag_id))
    .execute(&client)
    .await?;

dinoco::update_many::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.disconnect(&tag_id))
    .execute(&client)
    .await?;
```

Os mesmos campos funcionam em `update`, `update_many` e `find_and_update`, inclusive com `.returning::<T>()` nos dois primeiros. Execute o builder com `.execute(tx)` dentro de `transaction(&client, |tx| ...)` para que o write do endpoint e as alterações na pivô recebam commit ou rollback juntos.

Quando um endpoint ainda será criado, o update separado é dispensável: preencha seu `Option<Id>` virtual antes de `insert_into`, ou em cada item aplicável de `insert_many`, e o Dinoco cria o vínculo depois de inserir o endpoint.

Consulte [Many-to-many implícito](https://docs.dinoco.io/v1.3.3/guide/relations#many-to-many-implícito) para a API completa pelos endpoints e as regras da tabela pivô.

Para atualizar exatamente uma row e receber a entity completa sem `.returning()`, consulte a página dedicada [Find and update](https://docs.dinoco.io/v1.3.3/orm/find-and-update).

---

<a id="cap-find-and-update"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/find-and-update.md -->

## Find and update

`find_and_update` aplica um `UPDATE` condicional e retorna a entity. As operações numéricas são avaliadas pelo banco, portanto uma condição como `balance >= amount` e o decremento correspondente fazem parte da mesma mutation atômica.

Como altera dados, o builder sempre usa o backend primary, mesmo quando existem réplicas de leitura configuradas.

### 1. Defina o filtro

```rust
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .where_(|business| business.balance.gte(amount))
    .update(|business| business.balance.decrement(amount))
    .execute(&client)
    .await?;
```

As condições são compiladas no próprio `UPDATE`. O Dinoco não executa um `SELECT` antes para verificar existência ou calcular um valor numérico. Prefira uma primary key ou outra condição única, pois todas as rows compatíveis podem ser atualizadas.

### 2. Defina as alterações

Cada `.update(...)` representa a mudança de um field. As chamadas são acumuladas e compiladas em um único statement:

```rust
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.balance.decrement(amount))
    .update(|business| business.total_withdrawn.increment(amount))
    .update(|business| business.transaction_count.increment(1))
    .execute(&client)
    .await?;
```

Conceitualmente, isso produz:

```sql
UPDATE business
SET balance = balance - ?,
    total_withdrawn = total_withdrawn + ?,
    transaction_count = transaction_count + ?
WHERE id = ?
RETURNING ...
```

Os valores continuam como bind parameters. As operações existentes `.set(value)`, `connect` e `disconnect` continuam funcionando. Alterar o mesmo field mais de uma vez é rejeitado com `AtomicUpdateError::DuplicateField`, evitando semânticas de ordem diferentes entre bancos.

### Operações numéricas

Os fields de update gerados para `Integer`, `Float`, `Integer?` e `Float?` oferecem:

```rust
.increment(value)
.decrement(value)
.multiply(value)
.divide(value)
```

Eles são compilados respectivamente como `field = field + ?`, `field = field - ?`, `field = field * ?` e `field = field / ?`. Colunas numéricas opcionais preservam a semântica SQL normal de `NULL`: aritmética sobre `NULL` continua `NULL`; o Dinoco não adiciona `COALESCE` implícito.

Divisão por zero, overflow, arredondamento e limites numéricos continuam sob responsabilidade do banco selecionado e são propagados pela hierarquia tipada de erros. O Dinoco não faz leitura prévia nem calcula os valores em Rust.

### 3. Leia o retorno

O retorno é `Result<Model, AtomicUpdateError>`, e não `Option<Model>`. Não é necessário chamar `.returning()`:

```rust
use dinoco::AtomicUpdateError;

match result {
    Ok(business) => use_updated_business(business),
    Err(AtomicUpdateError::RowNotAffected) => {
        // Empresa ausente ou condição concorrente de saldo não satisfeita.
    }
    Err(AtomicUpdateError::Constraint { kind, source }) => {
        handle_constraint(kind, source);
    }
    Err(error) => return Err(error.into()),
}
```

`RowNotAffected` é determinado pelo resultado da mutation, sem um `SELECT` de existência. As outras variantes diferenciam update vazio, field duplicado, decode da row, constraint estruturada e outras falhas de banco. `DatabaseError` mantém o erro original do driver.

### Comportamento por adapter

SQLite, PostgreSQL Direct e PgBouncer usam um único `UPDATE ... RETURNING`. O MySQL abre uma transaction nativa, executa primeiro o `UPDATE` condicional, verifica a quantidade de rows afetadas e só então recarrega a row. A recarga usa `id` quando existe um predicado de igualdade; caso contrário, reutiliza as condições originais. Nunca existe um `SELECT` antes do update.

A recarga de compatibilidade do MySQL é um segundo statement na mesma conexão física transacional. Se um filtro sem `id` deixar de corresponder depois da alteração, o Dinoco desfaz a mutation em vez de informar `RowNotAffected` depois de um update bem-sucedido. Prefira uma condição de igualdade em `id` ao alterar um field que também participa do filtro.

### Use where complex

`find_and_update` aceita os mesmos grupos booleanos dos finds:

```rust
let account = dinoco::find_and_update::<Account>()
    .where_complex(|account, m| {
        m.and([
            account.email.eq("matheus@example.com"),
            m.not(account.locked.eq(true)),
        ])
    })
    .update(|account| account.active.set(true))
    .execute(&client)
    .await?;
```

Quando `where_complex` é usado, todos os `where_` do builder são ignorados. Veja [Where complex](https://docs.dinoco.io/v1.3.3/orm/where-complex).

### Use em uma transaction

```rust
let business = dinoco::transaction(&client, |tx| async move {
    let business = dinoco::find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    dinoco::insert_into::<BusinessTransaction>()
        .value(&movement)
        .execute(tx)
        .await?;

    Ok(business)
})
.await?;
```

`RowNotAffected` é promovido para `TransactionError::AtomicUpdate` e causa rollback automático dos writes anteriores.

### Limitações

- Não possui `select`, `includes`, `order_by`, `take` ou `skip`.
- Exige ao menos uma chamada `.update(...)`.
- A mesma coluna não pode ser alterada duas vezes no mesmo builder.
- Uma row ausente ou que deixou de satisfazer as condições gera `AtomicUpdateError::RowNotAffected`.
- O MySQL faz uma recarga depois do update, preferindo um predicado de igualdade em `id` e reutilizando as condições originais quando ele não existe.

---

<a id="cap-delete"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/delete.md -->

## Delete

`delete` exige um filtro em compile time. `delete_many` permite operações em massa, inclusive uma remoção intencional da tabela inteira.

### Remova um registro

```rust
dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .execute(&client)
    .await?;
```

Vários `.where_` são permitidos e combinados com `AND`.

### O filtro obrigatório

Antes de `.where_(...)`, o builder não possui method `.execute()`. Portanto, este código não compila:

```rust
// Inválido de propósito.
dinoco::delete::<User>()
    .execute(&client)
    .await?;
```

Esse typestate evita a forma mais comum de apagar uma tabela por engano.

### Remova vários registros

```rust
dinoco::delete_many::<Session>()
    .where_(|x| x.expires_at.lt(cutoff))
    .execute(&client)
    .await?;
```

Sem filtro, `delete_many` apaga todas as rows. A API permite isso para jobs de limpeza, mas a chamada deve ser tratada como operação crítica no review.

### Retorne os dados removidos

```rust
let deleted = dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

Com `returning`, ambos os builders retornam `Vec<S>`. Sem ele, retornam `()`.

### Relações e ações referenciais

O banco aplica a ação declarada na migration: `Cascade` remove dependentes, `Restrict`/`NoAction` podem rejeitar, `SetNull` desvincula relações opcionais e `SetDefault` aplica o default. O runtime não substitui silenciosamente essa regra.

---

<a id="cap-migrations"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/migrations.md -->

## Migrations

Migrations do Dinoco são artefatos SQL produzidos a partir de um diff validado. O `SqlCompiler` do adapter escreve statements de enum, tabela, coluna e foreign key no dialeto correto.

### Ciclo de uma migration

1. Compilar `dinoco/schema.dinoco`, exigir exatamente uma primary key por model e validar tipos, atributos de model e relações.
2. Conectar ao primary e inspecionar a estrutura real.
3. Construir o schema desejado em tabelas de teste isoladas no próprio banco.
4. Comparar as estruturas atual e desejada.
5. Mostrar cada passo e todo risco detectado.
6. Pedir confirmação para alterações destrutivas.
7. Gerar `up.sql` e `down.sql`, aplicar o up, registrar a migration e gerar models.

O estado atual vem do banco por introspecção, não de um `schema.bin` antigo.

### Gere uma migration

Configure apenas a URL do banco principal:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
dinoco migrate generate
```

Em PostgreSQL e MySQL, a CLI materializa o schema desejado no próprio banco usando tabelas isoladas com o prefixo `dinoco_migration_test_`. Essas tabelas, suas foreign keys e os enums auxiliares são removidos assim que o planejamento termina, inclusive quando ocorre um erro. Em SQLite, a validação continua usando um arquivo temporário.

O prefixo `dinoco_migration_test_` é reservado pelo Dinoco e não deve ser usado por tabelas da aplicação. Nenhuma URL shadow adicional é necessária.

### Como as alterações são detectadas

O planner compara:

- tabelas criadas e removidas;
- colunas adicionadas, removidas, renomeadas ou alteradas;
- mudanças de tipo escalar e tipo nativo;
- opcional para obrigatório e obrigatório para opcional;
- defaults, primary keys e constraints observáveis;
- enums criados, alterados e removidos;
- foreign keys e ações referenciais adicionadas, mudadas ou removidas;
- índices comuns declarados com `@index` ou `@@indexes`, grupos unique declarados com `@@uniques`, índices full-text declarados com `@fulltext` ou `@@fulltexts`, índices de primary keys fornecidos pelas próprias constraints e índices automáticos de foreign keys;
- relações adicionadas ou removidas por suas constraints físicas.

A detecção de rename é estrutural. Sempre confirme a inferência, pois um drop e um add independentes podem ter forma parecida.

### Migrations de índices

O planner trata separadamente índices comuns, unique e full-text. `@index` e `@@indexes` geram statements B-tree não únicos. `@@uniques` composto gera `CREATE UNIQUE INDEX`. `@fulltext` e `@@fulltexts` geram GIN no PostgreSQL, `FULLTEXT` no MySQL e nenhum índice no SQLite.

Primary keys declaradas com `@id` ou `@@ids` aparecem no schema desejado, mas a própria constraint satisfaz o índice e evita um `CREATE INDEX` duplicado. Toda foreign key recebe um índice automático, inclusive relações compostas e os dois lados de uma pivot many-to-many implícita.

Mudanças de nome, colunas, ordem ou tipo geram os passos de drop/create correspondentes. Consulte [Índices e constraints](https://docs.dinoco.io/v1.3.3/guide/indexes) para as regras do schema.

### Alterações perigosas

Remover tabela ou coluna populada, estreitar tipo, remover enum value ou tornar uma coluna nullable obrigatória pode perder dados ou falhar para rows atuais. A CLI destaca o risco e deixa a resposta padrão da confirmação como `No`.

Um banco com tabelas de usuário, mas sem `dinoco_migrations`, também exige confirmação. Em CI, `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` pode confirmar riscos; trate isso como configuração privilegiada de release.

### Revise o SQL gerado

```text
dinoco/migrations/1721320123456_generated/
  up.sql
  down.sql
```

Revise locks, reescrita de dados, impacto de índices e comportamento de enums. Operações irreversíveis aparecem como comentários explicativos no `down.sql`, porque dados apagados não podem ser recriados com segurança.

Faça commit dos dois arquivos e nunca edite uma migration já aplicada; gere outra para manter o histórico igual em todos os ambientes.

### Execute migrations pendentes

```bash
dinoco migrate run
```

O comando cria `dinoco_migrations`, ordena diretórios, ignora migrations já registradas e aplica os `up.sql` pendentes.

### Executar ao iniciar um SQLite local

O `connect()` abre imediatamente a primeira conexão SQLite, criando o arquivo quando ele ainda não existe. O codegen incorpora os `up.sql` do workspace selecionado no módulo `dinoco` e exporta um helper para aplicá-los no mesmo client:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;

if report.changed() {
    println!("Migrations aplicadas: {:?}", report.applied);
}
```

Chamar apenas `connect()` nunca aplica migrations nem cria tabelas da aplicação. Chame `migrate()` explicitamente somente onde a aplicação deve gerenciar o schema do banco local.

Também é possível chamar diretamente `dinoco::runtime::run_migrations`. Em runtime não há compilação do schema nem geração de models: os arquivos SQL já estão embutidos no binário com `include_str!`. As migrations são ordenadas, executadas em transações SQLite e registradas com checksum; remover ou alterar uma migration já aplicada produz erro.

### Models gerados

`migrate generate` sempre atualiza `dinoco/mod.rs` e `dinoco/models/`. Mesmo sem mudança de banco, os models são regenerados antes de encerrar.

---

<a id="cap-cli"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/cli.md -->

## Referência da CLI

O binário `dinoco` centraliza o fluxo do projeto. Rode os comandos na raiz do crate, pois `dinoco/schema.dinoco` é resolvido pelo diretório atual.

### dinoco init

```bash
dinoco init
```

Pergunta entre PostgreSQL, MySQL e SQLite. PostgreSQL abre uma segunda seleção para Direct ou PgBouncer. Cria `dinoco/migrations/` e um schema formatado com `database_url = env("DATABASE_URL")`.

Se o schema já existir, ele é preservado. Em automação, use:

```bash
DINOCO_CLI_INIT_DATABASE=postgresql \
DINOCO_CLI_INIT_POSTGRES_CONNECTION=direct \
dinoco init
```

### dinoco migrate generate

```bash
dinoco migrate generate
```

Compila e valida o schema, inspeciona o banco, planeja e confirma mudanças, gera e aplica a migration e, por fim, gera os models Rust.

Com workspaces, use `dinoco migrate generate --workspace dev` ou `dinoco migrate generate -w dev`. A migration é gravada em `dinoco/migrations/dev/`. Sem o nome, a CLI pergunta qual workspace usar.

Ambiente relevante: a variável indicada por `database_url` e `SNOWFLAKE_NODE_ID` quando houver `snowflake()`. Em PostgreSQL/MySQL, a validação do dialeto usa tabelas isoladas com o prefixo reservado `dinoco_migration_test_` no próprio banco.

### dinoco migrate run

```bash
dinoco migrate run
```

Executa cada `up.sql` pendente e registra o nome em `dinoco_migrations`. É o comando recomendado em deploy depois do review dos arquivos.

Use `--workspace nome` ou `-w nome` para executar somente as migrations daquele workspace.

### dinoco models generate

```bash
dinoco models generate
```

Compila o schema e recria os models sem conectar ao banco ou produzir migration. É útil após trocar de branch ou quando somente o código gerado ficou desatualizado.

Esse comando também aceita `--workspace nome` ou `-w nome`. Ao trocar de workspace, o codegen limpa a árvore anteriormente gerada antes de recriá-la com a configuração selecionada.

### Fluxo recomendado

```bash
# Uma vez
dinoco init

# Depois de alterar o schema
dinoco migrate generate

# Em outro ambiente
dinoco migrate run

# Apenas para atualizar Rust gerado
dinoco models generate
```

A CLI carrega `.env` quando o arquivo existe. Versione um `.env.example` seguro, nunca as credenciais reais.

---

<a id="cap-vscode"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/vscode.md -->

## Extensão do VS Code

A extensão Dinoco transforma `.dinoco` em uma linguagem inteligente e aproxima os comandos do banco do schema. Ela exige um workspace local confiável porque inicia um language server e pode executar a CLI.

### Abra um projeto Dinoco

Abra a raiz do projeto Cargo e rode **Dinoco: Open Schema** pela Command Palette. A extensão procura `dinoco/schema.dinoco` e também ativa automaticamente em qualquer arquivo `.dinoco`.

Para um projeto novo, use **Dinoco: Initialize Project**. Os prompts aparecem em um task terminal interativo.

### Recursos da linguagem

A v1.3.3 oferece:

- syntax e semantic highlight;
- diagnostics locais durante a edição e compilação completa do projeto ao salvar;
- workspace semântico com cache e suporte seguro a grafos de imports circulares;
- completion para paths de import e para os models/enums declarados por um path válido, além de config, scalars, atributos, defaults, relation keys e ações referenciais;
- hover com documentação;
- go to definition, references e rename seguro entre arquivos importados, inclusive para referências de fields em atributos de model;
- symbols, folding e selection ranges;
- quick fixes para config incompleta e nomes de tipos próximos.

Os comandos de models e migrations também estão na Command Palette e no menu de contexto do schema.

### Formatação

O editor usa o mesmo formatter da toolchain. Rode **Dinoco: Format Schema** ou habilite format on save:

```json
{
  "[dinoco]": {
    "editor.defaultFormatter": "dinoco-rs.dinoco-vscode",
    "editor.formatOnSave": true
  }
}
```

O formatter é idempotente: formatar de novo não deve alterar o resultado. Declarações `@@...` são normalizadas depois de todos os fields, com uma linha vazia entre as seções.

Os diagnostics apontam models sem primary key e models com mais de uma chave. O mesmo parser valida nomes, tipos e duplicações nos grupos compostos, além de conflitos entre índices comuns e full-text, antes de uma migration.

Ao salvar qualquer arquivo alcançável pelo schema principal, o LSP recompila o grafo a partir de `dinoco/schema.dinoco`. Cada arquivo canônico é lido uma única vez por travessia; ciclos fecham uma aresta já visitada em vez de reiniciar o carregamento.

### Solução de problemas

Defina `dinoco.cli.path` se o executável não estiver no `PATH` visto pelo VS Code. `dinoco.server.path` serve para testar um server customizado; builds empacotados usam o server bundled.

Use **Dinoco: Show Language Server Output** para logs e **Dinoco: Restart Language Server** após trocar um binário. `dinoco.trace.server` aceita `off`, `messages` e `verbose`.

---

<a id="cap-cookbook"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/cookbook.md -->

## Exemplo completo

Este capítulo junta schema, migration, queries simples, query com includes, projeção, escrita e many-to-many em um único exemplo copiável.

### Schema completo

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

enum TaskStatus {
    pending
    doing
    done
}

model Account {
    id               Integer   @id @default(snowflake())
    email            String    @unique
    name             String
    active           Boolean   @default(true)
    profile          Profile?
    owned_projects   Project[] @relation(fields: [id], references: [owner_id])
    assigned_tasks   Task[]    @relation(fields: [id], references: [assignee_id])
}

model Profile {
    id         String  @id @default(uuid())
    account_id Integer @unique
    bio        String?
    account    Account? @relation(fields: [account_id], references: [id], onDelete: Cascade)
}

model Project {
    id         String @id @default(uuid())
    owner_id   Integer
    name       String
    archived   Boolean @default(false)
    created_at DateTime @default(now())
    owner      Account? @relation(fields: [owner_id], references: [id], onDelete: Cascade)
    tasks      Task[] @relation(fields: [id], references: [project_id])
}

model Task {
    id          Integer    @id @default(snowflake())
    project_id  String
    assignee_id Integer?
    title       String
    status      TaskStatus @default(pending)
    priority    Integer    @default(0)
    project     Project?    @relation(fields: [project_id], references: [id], onDelete: Cascade)
    assignee    Account?   @relation(fields: [assignee_id], references: [id], onDelete: SetNull)
    tags        Tag[]
}

model Tag {
    id    String @id @default(uuid())
    name  String @unique
    tasks Task[]
}
```

Observe os tipos gerados:

- `Account.id`, `Project.owner_id`, `Task.assignee_id` e a chave correspondente da pivô usam `Snowflake`;
- `Profile.id`, `Project.id`, `Task.project_id` e `Tag.id` usam `Uuid`;
- `Task.assignee_id` usa `Option<Snowflake>`;
- chaves virtuais many-to-many preservam o wrapper do ID do model oposto.

### Gere o banco e os models

```bash
export DATABASE_URL="./dinoco.sqlite"
export SNOWFLAKE_NODE_ID="1"

dinoco migrate generate
```

Em outro ambiente que já possui os artefatos de migration:

```bash
dinoco migrate run
dinoco models generate
```

Não edite os arquivos de `dinoco/models/` manualmente. Altere `schema.dinoco` e gere novamente.

### Query simples

```rust
let account = dinoco::find_first::<Account>()
    .where_(|x| x.email.eq("ana@example.com"))
    .execute(&client)
    .await?;

let Some(account) = account else {
    anyhow::bail!("account não encontrado");
};
```

`find_first` retorna `Option`. Ausência não é erro de banco.

### Query com vários filtros

Cada `.where_` adiciona `AND`:

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|x| x.assignee_id.eq(account.id))
    .where_(|x| x.priority.gte(5))
    .where_(|x| x.title.like("migration"))
    .order_by(|x| x.priority.desc())
    .take(20)
    .skip(0)
    .read_in_primary()
    .execute(&client)
    .await?;
```

### Query complexa com includes

```rust
let projects = dinoco::find_many::<Project>()
    .where_(|project| project.archived.eq(false))
    .where_(|project| project.owner_id.eq(account.id))
    .order_by(|project| project.created_at.desc())
    .includes(|project| project.owner())
    .includes(|project| {
        project
            .tasks()
            .where_(|task| task.priority.gte(5))
            .order_by(|task| task.priority.desc())
            .take(10)
            .includes(|task| task.assignee())
    })
    .execute(&client)
    .await?;
```

O limite de tasks é aplicado por project. Os includes irmãos são carregados sem uma query por row.

### Projeção customizada

```rust
use dinoco::EntityExtend;

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountSummary {
    pub id: dinoco::Snowflake,
    pub email: String,
    pub name: String,
}
```

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .where_(|x| x.active.eq(true))
    .order_by(|x| x.name.asc())
    .execute(&client)
    .await?;
```

O tipo da projeção precisa ser idêntico ao field do model gerado.

### Insert relacionado

```rust
let account = Account::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
dinoco::insert_into::<Account>().values(&account).execute(&client).await?;

let project = Project::new(
    account.id,
    "Dinoco 1.3.3".to_string(),
);
dinoco::insert_into::<Project>().values(&project).execute(&client).await?;

let task = Task::new(
    project.id.clone(),
    "Documentar relações".to_string(),
);
dinoco::insert_into::<Task>().values(&task).execute(&client).await?;
```

Os argumentos exatos de `new` são os fields obrigatórios sem default, na ordem do model gerado.

### Update e count

```rust
dinoco::update::<Task>()
    .where_(|x| x.id.eq(task.id))
    .update(|x| x.priority.set(10))
    .update(|x| x.assignee_id.set(Some(account.id)))
    .execute(&client)
    .await?;

let count = dinoco::count::<Task>()
    .where_(|x| x.assignee_id.eq(account.id))
    .execute(&client)
    .await?;
```

### Many-to-many completo

Conecte uma nova `Tag` à task existente diretamente pelo `task_id` virtual durante o insert:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

O mesmo vale para cada item de `insert_many`:

```rust
let mut tags = vec![
    Tag::new("rust".to_string()),
    Tag::new("database".to_string()),
];

for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>().values(&tags).execute(&client).await?;
```

Quando os dois endpoints já existem, `connect` continua disponível. Este exemplo cria outra tag e a conecta depois:

```rust
let review_tag = Tag::new("review".to_string());
dinoco::insert_into::<Tag>().values(&review_tag).execute(&client).await?;

dinoco::update::<Task>()
    .where_(|item| item.id.eq(task.id))
    .update(|item| item.tag_id.connect(&review_tag.id))
    .execute(&client)
    .await?;
```

Carregue todos os vínculos pelo field de relação:

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|item| item.id.eq(task.id))
    .includes(|item| item.tags())
    .execute(&client)
    .await?;
```

Ao desconectar, somente a row selecionada da pivô é removida:

```rust
dinoco::update::<Task>()
    .where_(|item| item.id.eq(task.id))
    .update(|item| item.tag_id.disconnect(&review_tag.id))
    .execute(&client)
    .await?;
```

Preencher `tag.task_id` durante o insert cria a `Tag` e sua row na pivô. `connect` cria apenas a row da pivô para endpoints existentes, e `disconnect` remove somente essa row. Fields virtuais continuam `None` depois de reads.

### Checklist antes de colocar em produção

1. Mantenha URLs somente em variáveis de ambiente.
2. Revise `up.sql` e `down.sql`.
3. Não confunda ausência (`None`) com erro.
4. Ordene queries paginadas.
5. Limite includes de listas.
6. Use `.read_in_primary()` após writes quando consistência imediata for necessária.
7. Preencha os IDs virtuais em `insert_into`/`insert_many` ou use `connect`/`disconnect` para endpoints N:N existentes.
8. Nunca use `Float` como FK de Snowflake.

---

<a id="cap-api-reference"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/api-reference.md -->

## Referência da API

Esta página é um índice compacto. Os guias específicos explicam comportamento, riscos e exemplos completos.

### Referência do schema

```dinoco
config {
    database = "postgresql" # postgresql | mysql | sqlite
    connection = "direct"  # direct | pgbouncer
    database_url = env("DATABASE_URL")
    read_replicas = [env("DATABASE_REPLICA_URL")]
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

enum Role {
    USER
    ADMIN
}

model User {
    id   Integer @id @default(autoincrement())
    role Role    @default(USER)
}
```

Scalars: `String`, `Boolean`, `Integer`, `Float`, `DateTime`, `Date` e `Json`. `?` torna um scalar opcional e `[]` representa lista de relação. Fields de navegação singular sempre exigem `?`; suas foreign keys locais podem ser obrigatórias ou opcionais.

Fields: `@id`, `@unique`, `@index`, `@index(map: "...")`, `@fulltext`, `@default(...)` e `@relation(...)`.

Model: `@@ids([...])`, `@@uniques([...])`, `@@indexes([...])`, `@@fulltexts([...])` e `@@table_name("...")`. Todo model exige exatamente uma declaração de primary key: um `@id` ou um `@@ids`, nunca os dois. Arrays compostos preservam a ordem dos fields.

Toda primary key e foreign key recebe um índice comum automático; a constraint da primary key fornece seu índice sem um `CREATE INDEX` redundante. Full-text só aceita String, e um field não pode participar ao mesmo tempo de uma declaração comum e uma full-text. Defaults: `autoincrement()`, `uuid()`, `snowflake()` e `now()`. Ações: `Cascade`, `Restrict`, `NoAction`, `SetNull` e `SetDefault`.

### API gerada da entity

`#[derive(Entity)]` implementa metadados, conversões nativas de row, helpers `Where`, `OrderBy`, `Update`, `Include` e `Count`, suporte a insert e `new()`.

Uma projeção usa:

```rust
#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

### Methods de leitura

| Builder | Methods | Retorno |
| --- | --- | --- |
| `find_first::<M>()` | `where_`, `where_complex`, `select`, `includes`, `order_by`, `read_in_primary` | `Option<M>` ou `Option<S>` |
| `find_many::<M>()` | os anteriores, `take`, `skip` | `Vec<M>` ou `Vec<S>` |
| `count::<M>()` | `where_`, `includes` | `M::Count` |

Builders de include aceitam `where_`, `where_complex`, `select`, `includes`, `order_by`, `take` e `skip`.
Builders de relações do count aceitam filtros tipados com `where_` e preenchem campos `Option<i64>` em `M::Count`.

### Methods de escrita

| Builder | Obrigatório | Opcional | Retorno |
| --- | --- | --- | --- |
| `insert_into::<M>()` | `values` | `returning::<S>` | `()` ou `S` |
| `insert_many::<M>()` | `values` | `returning::<S>` | `()` ou `Vec<S>` |
| `update::<M>()` | `update` | `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `update_many::<M>()` | `update` | `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `find_and_update::<M>()` | `update` | `where_`, `where_complex` | `M` atualizado |
| `delete::<M>()` | `where_` | `returning::<S>` | `()` ou `Vec<S>` |
| `delete_many::<M>()` | nenhum | `where_`, `returning::<S>` | `()` ou `Vec<S>` |

Updates escalares usam `.set(value)`.

### Writes many-to-many implícitos

Relações many-to-many implícitas expõem um `Option<Id>` virtual write-only nos dois endpoints. Um field preenchido em `insert_into` cria o endpoint e, depois, seu vínculo na pivô:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

O `insert_many` avalia o field virtual de cada item, criando uma row na pivô para cada `Some(id)` e nenhum vínculo para `None`:

```rust
for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>().values(&tags).execute(&client).await?;
```

Para endpoints existentes, use `.connect(value)` e `.disconnect(value)` em `update`, `update_many` ou `find_and_update`. Fields virtuais ficam fora das listas de colunas de `INSERT` e `SELECT` do endpoint e permanecem `None` nas entities retornadas. As duas formas podem ser executadas pelo contexto transacional baseado em closure.

### Transactions

`transaction(&client, |tx| async move { ... }).await` abre uma transaction nativa em uma única conexão física com o primary. Execute cada mutation com `.execute(tx)`. `Ok(value)` faz commit e retorna o valor; qualquer erro causa rollback. `TransactionError` diferencia falhas de create, update, delete, atomic update, commit e rollback, preservando os erros dos drivers.

### Methods de filtro

Todos os scalars: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `batch`, `null` e `not_null`. Strings: `like`, `starts_with`, `ends_with`. Strings marcadas com `@fulltext` ou incluídas em `@@fulltexts`: `fulltext`. Inteiros e floats em filtros: `between`. Inteiros e floats em updates, inclusive opcionais: `increment`, `decrement`, `multiply` e `divide`. Ordenação: `asc` e `desc`.

`fulltext` funciona em `find_first`, `find_many`, `find_and_update`, builders one/many de includes e árvores `where_complex`. Ele continua disponível quando `find_and_update` executa com um contexto transacional. Um membro de `@@fulltexts` pesquisa o grupo completo declarado. O method não existe nas outras Strings.

`where_complex(|x, m| ...)` oferece `m.and`, `m.or`, `m.or_many` e `m.not`. Quando usado, todos os `where_` do mesmo builder são ignorados.

### Construtores de adapters

```rust
PostgresAdapter::direct(url).await?
PostgresAdapter::direct_with_pool(url, min_connections, max_connections).await?
PgBouncerAdapter::new(url).await?
MySqlAdapter::new(url)
SqliteAdapter::new(path).await.map_err(anyhow::Error::msg)?
```

Envolva em `Backend::{Postgres, PgBouncer, Mysql, Sqlite}`, passe para `DinocoClient::new`, adicione réplicas com `.with_read_replicas(vec![...])` e habilite o logger SQL com `.with_logger(true)`.

### Tipos de valor

`DinocoValue` suporta null, integer, float, string, enum, boolean, bytes, JSON, UTC date-time e naive date.

Fields gerados usam `String`, `bool`, `i64`, `f64`, `serde_json::Value`, `chrono::DateTime<chrono::Utc>` e `chrono::NaiveDate`. UUID e Snowflake gerados usam `dinoco::Uuid` e `dinoco::Snowflake`.

---

<a id="cap-release-notes"></a>

<!-- Fonte canônica: docs/src/content/v1.3.3/pt-br/release-notes.md -->

## Dinoco v1.3.3

O Dinoco v1.3.3 esclarece os filtros de fields nullable: use `field.null()` para gerar `IS NULL` e `field.not_null()` para gerar `IS NOT NULL`. Passar um `None` sem tipo para `eq` não é suportado, pois `eq` recebe valores escalares do banco e o Rust não consegue inferir ali o tipo interno do `Option`.

O Dinoco v1.3.3 tornou opcionais todos os fields de relação singular no schema. Fields de navegação como `fee Fee?` agora representam de forma consistente relações ainda não carregadas, enquanto suas foreign keys locais podem continuar obrigatórias (`fee_id Integer`) ou nullable (`fee_id Integer?`). Relações singulares sem `?` são rejeitadas pelo compilador e pelos diagnósticos do VS Code antes da geração dos models.

O compiler e o language server agora suportam imports circulares de forma segura. Cada caminho canônico é parseado e consolidado apenas uma vez, permitindo separar relações bidirecionais como `Account.sessions` e `Session.account` em arquivos diferentes sem recursão infinita ou duplicação de models.

O language server do VS Code acompanha toda a árvore de imports com cache compartilhado e detecção de arquivos já visitados. Models e enums importados participam de completion, hover, go to definition, references e rename entre arquivos. Dentro de `import { ... }`, o autocomplete só oferece declarações quando o path aponta para um `.dinoco` existente; dentro do path, ele sugere diretórios e arquivos `.dinoco` relativos.

Diagnostics rápidos continuam disponíveis durante a edição. Ao salvar, o servidor invalida apenas o arquivo alterado e executa a compilação completa desde `dinoco/schema.dinoco`, publicando erros globais no arquivo e na linha de origem, em um fluxo semelhante ao check-on-save do Rust.

O Dinoco v1.3.3 adiciona updates numéricos executados pelo banco, erros tipados para mutations atômicas e transactions, além da nova API transacional baseada em closure. `find_and_update` agora suporta `increment`, `decrement`, `multiply` e `divide`; todas as alterações encadeadas viram um único `UPDATE` condicional, e a ausência de uma row compatível retorna `AtomicUpdateError::RowNotAffected`.

O arquivo principal `schema.dinoco` agora pode carregar arquivos filhos completos com `config.imports = ["models.dinoco", "enums.dinoco"]`, sem repetir o nome de cada símbolo importado. Os arquivos filhos mantêm imports nomeados explícitos e escopos isolados, deixando visíveis as dependências entre eles.

Arquivos filhos podem importar declarações diretas com `import { Model, Enum } from "caminho/relativo.dinoco"`. O compiler resolve toda a árvore de imports, normaliza caminhos relativos, informa dependências ausentes em sua origem e preserva o arquivo e a linha original nos diagnósticos do schema.

`config.custom_derives` pode aplicar derives Rust adicionais globalmente aos enums ou structs de model gerados. Cada item declara seu alvo, caminho do derive e declaração `use`; o codegen deduplica derives e imports repetidos. Os arquivos `dinoco/mod.rs` gerados agora incluem `#![allow(unused)]`.

Enums opcionais gerados agora são identificados explicitamente como scalars enum em todo o codegen e nos derives. Fields `Enum?` compilam como `Option<Enum>`, defaults opcionais usam `Some(Variant)` e `NULL` do banco é convertido para `None`.

Relações nomeadas agora têm cobertura end-to-end para múltiplas foreign keys apontando ao mesmo model, filtros por enum e `includes` aninhados. O codegen preserva cada `relation_name` junto da foreign key correspondente, evitando que relações distintas sejam misturadas.

Enums gerados derivam `Clone`, `Copy` e `PartialEq`. Models gerados derivam `Clone`; eles também derivam `Copy` quando todos os fields são copiáveis. Models com `String`, UUID, `Json`, listas ou relações permanecem apenas `Clone`, respeitando as regras de ownership do Rust.

### Alterações herdadas da v1.2.0

Enums gerados agora podem ser usados por valor ou referência em todos os filtros e builders de query. Isso inclui `eq`, `neq`, `gt`, `gte`, `lt`, `lte` e `batch`, além de `find_first`, `find_many`, `count`, `find_and_update`, `update`, `update_many`, `delete` e `delete_many`. Variantes chamadas `Error` continuam compatíveis com `future_incompatible` negado.

`DateTime<Utc>`, `NaiveDate` e `serde_json::Value` também aceitam valores próprios e referências nos filtros e updates. Updates opcionais aceitam tanto `Option<T>` quanto `&Option<T>`, e fields de data e data/hora passam a oferecer `between`.

No PostgreSQL, `DateTime<Utc>` agora é serializado de acordo com o tipo real da coluna: como UTC ingênuo para `TIMESTAMP` e com timezone para `TIMESTAMPTZ`. A leitura gerada aceita os dois tipos, inclusive em fields opcionais. Isso corrige inserts, filtros, updates, retornos e transactions que antes podiam falhar com `cannot convert between ... DateTime<Utc> and ... timestamp`.

No PostgreSQL, foreign keys ausentes em tabelas legadas renomeadas são adicionadas como constraints `NOT VALID`. Isso preserva rows históricas órfãs, passa a bloquear novas violações imediatamente e evita que a atualização inteira falhe ou apague dados. Depois de corrigir os órfãos, a aplicação pode executar `ALTER TABLE <tabela> VALIDATE CONSTRAINT <constraint>` em uma migration revisada.

Uma normalização gerada pela v1.1.8 que ficou pendente após esse erro também pode ser recuperada diretamente com `dinoco migrate run`. O Dinoco reconhece o artefato gerado, aplica a mesma proteção `NOT VALID` durante a execução e mantém o checksum do arquivo original.

O Dinoco v1.2.0 adiciona um caminho de atualização para projetos criados com o modelo legado de migrations. O `dinoco migrate generate` copia o SQL legado sem alterações para `up.sql`, cria um `down.sql` conservador que não executa alterações, importa as rows ativas de `_dinoco_migrations` para a tabela de histórico atual e preserva `migration.sql`, `schema.bin` e a tabela de metadata antiga. Ele não apaga dados da aplicação nem sobrescreve um artefato que já esteja no formato atual. A metadata legada é ignorada na detecção de schema drift. Essa compatibilidade vale para projetos SQLite, PostgreSQL, MySQL e PgBouncer.

Tabelas físicas legadas como `"Account"` e `"AudioCreation"` agora são inspecionadas com identificadores que preservam maiúsculas e reconhecidas como antecessoras de `account` e `audio_creation`. O planner gera instruções reversíveis `ALTER TABLE ... RENAME TO ...` no lugar de create/drop, mantém rows e foreign keys e permite que um servidor com histórico legado importado gere e aplique essa normalização. Colunas camelCase legadas como `createdAt` continuam acessíveis porque o SQL gerado agora protege identificadores case-sensitive.

Os updates agora aceitam todas as representações escalares geradas e suportadas pelo Dinoco, incluindo valores próprios e referências de `serde_json::Value`, `DateTime<Utc>` e `NaiveDate`. As formas opcionais também são suportadas, além de strings, booleanos, inteiros, floats, bytes, IDs UUID/Snowflake e enums gerados.

O Dinoco v1.2.0 corrige valores de enum em `find_and_update`, `update` e `update_many`, e usa o suporte nativo de cada banco: tipos nomeados no PostgreSQL, colunas `ENUM` inline no MySQL e colunas `TEXT` com `CHECK` no SQLite. Agora, `find_and_update` retorna erro quando nenhuma row é afetada.

As implementações geradas de `FromStr` e `TryFrom` usam associated error types totalmente qualificados. Assim, variantes de enum chamadas `Error` compilam normalmente mesmo com `ambiguous_associated_items` e `future_incompatible` negados.

Esta versão também preserva grupos de siglas nos nomes de tabela gerados (`BusinessCNAE` vira `business_cnae`, enquanto `BusinessOffice` continua `business_office`) e faz o `migrate generate` mostrar as mudanças detectadas e exigir confirmação com `Y` antes de criar ou aplicar a migration e regenerar os models. Ela inclui ainda as melhorias de workspaces, migrations em runtime, Serde, transactions, relações, índices e query builders introduzidas durante a série v1.1.

### Conversão bidirecional de enums e strings

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

### API de endpoints para many-to-many implícito

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

No `insert_many`, preencha a chave virtual separadamente em cada item; `None` insere aquele endpoint sem vínculo. `disconnect` remove apenas a row correspondente da pivô. As mesmas chaves virtuais de insert e operações relacionais `connect`/`disconnect` podem executar com o contexto transacional baseado em closure e sofrem rollback atômico junto com writes escalares. Projetos existentes devem regenerar os models e substituir usos diretos das entities de pivôs implícitas; a tabela SQL e o histórico de migrations permanecem inalterados.

### Workspaces nomeados

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

### Migrations SQLite opcionais em runtime

O código gerado agora incorpora as migrations do workspace selecionado e exporta `dinoco::migrate(&client)`. A função `connect()` gerada apenas conecta ao SQLite e cria o arquivo quando necessário; ela nunca aplica migrations da aplicação automaticamente:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;
```

Aplicações que precisam de mais controle podem usar `dinoco::runtime::run_migrations` com seus próprios valores `Migration`. As migrations de runtime são ordenadas, transacionais, idempotentes e protegidas por checksums.

### Enums gerados serializáveis

Os enums e models emitidos pelo codegen agora derivam `serde::Serialize` e `serde::Deserialize` pelo reexport público do Dinoco. As variantes usam `#[serde(rename = "...")]` com o valor do banco, permitindo usar os models gerados diretamente em payloads JSON e outros formatos compatíveis com Serde, sem adicionar derives ou conversões manualmente.

### Compatibilidade com Axum e Send

Executores de mutation agora aceitam `DinocoClient`, referências, `Arc<DinocoClient>` e o contexto transacional da closure, preservando futures `Send` para handlers multithread.

A cobertura de regressão em tempo de compilação agora verifica todas as operações CRUD públicas junto com a execução de transactions. Assim, builders de `find`, `count`, `insert`, `update`, `delete` e transactions permanecem compatíveis com os requisitos multithread dos handlers do Axum.

### Índices no schema

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

### Busca full-text

Fields String marcados com `@fulltext` expõem a condição gerada `.fulltext(termo)`. PostgreSQL cria e inspeciona um índice de expressão GIN, MySQL usa um índice `FULLTEXT` nativo e SQLite usa o fallback `LIKE '%termo%'` sem criar um índice B-tree ineficaz. Um model pode ter vários fields full-text, mas `@fulltext` não pode dividir o mesmo field com `@index`.

O engine de migrations planeja, aplica, reverte e inspeciona alterações de índices. Ele gera `CREATE INDEX` e `DROP INDEX`, detecta drift e mantém compatibilidade com snapshots criados antes de os índices serem registrados.

`@@fulltexts([...])` forma um documento pesquisável com vários fields String. PostgreSQL usa a mesma expressão concatenada no índice GIN e na query, MySQL usa a lista composta exata em `MATCH(...)` e SQLite une os fallbacks de substring com `OR`. Todo membro expõe `.fulltext(...)` e pesquisa o grupo completo.

O formatter move todas as declarações `@@...` para depois dos fields. A extensão do VS Code destaca e completa os novos atributos, completa fields dentro dos arrays, resolve definitions e references e aponta primary keys ausentes ou duplicadas.

### Transactions atômicas

Use a API baseada em closure para executar mutations imediatamente e em ordem em uma única conexão física com o primary:

```rust
let business = dinoco::transaction(&client, |tx| async move {
    let business = dinoco::find_and_update::<Business>()
        .where_(|x| x.id.eq(&business_id))
        .where_(|x| x.balance.gte(amount))
        .update(|x| x.balance.decrement(amount))
        .execute(tx)
        .await?;

    dinoco::insert_into::<BusinessTransaction>()
        .value(&movement)
        .execute(tx)
        .await?;

    Ok(business)
})
.await?;
```

A transaction faz commit somente quando a closure retorna `Ok`. Create, update, delete, `find_and_update`, writes de relações e erros da aplicação causam rollback automático. `RowNotAffected` é preservado como `TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected)`. Falhas de commit são separadas, e `RollbackFailed` mantém tanto o erro original da operação quanto o erro do driver no rollback. Violações portáveis de unique, foreign key, not null e check usam códigos estruturados dos drivers, enquanto `DatabaseError` mantém acessível o erro original do driver.

### Filtros complexos

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

A API está disponível em `find_first`, `find_many`, `find_and_update` e nos find builders usados por relation includes. Um `find_and_update` transacional preserva o mesmo filtro complexo em seu `UPDATE` condicional.

### Compatibilidade e verificação

O comportamento da v1.2.0 é coberto por testes de parser, migrations, query builder, adapters e documentação. A suíte de integração com bancos exercita SQLite, PostgreSQL e MySQL; o PgBouncer usa o mesmo compiler PostgreSQL e o mesmo fluxo de execução transacional.

