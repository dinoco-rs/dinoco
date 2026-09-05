# Relações

Toda relação no Dinoco é, na verdade, duas coisas separadas usando um único nome:

- **A chave escalar** persistida no SQL — `account_id`, uma coluna `Integer` ou `String` comum que participa de inserts, filtros e constraints exatamente como qualquer outro field.
- **O field de navegação** — `account`, `sessions`, `systems` — que começa vazio e só é preenchido quando você pede explicitamente via `.includes(...)`.

> [!NOTE]
> Como fields de navegação não vêm carregados por padrão, todo field de navegação **singular** precisa ser escrito com `?` — `account Account?` — independentemente de sua foreign key local (`account_id`) ser obrigatória ou nullable. O `?` no field de navegação descreve "isso está carregado", não "essa relação é opcional".

## Regra de tipos para UUID e Snowflake

Declare uma chave baseada em UUID como `String` no schema, e uma baseada em Snowflake como `Integer`. O Dinoco segue o field que uma relação realmente referencia e reaproveita o wrapper Rust gerado do outro lado, então os dois ficam sincronizados automaticamente:

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

Aqui, `Session.id` gera como `Uuid`, e tanto `Account.id` quanto `Session.account_id` geram como `Snowflake` — uma foreign key opcional geraria `Option<Uuid>` ou `Option<Snowflake>` da mesma forma.

> [!DANGER]
> Nunca use `Float` para uma chave ou foreign key baseada em Snowflake. Snowflakes são inteiros, e igualdade de ponto flutuante não se comporta do jeito que um join ou uma checagem de unicidade precisa.

## Anatomia de @relation

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

`fields` lista as colunas escalares locais; `references` lista as colunas do model alvo que elas apontam. As duas listas precisam ter o mesmo tamanho, com tipos compatíveis par a par. A opcionalidade do field de navegação e a opcionalidade da foreign key são duas decisões independentes — `author_id String` com `author User?`, e `author_id String?` com `author User?`, são ambos schemas válidos. A chave controla o que o banco impõe; o field de navegação só controla se os dados relacionados já foram carregados na memória.

`SetNull` vem com um requisito rígido: todo field de foreign key local ao qual ele se aplica precisa ser nullable, porque o banco precisa de um lugar para colocar `NULL` quando a row referenciada muda ou desaparece.

Toda foreign key materializada ganha um índice automático sobre seus `fields`, na ordem declarada — relações compostas inclusive. Veja [Índices e constraints](/pt-br/docs/orm/guide/indexes#foreign-keys-sao-indexadas) para o detalhe completo, incluindo o que uma pivô de many-to-many implícito ganha de índice.

## One-to-many e many-to-one

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
        onDelete: Cascade
    )
}
```

Insira as duas linhas de forma independente — a foreign key é só um valor escalar comum no momento do insert:

```rust
let account = Account::new("ana@example.com".to_string());
dinoco::insert_into::<Account>().values(&account).execute(&client).await?;

let session = Session::new(account.id, "token-seguro".to_string());
dinoco::insert_into::<Session>().values(&session).execute(&client).await?;
```

Carregue a relação a partir de qualquer um dos lados:

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

`.take(10)` no lado lista se aplica **por account pai**, não ao resultado geral.

## One-to-one

Uma relação one-to-one é, na prática, uma one-to-many com uma constraint `@unique` na foreign key — essa constraint é o que torna o "muitos" impossível:

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
    user    User?   @relation(fields: [user_id], references: [id], onDelete: Cascade)
}
```

> [!WARNING]
> Remova o `@unique` de `user_id` e isso silenciosamente vira uma relação many-to-one — vários profiles poderiam então apontar para o mesmo user, e o Dinoco não teria como saber que isso não era intencional.

Para uma foreign key one-to-one composta, declare unicidade sobre a tupla local inteira com `@@uniques([field_a, field_b])`. Uma lista `references` composta pode apontar tanto para `@@ids([...])` quanto para um grupo `@@uniques([...])` correspondente no model relacionado.

## Many-to-many implícito

Dois fields de lista, dos dois lados, sem `fields`/`references` em nenhum deles — é só isso que define uma relação many-to-many implícita:

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

### O que o Dinoco gera

Por baixo dos panos existe uma tabela pivô SQL de verdade:

```text
_business_to_system
├── business_id  -> business.id
└── system_id    -> system.id
```

`(business_id, system_id)` é sua primary key composta, e as duas colunas ganham sua própria foreign key e índice através do planner de migrations — tudo isso é gerenciado para você, mas é SQL de verdade que você poderia inspecionar diretamente se precisasse.

Do lado Rust, cada endpoint fica conceitualmente assim:

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

Não existe uma entity Rust pública `BusinessSystem` para uma pivô implícita — em vez disso, `system_id` e `business_id` são fields virtuais `Option<Id>` regidos por duas regras rígidas:

