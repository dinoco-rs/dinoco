# Dinoco v2.0.0

Esta página acompanha o que mudou release a release. Cada item linka para a página que documenta a funcionalidade em profundidade — trate isto como um changelog, não como a referência principal.

## v2.0.0

> [!NOTE]
> Este é um release focado em tooling. A linguagem de schema, a API do client gerado e a CLI continuam iguais à v1.3.3 — tudo abaixo é sobre a experiência de edição.

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
