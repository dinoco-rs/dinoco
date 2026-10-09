# Dinoco v2.0.8

Esta página acompanha o que mudou release a release. Cada item linka para a página que documenta a funcionalidade em profundidade — trate isto como um changelog, não como a referência principal.

## v2.0.8

> [!WARNING]
> Release com breaking changes. As closures de `transaction(...)` agora retornam `Result<T, TransactionError<E>>` em vez de `anyhow::Result<T>`, e `find_batch(...)`, `QueryMode` e as migrations manuais foram removidos. Cada item abaixo diz o que mudar.

- **Erros customizados em transactions.** Retorne `Err(TransactionError::Custom(error))` de uma closure de `transaction(...)` para fazer rollback com um tipo de erro seu e trate-o ao lado dos erros do banco: `Err(TransactionError::Custom(RuntimeError::WithoutPermission)) => ...`. `TransactionError` agora é `TransactionError<E = Infallible>`, e o `?` continua convertendo erros de insert, update, delete e update atômico, e qualquer `anyhow::Error`. Veja [Erros customizados](/pt-br/docs/orm/orm/transactions#erros-customizados).
- Atualizando uma closure: troque `anyhow::bail!(...)` por `return Err(anyhow::anyhow!(...).into())` (ou por um erro customizado), e escreva uma expressão final `anyhow::Result` como `Ok(expr.await?)`. Onde nada nomeia o tipo customizado, tipicamente `.await?` dentro de uma função que retorna `anyhow::Result`, nomeie o padrão uma vez com `Ok::<_, TransactionError>(value)` ou `let result: Result<_, TransactionError> = ...`. Código que passa o erro para uma `fn(TransactionError)` ou o converte via `From<TransactionError>` não precisa mudar.
- **`find_batch(...)` e `QueryMode` foram removidos**, junto com `DinocoClient::with_query_mode`/`query_mode`, `config.query_mode` e a decodificação `DinocoJson` por trás do modo single query. Rode queries independentes com `tokio::try_join!`, que é o que o modo padrão `BatchQuery` fazia. Rode `dinoco models generate` para o `dinoco/mod.rs` parar de chamar `.with_query_mode(...)`.
- **As migrations manuais foram removidas.** As migrations voltam a ser sempre geradas a partir do `schema.dinoco`, como antes da v2.0.3. `config.migration_engine`, `dinoco init --migration-engine`, o argumento de nome do `dinoco migrate generate`, `dinoco migrate rollback`, `dinoco migrate status`, `DinocoMigration`, `DinocoManager`, `#[dinoco(migration)]`, `migrate_up`/`migrate_down`/`migration_status` e o registro `dinoco/migrations/mod.rs` deixaram de existir, e a extensão do VS Code não oferece mais `migration_engine`. Um schema que ainda define `migration_engine` ou `query_mode` falha ao compilar e pede para apagar a entrada. Num projeto que usava migrations manuais, regenere os models (para o `dinoco/mod.rs` perder o `pub mod migrations;`) e apague os arquivos Rust em `dinoco/migrations/`; o `migrate generate` ignora arquivos ali e só lê diretórios de migration. Veja [Migrations](/pt-br/docs/orm/tooling/migrations).

## v2.0.7

- **Filtros de relação.** Todo field de relação de um model agora existe no seu tipo `Where`, então uma query pode ser restringida pelas rows relacionadas sem carregá-las: `find_first::<Transaction>().where_(|x| x.payout.where_(|p| p.method.eq(PayoutMethod::Pix)))` retorna só as transactions cujo payout é Pix. Além de `where_`, os fields de relação oferecem `where_complex`, `none`, `every`, `exists` e `not_exists`, se aninham por outras relações e funcionam em todo formato de relação (many-to-one, one-to-many, os dois lados de um one-to-one, many-to-many implícito, self relations) e em todo builder que aceita `where_` — finds, `count`, `exists`, updates, deletes, filtros de include e transactions. Cada filtro vira uma subquery `EXISTS` correlacionada no mesmo statement. Veja [Filtros de relação](/pt-br/docs/orm/orm/relation-filters).
- `FindWhere` ganhou a variante `Relation` (com `RelationMatch`, `RelationJoinTable` e `RelationQuantifier`); código que faz `match` exaustivo sobre `FindWhere` precisa de um braço para ela.

## v2.0.6

> [!WARNING]
> Fields de relação singulares que formam um ciclo — os dois lados de um one-to-one, ou many-to-ones que voltam ao mesmo model — agora são gerados como `Option<Box<T>>`. Código que atribui esses fields precisa de `Some(Box::new(value))`; esses models não compilavam antes.

- **Relações one-to-one geradas corretamente.** O lado sem a foreign key (`User.profile`) era gerado como `many_to_one` sem chaves, então o include não carregava nada e as duas structs se continham por valor (`recursive types ... have infinite size`). Agora é `#[dinoco(one_to_one, inverse, foreign_key = "...", references = "...")]`: `.includes(...)` carrega a relação pelos dois lados, e preenchê-la em `insert_into`/`insert_many` insere a row relacionada com a foreign key preenchida. Veja [One-to-one](/pt-br/docs/orm/guide/relations#one-to-one).

## v2.0.5

> [!WARNING]
> O `dinoco/transform.rs` agora é compilado junto com a sua aplicação (veja abaixo). Um transform que importa `dinoco_codegen::prelude::*` precisa trocar para `use dinoco::codegen::*;` antes de o `dinoco/mod.rs` ser regenerado, senão a aplicação deixa de compilar.

- **`@updated_at`.** Um field `DateTime`/`Date` declarado com `@updated_at @default(now())` recebe a hora atual (UTC) do banco em todo `update`, `update_many` e `find_and_update` que altera a row — no mesmo `UPDATE`, sem parâmetro extra, incluindo `.returning(...)`, `.pluck(...)` e transactions. Um `.set(...)` explícito no field tem prioridade; updates só de relação não mexem nele. O compiler exige `@default(now())` e rejeita outros tipos, primary keys e argumentos. A extensão do VS Code completa, destaca e documenta o atributo. Veja [Timestamps de atualização](/pt-br/docs/orm/guide/defaults-enums#timestamps-de-atualizacao).
- **Transforms só precisam do `dinoco`.** Tudo que o `transform.rs` usa (`DinocoTransformer`, `Model`, `Field`, `ImplMethod`, `quote!`, ...) é reexportado como `dinoco::codegen::*`, então os projetos não dependem mais de `dinoco_codegen`. Veja [Transforms de código](/pt-br/docs/orm/guide/transforms#o-arquivo-de-transform).
- **Suporte do editor ao `transform.rs`.** Quando `dinoco/transform.rs` existe, o `dinoco/mod.rs` gerado declara `mod transform;`, então o rust-analyzer e o `cargo check` o analisam. A CLI continua aplicando o arquivo pelo próprio runner. Veja [Onde o transform roda](/pt-br/docs/orm/guide/transforms#onde-o-transform-roda).
- Um field de data opcional com `@default(now())` (`DateTime?`, `Date?`) agora gera `default = Some(...)`; antes gerava código que não compilava.

## v2.0.4

> [!WARNING]
> `insert_into(...)` e `insert_many(...)` agora retornam `Result<_, CreateError>` em vez de `anyhow::Result<_>`, e `CreateError::Constraint { kind, .. }` foi substituído por uma variante por tipo de constraint. `?` para `anyhow::Error` continua funcionando; código que passa o resultado para uma função que recebe `anyhow::Error` (por exemplo `.map_err(MyError::internal)`) ou que faz match em `CreateError::Constraint` precisa ser atualizado.

- **Erros de insert tipados.** `CreateError` agora tem as variantes `UniqueViolation`, `ForeignKeyViolation`, `NotNullViolation` e `CheckViolation`, com a `table`, a `constraint` e as `columns` que o driver reportou, além de `NotReturned` quando um insert com returning não consegue ler sua row de volta. Helpers: `is_unique_violation()`, `constraint()`, `columns()`, `database_error()`. `DatabaseError::constraint_details()` expõe os mesmos detalhes para qualquer erro classificado. Veja [Trate erros de insert](/pt-br/docs/orm/orm/insert#trate-erros-de-insert).
- **Leituras dentro de transactions.** `find_first` e `find_many` aceitam o contexto de transaction (`.execute(tx)`), incluindo `.includes(...)`, `.pluck(...)` e `.transform(...)`. Elas rodam na conexão da transaction e enxergam seus writes ainda não commitados. Veja [Transactions](/pt-br/docs/orm/orm/transactions#leia-dentro-de-uma-transaction).
- **Ambiente de teste.** `create_test_ambient()` devolve um client sobre um banco SQLite novo na memória, com o schema inteiro já criado (seja qual for o `config.database`), um banco isolado por chamada. `setup_test_methods::<M>(&client)` instala callbacks `on_insert`/`on_update`/`on_delete`/`on_find` de um model, que recebem o resultado, a query compilada e o erro de toda operação nele (dentro de transactions e `.includes(...)` também); `remove_test_methods::<M>(&client)` os remove. Veja [Testes](/pt-br/docs/orm/orm/testing).
- **O `connect()` gerado usa o ambiente de teste sob `cfg(test)`.** O `dinoco/mod.rs` agora também exporta `connect_test()` (o ambiente de teste para o schema e o workspace do crate) e `connect_database()` (a conexão real). `connect()` devolve `connect_test()` nos testes do próprio crate e `connect_database()` em todo o resto. Gere os models de novo com `dinoco models generate` para recebê-las. Veja [Testes](/pt-br/docs/orm/orm/testing#o-connect-gerado-nos-testes).
- **Schema salvo por workspace.** `migrate generate` e `models generate` com um workspace agora copiam o `schema.dinoco` e todo arquivo que ele importa para `dinoco/migrations/<workspace>/schema/`, mantendo a estrutura de pastas. Veja [Schema salvo por workspace](/pt-br/docs/orm/guide/configuration#schema-salvo-por-workspace).
- Writes dentro de uma transaction agora reportam internamente a contagem real de rows afetadas (antes reportavam `0`).
- O erro 1364 do MySQL (coluna `NOT NULL` sem valor e sem default) agora é classificado como `NotNullViolation`.

## v2.0.3

> [!WARNING]
> O `config.custom_derives` foi **removido**. Um schema que ainda o declara falha ao compilar com um aviso apontando para os code transforms. Troque cada entrada por um hook em `dinoco/transform.rs`; veja [Migrar de custom_derives](/pt-br/docs/orm/guide/transforms-recipes#migrar-de-custom-derives).

- **Migrations manuais.** Novo `config.migration_engine = "automatic" | "manual"` (padrão `automatic`, então projetos existentes não mudam). Com `manual`, cada migration é um tipo Rust que implementa `DinocoMigration` (`#[dinoco(migration)]`, `up`/`down` sobre um `DinocoManager` que expõe toda operação suportada pelo motor automático), registrado em `dinoco/migrations/mod.rs`. `dinoco migrate generate <nome>` cria o esqueleto, `migrate run` aplica as pendentes, e os novos `migrate rollback` e `migrate status` revertem e listam. `dinoco init --migration-engine manual` inicia um projeto assim. Removido na v2.0.8.
- **Code transforms.** `dinoco/transform.rs` expõe um `transformer()` que implementa `DinocoTransformer`, aplicado por `models generate` e `migrate generate`. Ele pode adicionar derives, atributos (em tipos, campos, variantes e relações), imports, métodos inerentes e impls de trait, tanto em structs quanto em enums. Veja [Code transforms](/pt-br/docs/orm/guide/transforms).
- **`where_complex(...)` em `count::<M>()`.** O `count` agora aceita a mesma composição `AND`/`OR` dos builders de find (`exists::<M>()` já aceitava). Veja [Count](/pt-br/docs/orm/orm/count#filtros-complexos).
- **Ferramentas.** A extensão do VS Code conhece `migration_engine` (completion, hover e validação).
- **Docs para IA e buscadores.** O site agora serve um servidor MCP (`/mcp`), `llms.txt`/`llms-full.txt`, metadados de página mais ricos (títulos por página, hreflang, imagens Open Graph, JSON-LD) e um sitemap com alternates. Veja [Usar com IA](/pt-br/docs/orm/guide/ai).

## v2.0.2

> [!NOTE]
> Um point release: várias capacidades aditivas do query builder. A linguagem de schema e a forma do código gerado continuam iguais fora da nova opção `config.query_mode` — projetos existentes continuam compilando.

- **`exists::<M>()`.** Checa se alguma row bate, compilando para `SELECT EXISTS(...)` e retornando um `bool` puro sem materializar nenhuma row. Veja [Visão geral de queries](/pt-br/docs/orm/orm/find#checando-existência).
- **`.pluck(...)`.** Projeta sobre uma única coluna e retorna seus valores diretamente (`Vec<T>`/`Option<T>`/`T`, dependendo do builder) em vez de um row model completo. Disponível em `find_many`, `find_first`, `update`, `update_many`, `insert_into`, `insert_many`, `delete` e `delete_many`. Veja [Find many](/pt-br/docs/orm/orm/find-many#pluck-em-uma-única-coluna), [Update](/pt-br/docs/orm/orm/update#pluck-em-uma-única-coluna), [Insert](/pt-br/docs/orm/orm/insert#pluck-em-uma-única-coluna) e [Delete](/pt-br/docs/orm/orm/delete#pluck-em-uma-única-coluna).
- **`.transform(...)`.** Aplica uma closure Rust simples sobre a(s) row(s) já buscada(s) — um mapeamento pós-query, não uma projeção SQL. Disponível em `find_many`, `find_first`, e depois de `.returning::<S>()` em `update`, `update_many`, `insert_into`, `insert_many`, `delete` e `delete_many`. Veja [Find many](/pt-br/docs/orm/orm/find-many#transforme-os-resultados).
- **`.connect_batch(...)`/`.disconnect_batch(...)`.** Liga ou desliga vários destinos many-to-many em um único round trip em vez de uma chamada `.connect(...)`/`.disconnect(...)` por valor — colapsa em um único `INSERT` multi-row/`DELETE` com lista IN. Veja [Relações](/pt-br/docs/orm/guide/relations#conectardesconectar-vários-endpoints-de-uma-vez).
- **`find_batch(...)`.** Roda uma tupla de 2 a 8 builders `find_many`/`find_first` independentes e retorna uma tupla correspondente de resultados. O novo `QueryMode` (`BatchQuery`, o default; ou `SingleQuery`, que combina cada item em um único round trip agregado em JSON) é configurável por client via `.with_query_mode(...)` ou pelo `schema.dinoco` via `config.query_mode`. Removido na v2.0.8.

## v2.0.1

> [!NOTE]
> Um point release: uma capacidade aditiva do query builder mais o trabalho de tooling abaixo. A linguagem de schema e a CLI continuam iguais, e nenhum código gerado muda de forma — projetos existentes continuam compilando.

- **Filtrar many-to-many implícito pelo outro lado.** As chaves virtuais `Option<Id>` geradas (`system.business_id`) agora funcionam como entrada de `where_(...)`, não só como alvo de `connect`/`disconnect`/insert. `find_many::<System>().where_(|system| system.business_id.eq(&business_id))` retorna apenas os systems vinculados àquele business, compilado como uma subquery de pertinência sobre a pivô. Toda a superfície de filtros do `Field` se aplica à coluna de destino da pivô — `eq`/`neq`, `gt`/`gte`/`lt`/`lte`, `batch`/`not_in`, `null`/`not_null`, `like`/`starts_with`/`ends_with`, `between` — compõe com filtros escalares e `where_complex`, e `count::<T>()` respeita. Veja [relações](/pt-br/docs/orm/guide/relations#filtrar-por-um-dos-lados).
- **Formatter configurável.** O formatter da extensão do VS Code agora aceita `dinoco.formatter.maxWidth`, `dinoco.formatter.useTabs`, `dinoco.formatter.useSpaces`, `dinoco.formatter.indentSize` e `dinoco.formatter.removeComments`. `useTabs`/`useSpaces` são mutuamente exclusivos e mantidos sincronizados automaticamente. Veja a [extensão do VS Code](/pt-br/docs/orm/tooling/vscode#formatacao).
- **Semantic highlighting de verdade.** O language server agora emite semantic tokens derivados do mesmo índice usado por hover e completion, então o nome de um model é colorido de forma diferente dependendo se é uma declaração ou uma referência — algo que uma gramática baseada em regex não consegue fazer de forma confiável.
- **Syntax highlighting mais preciso.** Comentários `//`, as ações referenciais `Restrict`/`NoAction` e os atributos de campo principais (`@id`, `@unique`, `@relation`, `@default`, `@index`, `@fulltext`) agora têm seus próprios scopes, em vez de caírem em scopes genéricos.

## v1.3.3

- Corrigido: relações repetidas em árvores de `includes` aninhadas — uma entidade alcançada por dois caminhos de relação diferentes agora é hidratada de forma independente em qualquer adapter. Veja [includes](/pt-br/docs/orm/orm/includes).
- Esclarecido o comportamento de filtros em campos nulos: `field.null()` / `field.not_null()` geram `IS NULL` / `IS NOT NULL`; um `None` sem tipo passado para `.eq(...)` não é suportado, já que o Rust não consegue inferir o tipo interno ali. Veja [filtros](/pt-br/docs/orm/orm/filters).
- Todo field de navegação de relação singular agora precisa ser opcional (`fee Fee?`) no schema, independentemente de a foreign key local ser obrigatória ou não. Veja [relações](/pt-br/docs/orm/guide/relations).
- O compiler e o language server suportam imports circulares com segurança — cada arquivo é parseado e consolidado uma única vez, então relações bidirecionais podem viver em arquivos separados. Veja [organização do schema](/pt-br/docs/orm/guide/schema-organization).
- Adicionadas atualizações numéricas atômicas no banco (`increment`/`decrement`/`multiply`/`divide`) e erros tipados de mutação atômica/transaction em `find_and_update`. Veja [find and update](/pt-br/docs/orm/orm/find-and-update) e [transactions](/pt-br/docs/orm/orm/transactions).
- Adicionado `config.imports` para carregar arquivos de schema filhos inteiros sem repetir cada símbolo, além do `import { ... } from "..."` nomeado já existente. Veja [organização do schema](/pt-br/docs/orm/guide/schema-organization).
- Adicionado `config.custom_derives` para aplicar derives Rust extras a enums e structs de model gerados.
- Fields `Enum?` agora compilam como `Option<Enum>` de ponta a ponta, incluindo defaults e decodificação de `NULL`.
- Relações nomeadas têm suporte completo para múltiplas foreign keys apontando para o mesmo model. Veja [relações](/pt-br/docs/orm/guide/relations).
- Enums gerados derivam `Clone, Copy, PartialEq`; models gerados derivam `Clone`, e `Copy` quando todo field é copiável.
- Conversão bidirecional enum ↔ string (`.to_string()` / `TryFrom<&str>` / `FromStr`) usando os valores originais do schema.
- Relações many-to-many implícitas não geram mais uma entidade pivô pública — em vez disso, cada lado ganha uma foreign key virtual write-only (`business.system_id`) usada para `connect`/`disconnect` ou para vincular uma linha durante o insert. Veja [relações](/pt-br/docs/orm/guide/relations#many-to-many-implicito).
- Adicionadas configurações de banco nomeadas, por ambiente, em `config.workspace`, selecionadas com `--workspace`/`-w`. Veja [configuração](/pt-br/docs/orm/guide/configuration#workspaces).
- Adicionadas migrations SQLite embutidas e opt-in via `dinoco::migrate(&client)`, para aplicações que querem aplicar migrations a partir do binário em vez da CLI.
- Enums e models gerados derivam `serde::Serialize`/`Deserialize` através do próprio re-export do Dinoco.
- Verificada a compatibilidade de futures `Send` em todo builder e no contexto de transaction, para frameworks multithread como o Axum.
- Adicionados `@index`, `@@indexes([...])` e `@@uniques([...])` para índices explícitos e compostos; toda primary key e foreign key é indexada automaticamente. Veja [índices e constraints](/pt-br/docs/orm/guide/indexes).
- Adicionada busca full-text com `@fulltext` e `@@fulltexts([...])`, com índice nativo em PostgreSQL e MySQL e um fallback portável em SQLite. Veja [busca full-text](/pt-br/docs/orm/orm/full-text-search).
- Adicionada a API de transaction por closure (`dinoco::transaction(&client, |tx| async move { ... })`) com commit/rollback automático e erros tipados. Veja [transactions](/pt-br/docs/orm/orm/transactions).
- Adicionado `where_complex` para agrupamento explícito de `AND`/`OR`/`NOT`. Veja [where complex](/pt-br/docs/orm/orm/where-complex).

## v1.2.0

- Enums gerados podem ser passados por valor ou referência para todo filtro e query builder.
- `DateTime<Utc>`, `NaiveDate` e `serde_json::Value` aceitam valores por valor e por referência em filtros e updates; fields de data/datetime ganharam `.between(...)`.
- Corrigida a serialização de `DateTime<Utc>` no PostgreSQL para respeitar o tipo real da coluna (`TIMESTAMP` vs. `TIMESTAMPTZ`).
- Adicionado um caminho de upgrade a partir do modelo legado de migrations: `dinoco migrate generate` importa o histórico existente e tabelas legadas (incluindo identificadores case-sensitive) sem apagar dados.
- Corrigido o tratamento de enums em `find_and_update`/`update`/`update_many` para usar o suporte nativo a enum de cada banco.
- `migrate generate` agora mostra as mudanças detectadas e pede confirmação antes de criar ou aplicar uma migration.

## Releases anteriores

A série v1.1 introduziu as bases de workspace, migrations em runtime, Serde, transactions, relações, índices e query builders sobre as quais os releases acima se apoiam.