- Eles são aceitos como entrada de **escrita**, para criar ou apontar uma linha de pivô.
- Uma **leitura** do banco sempre os deixa como `None` — eles nunca compilam para um `SELECT` contra as tabelas dos endpoints.

Os fields de navegação (`systems` e `business`) são o lado de leitura dessa relação, nomeados exatamente como o schema declara — repare que `business` em `System` é um `Vec`, apesar do nome no singular, porque foi assim que o field foi chamado no schema. Carregue-os com `.includes(...)`; nunca leia um ID virtual esperando que ele diga se um vínculo existe.

### Carregar nos dois sentidos

O loader de include sempre atravessa `_business_to_system` diretamente — ele nunca procura uma coluna `business_id` inexistente em `system`:

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

Filtros e ordenação se aplicam ao model relacionado normalmente, `take`/`skip` se aplicam por parent, e um include aninhado pode atravessar a mesma pivô de volta no sentido contrário (como no exemplo acima com `system.business()`).

O sentido inverso só usa o field de navegação do outro lado:

```rust
let systems = dinoco::find_many::<System>()
    .includes(|system| system.business())
    .execute(&client)
    .await?;
```

### Contar registros relacionados

Counts de relação atravessam a pivô da mesma forma que includes, e aceitam filtros no model relacionado:

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

### Conectar endpoints existentes

Quando os dois endpoints já existem, insira-os primeiro, depois chame `.connect(...)` no ID virtual do alvo:

```rust
let business = Business::new("Dinoco".to_string());
let system = System::new("Backoffice".to_string(), "Sistema administrativo".to_string());

dinoco::insert_into::<Business>().values(&business).execute(&client).await?;
dinoco::insert_into::<System>().values(&system).execute(&client).await?;

let business_id = business.id;
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system.id))
    .execute(&client)
    .await?;
```

Isso insere exatamente `(business.id, system.id)` em `_business_to_system` — nada mais. O `business.system_id` retornado continua `None`, porque o field permanece write-only mesmo numa query com returning; conectar nunca insere um endpoint e nunca toca numa coluna de verdade em `business` ou `system`.

A chamada inversa cria o mesmo par:

```rust
dinoco::update::<System>()
    .where_(|system| system.id.eq(&system_id))
    .update(|system| system.business_id.connect(&business_id))
    .execute(&client)
    .await?;
```

> [!WARNING]
> A primary key composta da pivô rejeita um par duplicado, então conectar os mesmos dois endpoints duas vezes retorna um erro de constraint do banco em vez de suceder silenciosamente. Foreign keys também rejeitam um `connect` apontando para um endpoint que não existe — insira os dois lados antes de conectá-los.

### Desconectar endpoints

Use o mesmo field virtual, com `.disconnect(...)`:

```rust
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.disconnect(&system_id))
    .execute(&client)
    .await?;
```

Somente a linha de pivô correspondente é apagada — os dois endpoints em si ficam totalmente intactos.

### Builders de update suportados

Chaves virtuais de many-to-many suportam `.connect(...)`/`.disconnect(...)` em:

- `update::<M>()`
- `update_many::<M>()`
- `find_and_update::<M>()`
- `update`/`update_many` combinados com `.returning::<S>()`

O filtro que seleciona o endpoint pode usar qualquer field, não só sua primary key — o Dinoco resolve o ID de todo endpoint correspondente na conexão primary antes de tocar na pivô. Isso significa que um system pode ser conectado a vários businesses em uma única chamada:

```rust
dinoco::update_many::<Business>()
    .where_(|business| business.name.starts_with("Dinoco"))
    .update(|business| business.system_id.connect(&system_id))
    .execute(&client)
    .await?;
```

Essas operações também aceitam o contexto de transaction por closure — chame `.execute(tx)` para que a alteração da pivô e qualquer outro update escalar no mesmo closure compartilhem uma única conexão física, e uma falha posterior desfaça os dois juntos.

### Conectar durante o insert

Preencha o ID virtual *antes* de inserir o endpoint, e o `insert_into` insere o endpoint (sem nunca tratar o field virtual como uma coluna de verdade) e cria a linha de pivô na mesma chamada:

```rust
let mut system = System::new("ERP".to_string(), "Planejamento de recursos empresariais".to_string());
system.business_id = Some(business.id);

dinoco::insert_into::<System>().values(&system).execute(&client).await?;
```

`insert_many` aplica a mesma regra de forma independente para cada item do lote:

```rust
let mut systems = vec![
    System::new("CRM".to_string(), "Gestão de clientes".to_string()),
    System::new("BI".to_string(), "Inteligência de negócios".to_string()),
];

for system in &mut systems {
    system.business_id = Some(business.id);
}

dinoco::insert_many::<System>().values(&systems).execute(&client).await?;
```

