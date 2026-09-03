# Dinoco v1.3.2

O Dinoco v1.3.2 esclarece os filtros de fields nullable: use `field.null()` para gerar `IS NULL` e `field.not_null()` para gerar `IS NOT NULL`. Passar um `None` sem tipo para `eq` não é suportado, pois `eq` recebe valores escalares do banco e o Rust não consegue inferir ali o tipo interno do `Option`.

O Dinoco v1.3.2 tornou opcionais todos os fields de relação singular no schema. Fields de navegação como `fee Fee?` agora representam de forma consistente relações ainda não carregadas, enquanto suas foreign keys locais podem continuar obrigatórias (`fee_id Integer`) ou nullable (`fee_id Integer?`). Relações singulares sem `?` são rejeitadas pelo compilador e pelos diagnósticos do VS Code antes da geração dos models.

O compiler e o language server agora suportam imports circulares de forma segura. Cada caminho canônico é parseado e consolidado apenas uma vez, permitindo separar relações bidirecionais como `Account.sessions` e `Session.account` em arquivos diferentes sem recursão infinita ou duplicação de models.

O language server do VS Code acompanha toda a árvore de imports com cache compartilhado e detecção de arquivos já visitados. Models e enums importados participam de completion, hover, go to definition, references e rename entre arquivos. Dentro de `import { ... }`, o autocomplete só oferece declarações quando o path aponta para um `.dinoco` existente; dentro do path, ele sugere diretórios e arquivos `.dinoco` relativos.

Diagnostics rápidos continuam disponíveis durante a edição. Ao salvar, o servidor invalida apenas o arquivo alterado e executa a compilação completa desde `dinoco/schema.dinoco`, publicando erros globais no arquivo e na linha de origem, em um fluxo semelhante ao check-on-save do Rust.

O Dinoco v1.3.2 adiciona updates numéricos executados pelo banco, erros tipados para mutations atômicas e transactions, além da nova API transacional baseada em closure. `find_and_update` agora suporta `increment`, `decrement`, `multiply` e `divide`; todas as alterações encadeadas viram um único `UPDATE` condicional, e a ausência de uma row compatível retorna `AtomicUpdateError::RowNotAffected`.

O arquivo principal `schema.dinoco` agora pode carregar arquivos filhos completos com `config.imports = ["models.dinoco", "enums.dinoco"]`, sem repetir o nome de cada símbolo importado. Os arquivos filhos mantêm imports nomeados explícitos e escopos isolados, deixando visíveis as dependências entre eles.

Arquivos filhos podem importar declarações diretas com `import { Model, Enum } from "caminho/relativo.dinoco"`. O compiler resolve toda a árvore de imports, normaliza caminhos relativos, informa dependências ausentes em sua origem e preserva o arquivo e a linha original nos diagnósticos do schema.

`config.custom_derives` pode aplicar derives Rust adicionais globalmente aos enums ou structs de model gerados. Cada item declara seu alvo, caminho do derive e declaração `use`; o codegen deduplica derives e imports repetidos. Os arquivos `dinoco/mod.rs` gerados agora incluem `#![allow(unused)]`.

Enums opcionais gerados agora são identificados explicitamente como scalars enum em todo o codegen e nos derives. Fields `Enum?` compilam como `Option<Enum>`, defaults opcionais usam `Some(Variant)` e `NULL` do banco é convertido para `None`.

Relações nomeadas agora têm cobertura end-to-end para múltiplas foreign keys apontando ao mesmo model, filtros por enum e `includes` aninhados. O codegen preserva cada `relation_name` junto da foreign key correspondente, evitando que relações distintas sejam misturadas.

Enums gerados derivam `Clone`, `Copy` e `PartialEq`. Models gerados derivam `Clone`; eles também derivam `Copy` quando todos os fields são copiáveis. Models com `String`, UUID, `Json`, listas ou relações permanecem apenas `Clone`, respeitando as regras de ownership do Rust.

## Alterações herdadas da v1.2.0

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

No `insert_many`, preencha a chave virtual separadamente em cada item; `None` insere aquele endpoint sem vínculo. `disconnect` remove apenas a row correspondente da pivô. As mesmas chaves virtuais de insert e operações relacionais `connect`/`disconnect` podem executar com o contexto transacional baseado em closure e sofrem rollback atômico junto com writes escalares. Projetos existentes devem regenerar os models e substituir usos diretos das entities de pivôs implícitas; a tabela SQL e o histórico de migrations permanecem inalterados.

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

Os enums e models emitidos pelo codegen agora derivam `serde::Serialize` e `serde::Deserialize` pelo reexport público do Dinoco. As variantes usam `#[serde(rename = "...")]` com o valor do banco, permitindo usar os models gerados diretamente em payloads JSON e outros formatos compatíveis com Serde, sem adicionar derives ou conversões manualmente.

## Compatibilidade com Axum e Send

Executores de mutation agora aceitam `DinocoClient`, referências, `Arc<DinocoClient>` e o contexto transacional da closure, preservando futures `Send` para handlers multithread.

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

A API está disponível em `find_first`, `find_many`, `find_and_update` e nos find builders usados por relation includes. Um `find_and_update` transacional preserva o mesmo filtro complexo em seu `UPDATE` condicional.

## Compatibilidade e verificação

O comportamento da v1.2.0 é coberto por testes de parser, migrations, query builder, adapters e documentação. A suíte de integração com bancos exercita SQLite, PostgreSQL e MySQL; o PgBouncer usa o mesmo compiler PostgreSQL e o mesmo fluxo de execução transacional.
