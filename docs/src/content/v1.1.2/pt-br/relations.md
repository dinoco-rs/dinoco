# Relações

Relações têm duas partes diferentes:

- a chave escalar persistida no banco, como `account_id`;
- o field de navegação, como `account`, `sessions` ou `systems`.

Não misture as duas. A chave escalar participa de inserts, filtros e constraints. O field de navegação começa vazio e é preenchido por include quando a relação possui carregamento direto.

## Regra de tipos para UUID e Snowflake

No schema, UUID continua sendo declarado como `String` e Snowflake como `Integer`. O codegen da v1.1.2 acompanha a chave referenciada e preserva o wrapper Rust:

```dinoco
model Account {
    id       Integer   @id @default(snowflake())
    sessions Session[] @relation(fields: [id], references: [account_id])
}

model Session {
    id         String  @id @default(uuid())
    account_id Integer
    account    Account @relation(fields: [account_id], references: [id])
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
    pub account: Account,
}
```

Portanto:

- `String @default(uuid())` vira `dinoco::Uuid`;
- uma FK `String` que referencia esse ID também vira `dinoco::Uuid`;
- `Integer @default(snowflake())` vira `dinoco::Snowflake`;
- uma FK `Integer` que referencia esse ID também vira `dinoco::Snowflake`;
- optionalidade é preservada: `String?` vira `Option<Uuid>` e `Integer?` vira `Option<Snowflake>`.

Não use `Float` para referenciar Snowflake. A chave Snowflake é inteira.

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

- `fields` contém fields escalares do model atual.
- `references` contém fields escalares do model relacionado.
- As duas listas precisam ter o mesmo tamanho e tipos compatíveis.
- `author_id` é opcional, então `author` também precisa ser opcional.
- `onDelete` e `onUpdate` viram ações da foreign key.

Cada foreign key materializada recebe automaticamente um índice nas colunas de `fields`, preservando a ordem em relações compostas. Uma tabela pivô many-to-many implícita tem um índice para sua primary key composta e um para cada foreign key.

## One-to-many e many-to-one

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
    account    Account @relation(
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

## One-to-one

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
    user    User   @relation(
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

## Many-to-many implícito

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

### O que o Dinoco gera

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

### Carregar nos dois sentidos

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

### Contar registros relacionados

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

### Conectar endpoints existentes

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

### Desconectar endpoints

Use o mesmo field virtual com `disconnect`:

```rust
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.disconnect(&system_id))
    .execute(&client)
    .await?;
```

Somente a row correspondente da pivô é removida. Os dois endpoints continuam intactos.

### Builders de update suportados

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

Essas operações relacionais também funcionam dentro de batches transacionais. O Dinoco executa a alteração da pivô na mesma conexão física que o update escalar do builder; assim, uma falha posterior desfaz os dois writes.

### Conectar durante o insert

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

`insert_into` e `insert_many` com IDs virtuais preenchidos também podem entrar em uma transaction. As rows do endpoint e da pivô são atômicas. O ID do endpoint precisa ser conhecido antes da execução por UUID, Snowflake ou por um valor fornecido pelo caller; um insert transacional não aceita uma chave virtual preenchida quando a primary key do endpoint é `autoincrement()`. Veja [Relações many-to-many em transactions](/v1.1.2/orm/transactions#relações-many-to-many) para exemplos completos e limitações dos adapters.

### Vínculos duplicados e endpoints ausentes

A primary key composta rejeita pares duplicados. Repetir `connect` pode retornar erro de constraint do banco. As foreign keys também rejeitam vínculos com endpoints inexistentes.

### Atualizar models gerados da v1.1.1

Gere os models novamente depois do upgrade. O Dinoco remove o arquivo antigo da pivô e deixa de exportar `BusinessSystem`. Substitua código que atualizava a entity pivô:

```rust
// Antes
dinoco::update::<BusinessSystem>()
    .where_(|pivot| pivot.business_id.eq(&business_id))
    .update(|pivot| pivot.system_id.connect(&system_id));

// v1.1.2
dinoco::update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.system_id.connect(&system_id));
```

A pivô SQL e seu histórico de migrations permanecem intactos; somente a API Rust pública gerada muda.

### Erros comuns

1. Usar `set` em vez de `connect` ou `disconnect` no field virtual.
2. Esperar que `business.system_id` venha preenchido em um read.
3. Conectar antes de os dois endpoints existirem.
4. Conectar o mesmo par duas vezes.
5. Esperar que `business.systems` preenchido crie rows na pivô.

## Many-to-many com campos extras

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

    account Account @relation(fields: [account_id], references: [id], onDelete: Cascade)
    system  Systems @relation(fields: [systems_id], references: [id], onDelete: Cascade)
}
```

Nesse caso, insira `AccountSystemAccess` normalmente. As FKs geradas continuam sendo `Snowflake` no Rust.

## Relações repetidas

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

    author   User  @relation(name: "PostAuthor", fields: [author_id], references: [id])
    reviewer User? @relation(name: "PostReviewer", fields: [reviewer_id], references: [id])
}
```

Sem nomes, o compiler não consegue determinar quais lados formam cada par.

## Self relations

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

## Ações referenciais

| Ação | Efeito |
| --- | --- |
| `Cascade` | Propaga update ou delete aos dependentes. |
| `Restrict` | Impede a operação enquanto houver dependentes. |
| `NoAction` | Delega o momento do enforcement ao banco. |
| `SetNull` | Grava `NULL`; exige FK e relação opcionais. |
| `SetDefault` | Aplica o default declarado na FK. |

Use `Cascade` somente quando o child realmente não faz sentido sem o parent.

## Checklist de relações

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