Isso também funciona com `.returning::<S>()` em qualquer uma das chamadas de insert — as chaves virtuais retornadas continuam vindo `None`. Um único field virtual só consegue guardar um target ID por vez; para vincular um endpoint a *vários* targets, insira-o uma vez e faça chamadas `.connect(...)` repetidas depois. Atribuir valores diretamente em `business.systems` não faz nada no nível do banco — só o field de ID virtual cria linhas de pivô.

`insert_into`/`insert_many` com um ID virtual preenchido também funcionam dentro da API de transaction por closure, via `.execute(tx)`, tornando o insert do endpoint e sua linha de pivô atômicos entre si. Veja [Transactions](/pt-br/docs/orm/orm/transactions#builders-suportados) para a lista completa do que roda dentro de uma transaction.

### Atualizar models gerados da v1.1.1

Se você tem um projeto gerado antes das pivôs de many-to-many implícito se tornarem baseadas em field virtual, gerar os models novamente remove o arquivo antigo da pivô e para de exportar a entity `BusinessSystem`. Substitua qualquer código que ainda atualize essa entity diretamente:

```rust
// Antes
dinoco::update::<BusinessSystem>()
    .where_(|pivot| pivot.business_id.eq(&business_id))
    .update(|pivot| pivot.system_id.connect(&system_id));

// v1.2.0 em diante
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system_id));
```

A tabela pivô SQL e seu histórico de migrations não mudam em nada — só a API Rust pública gerada muda.

## Many-to-many com campos extras

Use um model de pivô explícito no momento em que o vínculo em si precisa carregar dados — um papel, um timestamp, um nível de permissão:

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

Isso deixa de ser uma relação many-to-many do ponto de vista do compiler — são duas relações one-to-many comuns se encontrando em `AccountSystemAccess`. Insira esse model exatamente como qualquer outra entity; seus fields de foreign key continuam gerando como `Snowflake`, batendo com os models que referenciam.

## Relações repetidas

Quando dois models se relacionam de mais de uma forma, dê a cada relação um `name` correspondente para que o Dinoco (e o compiler) consigam diferenciá-las:

```dinoco
model User {
    id             String @id @default(uuid())
    authored_posts Post[] @relation(name: "PostAuthor", fields: [id], references: [author_id])
    reviewed_posts Post[] @relation(name: "PostReviewer", fields: [id], references: [reviewer_id])
}

model Post {
    id          String @id @default(uuid())
    author_id   String
    reviewer_id String?
    author      User?   @relation(name: "PostAuthor", fields: [author_id], references: [id])
    reviewer    User?  @relation(name: "PostReviewer", fields: [reviewer_id], references: [id])
}
```

Sem o `name`, o compiler não tem como saber que `authored_posts` forma par com `author` em vez de `reviewer` — duas relações entre os mesmos dois models são ambíguas sem nome.

## Self relations

Um model pode se relacionar consigo mesmo do mesmo jeito que se relaciona com qualquer outro model — a única diferença é o `name` explícito, necessário para que o Dinoco consiga distinguir os dois "lados" do que pareceria um único field se relacionando duas vezes consigo mesmo:

```dinoco
model Employee {
    id         String     @id @default(uuid())
    manager_id String?
    manager    Employee?  @relation(name: "Management", fields: [manager_id], references: [id], onDelete: SetNull)
    reports    Employee[] @relation(name: "Management", fields: [id], references: [manager_id])
}
```

## Ações referenciais

| Ação | Efeito |
| --- | --- |
| `Cascade` | Propaga o update ou delete para as linhas dependentes. |
| `Restrict` | Rejeita a operação de imediato enquanto existirem linhas dependentes. |
| `NoAction` | Delega o momento do enforcement para o próprio banco. |
| `SetNull` | Define a foreign key como `NULL`; exige que a chave e o field de navegação sejam opcionais. |
| `SetDefault` | Recorre ao `@default(...)` declarado na foreign key. |

## Checklist de relações

1. Decida qual model de fato guarda a foreign key — esse é o lado "muitos"/dono.
2. Use `String` para uma chave baseada em UUID, `Integer` para uma baseada em Snowflake.
3. Trate a opcionalidade da foreign key e a do field de navegação como duas escolhas independentes e deliberadas.
4. Adicione `@unique` na foreign key para uma relação one-to-one — sem isso, é many-to-one.
5. Mapeie **os dois** lados de uma relação one-to-many com `fields`/`references` correspondentes.
6. Deixe os dois fields de lista sem mapear só quando você realmente quer um many-to-many implícito.
7. Preencha o ID virtual gerado em `insert_into`/`insert_many` para endpoints novos, ou use `.connect(...)`/`.disconnect(...)` para os que já existem — nunca atribua diretamente na lista de navegação.
8. Use um model de pivô explícito no momento em que o vínculo precisa guardar seus próprios dados.
9. Nomeie toda relação que seja repetida entre dois models, ou uma self relation.
10. Leia as constraints da migration gerada antes de aplicá-la — uma decisão de ação referencial é uma decisão de integridade de dados, não só uma forma de agradar o compiler.
