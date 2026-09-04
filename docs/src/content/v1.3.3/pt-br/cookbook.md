# Exemplo completo

Este capítulo junta schema, migration, queries simples, query com includes, projeção, escrita e many-to-many em um único exemplo copiável.

## Schema completo

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

## Gere o banco e os models

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

## Query simples

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

## Query com vários filtros

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

## Query complexa com includes

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

## Projeção customizada

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

## Insert relacionado

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

## Update e count

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

## Many-to-many completo

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

## Checklist antes de colocar em produção

1. Mantenha URLs somente em variáveis de ambiente.
2. Revise `up.sql` e `down.sql`.
3. Não confunda ausência (`None`) com erro.
4. Ordene queries paginadas.
5. Limite includes de listas.
6. Use `.read_in_primary()` após writes quando consistência imediata for necessária.
7. Preencha os IDs virtuais em `insert_into`/`insert_many` ou use `connect`/`disconnect` para endpoints N:N existentes.
8. Nunca use `Float` como FK de Snowflake.
