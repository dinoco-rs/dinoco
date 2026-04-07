# Documentação Dinoco

O **Dinoco** é um toolkit de banco de dados (ORM) de alta performance para Rust, focado em segurança de tipos, produtividade e baixa latência. Ele utiliza geração de código (codegen) para garantir que suas queries sejam validadas em tempo de compilação.

---

## 1. Configuração do Cliente

O `DinocoClient` é o ponto de entrada para todas as operações. Você pode configurar comportamentos globais como loggers e geradores de ID.

### Exemplo de Inicialização

```rust
let config = DinocoClientConfig::default()
    .with_snowflake_node_id(7) // Define o ID do nó para geração de IDs únicos (Snowflake)
    .with_query_logger(DinocoQueryLogger::stdout(
        DinocoQueryLoggerOptions::verbose()
    ));

let client = app::create_connection(config).await?;
```

---

## 2. O que o Codegen gera

Para cada `model` definido no seu `schema.dinoco`, o motor do Dinoco gera:

- **Struct Principal**: (ex: `User`) Implementa `Model`, `Serialize`, `Deserialize`.
- **DSL de Filtro**: (ex: `UserWhere`) Usado dentro do método `.cond()`.
- **DSL de Update Atômico**: (ex: `UserUpdate`) Usado dentro do método `.update()` de `find_and_update`.
- **DSL de Inclusão**: (ex: `UserInclude`) Para carregar relações.
- **Traits de Escrita**: `InsertModel`, `UpdateModel`, `FindAndUpdateModel`, etc.

---

## 3. Operações de Leitura

### `find_many::<M>()` e `find_first::<M>()`

Ambos expõem uma interface fluida para construção de queries.

| Método                  | Descrição                                                  |
| :---------------------- | :--------------------------------------------------------- |
| `.select::<T>()`        | Define uma projeção customizada (usando a macro `Extend`). |
| `.cond(`                | w                                                          |
| `.includes(             | i                                                          |
| `.take(n)` / `.skip(n)` | Paginação (Limit/Offset).                                  |
| `.order_by(             | w                                                          |
| `.read_in_primary()`    | Força a leitura no banco principal (ignora réplicas).      |

#### Exemplo: Busca Complexa com Projeção e Relações

```rust
let employees = find_many::<Employee>()
    .select::<EmployeeListItem>()
    .order_by(|x| x.name.asc())
    .includes(|x| x.department().select::<DepartmentListItem>())
    .includes(|x| x.assignedTasks().select::<TaskListItem>())
    .read_in_primary()
    .execute(&client)
    .await?;
```

### `count::<M>()`

Retorna a quantidade de registros que satisfazem uma condição.

```rust
let total = count::<Task>()
    .cond(|x| x.status.eq(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

---

## 4. Projeções com `Extend`

Use o derive `Extend` para criar views parciais dos seus modelos. Isso otimiza o SQL para selecionar apenas as colunas necessárias.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Employee)]
struct EmployeeListItem {
    id: String,
    email: String,
    // Você pode incluir campos que são relações se eles também forem Extend
    department: Option<DepartmentListItem>,
}
```

---

## 5. Operações de Escrita

### `insert_into` e `insert_many`

O Dinoco aceita instâncias do modelo para inserção.

```rust
// Inserção única
insert_into::<Department>()
    .values(department_named("Engineering", None))
    .execute(&client)
    .await?;

// Inserção em lote (Batch)
insert_many::<Skill>()
    .values(vec![skill_named("Rust"), skill_named("TypeScript")])
    .execute(&client)
    .await?;
```

### `update`

Atualiza registros baseados em uma condição. Você pode passar o objeto modificado para o método `.values()`.

```rust
let mut task = task_existente;
task.status = TaskStatus::REVIEW;

update::<Task>()
    .cond(|x| x.id.eq(task.id.clone()))
    .values(task)
    .execute(&client)
    .await?;
```

### `find_and_update`

Atualiza um único registro com operações atômicas no banco e devolve o item atualizado. Esse fluxo evita o padrão vulnerável de `find_first` + mutação em memória + `update`.

```rust
let task = find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

Você também pode projetar o retorno:

```rust
let user = find_and_update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .update(|x| x.score.increment(1.0_f64))
    .returning::<UserScoreView>()
    .execute(&client)
    .await?;
```

Operações disponíveis em `ModelUpdate`:

- `set`
- `increment`
- `decrement`
- `multiply`
- `division`

Se nenhuma linha satisfizer a condição, o retorno será `DinocoError::RecordNotFound`.

### `delete_many`

Remove múltiplos registros. Comum em seeds de teste ou limpeza de cache.

```rust
delete_many::<TaskAssignees>().execute(&client).await?;
```

---

## 6. Filtros Disponíveis (Where)

Os campos escalares expõem operadores dependendo do seu tipo:

- **Gerais**: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in_values`, `is_null`.
- **Strings**: `includes` (LIKE %val%), `starts_with`, `ends_with`.
- **Lógicos**: `.and()`, `.or()`.

---

## 7. O Schema (`schema.dinoco`)

Linguagem declarativa para definir sua infraestrutura.

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
}

enum TaskStatus {
    TODO
    DOING
    REVIEW
    DONE
}

model Task {
    id           String     @id @default(snowflake())
    title        String
    status       TaskStatus @default(TODO)
    projectId    String
    project      Project    @relation(fields: [projectId], references: [id])
}
```

### Decoradores Suportados

- `@id`: Define a chave primária.
- `@unique`: Cria um índice de unicidade.
- `@default(...)`: Valores padrão como `now()`, `uuid()`, `snowflake()`, `autoincrement()`.
- `@relation(...)`: Define chaves estrangeiras e comportamentos de `onDelete`.

---

## 8. Logging e Debug

O logger do Dinoco fornece visibilidade total sobre o SQL gerado e o tempo de execução.

```text
[Dinoco Query | adapter=sqlite | duration=1.2ms] query=SELECT "id", "name" FROM "Employee" WHERE "status" = ?1 params=["REVIEW"]
```

Para implementar um logger customizado (ex: enviar logs para o Grafana Loki):

1. Implemente a trait `DinocoQueryLogWriter`.
2. Injete no `DinocoClientConfig` usando `DinocoQueryLogger::custom(meu_writer, options)`.

3. Versions supported:

- SQLite: **>= 3.25**
- PostgreSQL: **>= 10**
- MySQL: **>= 8.0**
