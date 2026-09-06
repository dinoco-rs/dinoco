# Exemplo completo

Enquanto o [quickstart](/pt-br/docs/orm/guide/quickstart) é minimalista de propósito, esta página vai na direção contrária: um schema realista com vários formatos de relação, e todo tipo de query e escrita que você provavelmente vai usar numa aplicação real. Trate como uma referência para copiar, não um tutorial para ler do início ao fim.

## Schema completo

```dinoco
config {
    database          = "sqlite"
    database_url      = env("DATABASE_URL")
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

enum TaskStatus {
    pending
    doing
    done
}

model Account {
    id             Integer   @id @default(snowflake())
    email          String    @unique
    name           String
    active         Boolean   @default(true)
    profile        Profile?
    owned_projects Project[] @relation(fields: [id], references: [owner_id])
    assigned_tasks Task[]    @relation(fields: [id], references: [assignee_id])
}

model Profile {
    id         String   @id @default(uuid())
    account_id Integer  @unique
    bio        String?
    account    Account? @relation(fields: [account_id], references: [id], onDelete: Cascade)
}

model Project {
    id         String   @id @default(uuid())
    owner_id   Integer
    name       String
    archived   Boolean  @default(false)
    created_at DateTime @default(now())
    owner      Account? @relation(fields: [owner_id], references: [id], onDelete: Cascade)
    tasks      Task[]   @relation(fields: [id], references: [project_id])
}

model Task {
    id          Integer    @id @default(snowflake())
    project_id  String
    assignee_id Integer?
    title       String
    status      TaskStatus @default(pending)
    priority    Integer    @default(0)
    project     Project?   @relation(fields: [project_id], references: [id], onDelete: Cascade)
    assignee    Account?   @relation(fields: [assignee_id], references: [id], onDelete: SetNull)
    tags        Tag[]
}

model Tag {
    id    String @id @default(uuid())
    name  String @unique
    tasks Task[]
}
```

Esse único schema exercita todos os formatos de relação que o Dinoco suporta: `Account` ↔ `Profile` é um-para-um, `Account` ↔ `Project`/`Task` é um-para-muitos, e `Task` ↔ `Tag` é um many-to-many **implícito** (nenhum dos dois lados declara `fields`/`references` — o Dinoco gera e gerencia a tabela pivô para você). Vale a pena rastrear os tipos de chave gerados antes de seguir em frente:

- `Account.id`, `Project.owner_id` e `Task.assignee_id` são `Snowflake`.
- `Profile.id`, `Project.id`, `Task.project_id` e `Tag.id` são `Uuid`.
- `Task.assignee_id`, especificamente, é `Option<Snowflake>` (a relação é opcional).
- As chaves virtuais de many-to-many geradas espelham o wrapper que o `@id` do model **oposto** usa.

## Gere o banco e os models

```bash
export DATABASE_URL="./dinoco.sqlite"
export SNOWFLAKE_NODE_ID="1"

dinoco migrate generate
```

Em outra máquina que já tem os arquivos de migration em disco — CI, o notebook de um colega —, aplique-os sem planejar uma nova:

```bash
dinoco migrate run
dinoco models generate
```

> [!WARNING]
> Nunca edite arquivos dentro de `dinoco/models/` manualmente. Eles são regenerados a partir do schema a cada `migrate generate`/`migrate run`/`models generate`, então qualquer edição manual é descartada silenciosamente. Se o código gerado precisa ser diferente, mude o `schema.dinoco`.

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

`find_first` retorna `Option<Account>` — a ausência de uma linha não é um erro de banco, então trate isso explicitamente em vez de dar unwrap.

## Query com vários filtros

Cada chamada `.where_(...)` adiciona mais uma condição, combinada com `AND`:

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

`.read_in_primary()` tira essa leitura específica do roteamento para read replicas — use logo depois de um write, quando o resultado precisa refletir aquele write imediatamente. Veja [filtros](/pt-br/docs/orm/orm/filters) se precisar de `OR`/`NOT` em vez do `AND` implícito.

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

O limite `.take(10)` se aplica **por project**, não ao resultado geral — cada project retornado traz até dez das suas próprias tasks de maior prioridade. Includes irmãos (`owner()` junto com `tasks()`) são carregados como queries em lote separadas, nunca como uma query por linha.

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

