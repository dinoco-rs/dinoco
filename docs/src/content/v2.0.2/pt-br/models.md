# Models e fields

Um `model` descreve duas coisas ao mesmo tempo: uma tabela no banco e uma struct Rust gerada. Não existe uma camada separada de "entity" para manter sincronizada — os nomes de field que você escreve no schema são os mesmos nomes que aparecem no Rust, no SQL e em toda mensagem de erro, o que faz do próprio schema a forma mais rápida de entender um projeto Dinoco.

## Declare um model

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

Isso gera uma `Entity` chamada `User` e uma tabela SQL chamada `user` (nomes de model são `PascalCase`; nomes de tabela são gerados em `snake_case`). Todo field gerado é público, usando o tipo Rust da tabela de [tipos escalares](#tipos-escalares) abaixo.

## Regra da primary key

Todo model precisa de **exatamente uma** primary key, declarada de uma das duas formas:

- um único `@id` em um field, para uma chave de uma coluna; ou
- um único atributo de model `@@ids([...])`, para uma chave composta.

> [!WARNING]
> Um `@@ids` composto conta como *uma* declaração de primary key, não importa quantos fields ele liste. Um model sem primary key nenhuma falha na compilação, e o mesmo vale para um com dois `@id`, dois `@@ids`, ou `@id` combinado com `@@ids` — escolha exatamente uma forma.

Fields de primary key precisam ser scalars ou enums obrigatórios (não opcionais). Para uma chave composta, a ordem dos fields em `@@ids([...])` é preservada tanto na constraint do banco quanto no seu índice automático — coloque primeiro a coluna que você mais vai filtrar.

## Tipos escalares

| Dinoco | Rust | SQLite | PostgreSQL | MySQL |
| --- | --- | --- | --- | --- |
| `String` | `String` | `TEXT` | `TEXT` | `VARCHAR(255)` |
| `Boolean` | `bool` | `BOOLEAN` | `BOOLEAN` | `TINYINT(1)` |
| `Integer` | `i64` | `INTEGER` | `BIGINT` | `BIGINT` |
| `Float` | `f64` | `REAL` | `DOUBLE PRECISION` | `DOUBLE PRECISION` |
| `DateTime` | `DateTime<Utc>` | `DATETIME` | `TIMESTAMP` | `TIMESTAMP` |
| `Date` | `NaiveDate` | `DATE` | `DATE` | `DATE` |
| `Json` | `serde_json::Value` | `BLOB` | `JSONB` | `JSON` |

Essa é a lista completa — não existe `Bytes`, `Decimal` nem um escape hatch para tipo escalar customizado. Dois desses tipos ganham um tipo Rust mais descritivo quando são identificadores, para que uma foreign key não seja comparada acidentalmente contra o tipo errado de ID:

| Declaração | Rust gerado |
| --- | --- |
| `String @default(uuid())` | `dinoco::Uuid` |
| `Integer @default(snowflake())` | `dinoco::Snowflake` |
| Foreign key `String` referenciando um `@id` UUID | `dinoco::Uuid` |
| Foreign key `Integer` referenciando um `@id` Snowflake | `dinoco::Snowflake` |

Os tipos Rust de `DateTime`/`Date`/`Json` são reexportados pelo `dinoco_engine`, então todo adapter decodifica da mesma forma — você nunca precisa adicionar `chrono` ou `serde_json` só para nomear o tipo de um field.

## Fields opcionais e listas

Adicione `?` para tornar uma coluna escalar, ou uma relação "um", opcional:

```dinoco
display_name String?
profile      Profile?
```

Isso gera `Option<String>` e `Option<Profile>`. Adicione `[]` para uma relação "muitos":

```dinoco
posts Post[]
```

Isso gera `Vec<Post>`. Repare que `[]` no Dinoco sempre significa "uma relação para muitas linhas" — não existe aqui um tipo de coluna array do SQL, então não dá para escrever `String[]` para guardar uma lista de strings em uma linha.

## Atributos de field

- `@id` — marca o field como a primary key (de uma coluna).
- `@unique` — adiciona uma constraint de unicidade.
- `@index` — adiciona um índice não único.
- `@fulltext` — adiciona capability e índice de busca full-text, em um field `String`.
- `@default(valor)` — um literal, uma variante de enum, ou uma chamada geradora (`uuid()`, `snowflake()`, `autoincrement()`, `now()`).
- `@relation(...)` — declara identidade da relação, colunas de foreign key e ações referenciais. Veja [Relações](/pt-br/docs/orm/guide/relations).

## Atributos do model

Enquanto um atributo de field descreve uma coluna, um atributo de model descreve um grupo ordenado delas:

| Atributo | Finalidade |
| --- | --- |
| `@@ids([tenant_id, id])` | Primary key composta |
| `@@uniques([tenant_id, slug])` | Constraint de unicidade composta |
| `@@indexes([tenant_id, created_at])` | Índice comum composto |
| `@@fulltexts([title, body])` | Índice full-text composto sobre vários fields |
| `@@table_name("audit_users")` | Sobrescreve o nome físico gerado da tabela |

`@@ids`, `@@uniques`, `@@indexes` e `@@fulltexts` recebem, cada um, um array não vazio de nomes de field existentes, scalars ou enums, sem repetição. Todo field listado em `@@fulltexts` precisa ser `String` ou `String?`.

> [!TIP]
> O formatter sempre move atributos de model para depois de todos os fields, separados por uma linha vazia — você nunca precisa pensar onde no corpo do model uma declaração `@@...` deve ficar; rode o formatter e ela cai sempre no mesmo lugar.

Veja [Índices e constraints](/pt-br/docs/orm/guide/indexes) para o panorama completo sobre índices simples vs. compostos, unicidade, e quais índices o Dinoco cria automaticamente para você.

## A função new gerada

Toda entidade gerada ganha um `pub fn new(...) -> Self`. Os parâmetros dele são exatamente os fields escalares que são obrigatórios *e* não têm default nem gerador — tudo o mais (fields opcionais, fields de relação, fields com default) é preenchido automaticamente.

Dado:

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

a construção só precisa dos dois fields que são de fato entradas obrigatórias:

```rust
let user = User::new("ana@example.com".to_string(), "Ana".to_string());
```

`id` é gerado para você, `enabled` recebe seu default declarado, `bio` começa como `None`, e `posts` começa como um `Vec` vazio. Todo field da struct resultante é público, então você pode ajustar qualquer um deles — inclusive os que `new` não pediu — antes de passar a entidade para `insert_into`.

## Fields gerados para many-to-many implícito

Uma relação many-to-many **implícita** (nenhum dos lados declara `fields`/`references`) mantém sua tabela pivô SQL totalmente interna — não existe uma entity pública estilo `PostTag` para importar. Em vez disso, cada lado ganha um field virtual write-only: `Post` ganha `tag_id: Option<TagId>`, e `Tag` ganha `post_id: Option<PostId>`.

Atribuir esse field virtual antes de `insert_into`, ou em cada item de um lote `insert_many`, cria a linha de pivô correspondente logo depois que o próprio endpoint é inserido:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.post_id = Some(post.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

> [!NOTE]
> Esse field virtual não é uma coluna de verdade em `tag` — ele sempre volta como `None` na leitura. Ele existe puramente como alvo de escrita. Para dois endpoints que já existem, use as operações `.connect(...)`/`.disconnect(...)` do mesmo field gerado, a partir de `update`, em vez de mexer nele durante o insert. Veja [Many-to-many implícito](/pt-br/docs/orm/guide/relations#many-to-many-implicito) para o contrato completo, incluindo relações nomeadas e ações referenciais.

## Arquivos gerados

`dinoco models generate` e `dinoco migrate generate` produzem a mesma árvore de módulos previsível:

```text
dinoco/
  mod.rs
  models/
    mod.rs
    user.rs
    post.rs
```

`dinoco/mod.rs` reexporta todo model mais a função `connect()`. Enums ficam compactos dentro de `models/mod.rs` via o derive `DinocoEnum`. Um arquivo por model mantém a revisão de código focada no que de fato mudou.

O módulo raiz gerado começa com `#![allow(unused)]`, que suprime warnings de código não usado do módulo gerado e dos arquivos que ele importa — isso não tem efeito nenhum sobre o código da sua própria aplicação. Se você precisa de derives extras nos tipos gerados, isso é papel de `config.custom_derives`, não de editar um arquivo gerado à mão (que seria simplesmente sobrescrito); veja [Organização do schema](/pt-br/docs/orm/guide/schema-organization#custom-derives).