let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .where_(|x| x.active.eq(true))
    .order_by(|x| x.name.asc())
    .execute(&client)
    .await?;
```

> [!WARNING]
> Todo field de uma struct de projeção precisa bater exatamente com o **tipo** do field no model gerado — `id: dinoco::Snowflake`, não `i64` nem `String`, porque é isso que `Account.id` de fato gera. Uma divergência é erro de compilação, não uma surpresa em runtime, mas é o erro mais comum ao escrever uma struct `#[derive(EntityExtend)]` na mão.

## Insert relacionado

```rust
let account = Account::new("ana@example.com".to_string(), "Ana".to_string());
dinoco::insert_into::<Account>().values(&account).execute(&client).await?;

let project = Project::new(account.id, "Dinoco 2.0.1".to_string());
dinoco::insert_into::<Project>().values(&project).execute(&client).await?;

let task = Task::new(project.id.clone(), "Documentar relações".to_string());
dinoco::insert_into::<Task>().values(&task).execute(&client).await?;
```

Os argumentos de `new` são exatamente os fields obrigatórios do model sem default, na ordem em que são declarados no schema — `Project::new` pede `owner_id` e `name` porque `id`, `archived` e `created_at` já têm default.

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

`Task` ↔ `Tag` é implícito, então o Dinoco gera uma tabela pivô escondida e um field virtual `task_id`/`tag_id` de cada lado, que você pode atribuir diretamente. Conecte uma `Tag` nova à task existente durante o insert:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

O mesmo vale para cada item de um insert em lote:

```rust
let mut tags = vec![Tag::new("rust".to_string()), Tag::new("database".to_string())];

for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>().values(&tags).execute(&client).await?;
```

Quando os dois endpoints já existem, use `connect` em vez de mexer no field virtual — isto cria mais uma tag e a conecta separadamente:

```rust
let review_tag = Tag::new("review".to_string());
dinoco::insert_into::<Tag>().values(&review_tag).execute(&client).await?;

dinoco::update::<Task>()
    .where_(|item| item.id.eq(task.id))
    .update(|item| item.tag_id.connect(&review_tag.id))
    .execute(&client)
    .await?;
```

Carregue todas as tags vinculadas pelo field de relação, igual a qualquer outro include:

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|item| item.id.eq(task.id))
    .includes(|item| item.tags())
    .execute(&client)
    .await?;
```

`disconnect` remove apenas a linha da pivô selecionada — nunca apaga a `Tag` em si:

```rust
dinoco::update::<Task>()
    .where_(|item| item.id.eq(task.id))
    .update(|item| item.tag_id.disconnect(&review_tag.id))
    .execute(&client)
    .await?;
```

> [!NOTE]
> Três operações diferentes, três efeitos diferentes: atribuir o field virtual (`tag.task_id = Some(...)`) durante o insert **cria** a `Tag` e sua linha na pivô em uma única instrução. `connect` cria **apenas** a linha da pivô, para dois endpoints que já existem. `disconnect` remove **apenas** a linha da pivô — a tag e a task continuam intactas. Fields virtuais como `task_id` sempre voltam como `None` na leitura; eles existem para serem escritos, não lidos.

## Checklist antes de colocar em produção

1. Mantenha toda URL de conexão em variável de ambiente — nunca um literal em `schema.dinoco`.
2. Leia o `up.sql`/`down.sql` gerado antes de rodar uma migration contra um banco real.
3. Trate `Option`/`None` de `find_first` como um resultado normal, distinto de um `Err`.
4. Sempre combine `.take(...)` com um `.order_by(...)` explícito em queries paginadas.
5. Limite includes de listas com `.take(...)` — um `.includes(...)` sem limite numa relação grande ainda é uma query em lote, mas com um result set sem limite.
6. Use `.read_in_primary()` quando uma leitura precisa observar um write que acabou de acontecer na primária.
7. Preencha os IDs virtuais de many-to-many gerados durante `insert_into`/`insert_many` para endpoints novos, ou use `connect`/`disconnect` para endpoints que já existem.
8. Nunca use `Float` para um field que é foreign key de um id `Snowflake` — igualdade de ponto flutuante não se comporta do jeito que um join precisa.
