# Testes

O Dinoco vem com um ambiente de teste: um banco de dados de verdade para os seus testes, sem servidor, sem Docker e sem limpeza. `create_test_ambient()` abre um banco SQLite novo na memória, cria toda tabela, índice e foreign key declarados em `dinoco/schema.dinoco`, e devolve um `DinocoClient` comum. Depois, `setup_test_methods::<M>(&client)` permite que um teste acompanhe cada insert, update, delete e find que toca o model `M` — assim um teste de rota consegue verificar o que a rota fez no banco, e não só o que ela respondeu.

```rust
use dinoco::{create_test_ambient, remove_test_methods, setup_test_methods};

let client = create_test_ambient().await?;

setup_test_methods::<User>(&client)
    .on_insert(|inserted, query, error| { /* ... */ })
    .on_update(|affected, query, error| { /* ... */ })
    .on_delete(|affected, query, error| { /* ... */ })
    .on_find(|rows, query, error| { /* ... */ });

// ... chame a rota ou a função sendo testada ...

remove_test_methods::<User>(&client);
```

## Crie um ambiente de teste

```rust
use dinoco::{create_test_ambient, find_first, insert_into};

#[tokio::test]
async fn creates_a_user() -> anyhow::Result<()> {
    let client = create_test_ambient().await?;

    insert_into::<User>().values(&user).execute(&client).await?;

    let stored = find_first::<User>()
        .where_(|x| x.email.eq("ana@example.com"))
        .execute(&client)
        .await?;
    assert!(stored.is_some());

    Ok(())
}
```

`create_test_ambient()` retorna `anyhow::Result<DinocoClient>`, o mesmo tipo que o `connect()` retorna, então código que recebe `&DinocoClient`, `Arc<DinocoClient>` ou um state da aplicação que o envolve funciona sem mudanças. Todo builder funciona contra ele: `find_first`, `find_many`, `.includes(...)`, `count`, `exists`, `find_batch`, todos os builders de insert/update/delete e `transaction(...)`.

### O que é criado

As tabelas são planejadas a partir do schema pelo mesmo motor por trás do `dinoco migrate generate` e depois compiladas para SQLite:

| No schema | No banco de teste |
| --- | --- |
| `model` | Uma tabela com o nome de tabela do model |
| Campos escalares, `?` | Colunas, `NOT NULL` a menos que sejam opcionais |
| `@id`, `@@ids([...])` | Primary key, simples ou composta |
| `@default(...)` | Default da coluna (`now()`, `autoincrement()`, valores de enum, literais); `uuid()`/`snowflake()` são gerados pelo Dinoco no insert, como em produção |
| `@unique`, `@@uniques([...])` | Índices unique, então um duplicado falha com `CreateError::UniqueViolation` |
| `@index`, `@@indexes([...])` | Índices comuns |
| `@relation(...)` | Foreign keys, aplicadas (o SQLite roda com `foreign_keys = ON`) |
| Many-to-many implícito | A tabela pivô |
| `enum` | Uma coluna de texto com a grafia da variante no schema |

**É sempre SQLite, não importa o `config.database`.** Um projeto PostgreSQL ou MySQL recebe as mesmas tabelas, constraints e índices, então os testes exercitam as mesmas regras que o banco de produção aplica.

### Isolamento e testes em paralelo

Cada chamada a `create_test_ambient()` recebe seu próprio banco vazio. O `cargo test` roda os testes em threads paralelas, e nenhum deles enxerga as rows dos outros — então os testes não precisam de limpeza nem de ordem:

```rust
#[tokio::test]
async fn first() -> anyhow::Result<()> {
    let client = create_test_ambient().await?; // vazio
    // ...
    Ok(())
}

#[tokio::test]
async fn second() -> anyhow::Result<()> {
    let client = create_test_ambient().await?; // também vazio, mesmo enquanto `first` roda
    // ...
    Ok(())
}
```

Dentro de um ambiente, o banco é compartilhado por toda conexão do pool e toda transaction daquele client, como um banco em arquivo: leituras que rodam ao mesmo tempo (`tokio::join!`, `.includes(...)`) e writes feitos numa transaction commitada enxergam os mesmos dados. O banco é liberado quando o client é descartado (com um `Arc<DinocoClient>`, quando o último `Arc` é).

> [!TIP]
> Crie um ambiente por teste. Montar um é rápido (o schema é compilado e as tabelas criadas na memória), e um banco novo mantém cada teste independente. Compartilhar um ambiente entre testes por meio de um static traz de volta os problemas de ordem que o ambiente existe para eliminar.

### Opções

`create_test_ambient()` lê `dinoco/schema.dinoco` relativo ao diretório atual. No `cargo test`, é a raiz do crate sendo testado — onde o `dinoco init` coloca o schema. `TestAmbient` cobre os outros casos:

```rust
use dinoco::TestAmbient;

// Um schema em outro lugar. O arquivo continua precisando se chamar `schema.dinoco`.
let client = TestAmbient::new()
    .schema(concat!(env!("CARGO_MANIFEST_DIR"), "/../app/dinoco/schema.dinoco"))
    .create()
    .await?;

// Um schema que usa `config.workspace`: escolha o workspace cujas configurações valem.
let client = TestAmbient::new().workspace("dev").create().await?;
```

| Método | Padrão | Efeito |
| --- | --- | --- |
| `TestAmbient::new()` | — | O mesmo que `create_test_ambient()` até você mudar algo |
| `.schema(path)` | `dinoco/schema.dinoco` | Arquivo de schema usado para montar o banco. Os arquivos que ele importa são resolvidos relativos a ele |
| `.workspace(name)` | nenhum | Aplica a configuração daquele workspace. Um nome desconhecido é erro |
| `.create().await` | — | Monta o banco e devolve o client |

Da configuração (depois de aplicar o workspace), `with_logger` e `query_mode` são respeitados, então um schema com `with_logger = true` imprime todo SQL nos testes também. `database`, `database_url`, `connection`, tamanhos de pool e `read_replicas` são ignorados: o ambiente nunca abre uma conexão de rede e não precisa de variáveis de ambiente.

## O connect() gerado nos testes

O `dinoco/mod.rs` que a geração de código escreve tem três funções de conexão:

```rust
/// Connects to the database configured in `schema.dinoco`. When this crate is
/// compiled for its own tests (`cfg(test)`), it returns [`connect_test`]
/// instead, so code under test never reaches the real database.
pub async fn connect() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {
    if cfg!(test) {
        return connect_test().await;
    }

    connect_database().await
}

/// A fresh in-memory SQLite database with every table of `schema.dinoco`,
/// isolated from every other call. See `dinoco::create_test_ambient`.
pub async fn connect_test() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {
    ::dinoco::TestAmbient::new()
        .schema(concat!(env!("CARGO_MANIFEST_DIR"), "/dinoco/schema.dinoco"))
        .create()
        .await
}

/// Connects to the configured database, even under `cfg(test)`.
pub async fn connect_database() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {
    let database_url = std::env::var("DATABASE_URL")?;
    // ... o adapter, as réplicas, o logger e o query mode do `config` ...
}
```

| Função | Retorna |
| --- | --- |
| `connect()` | `connect_test()` quando o crate é compilado com `cfg(test)`, `connect_database()` caso contrário |
| `connect_test()` | Um ambiente de teste novo, montado a partir do `dinoco/schema.dinoco` do crate |
| `connect_database()` | O banco real, como o `connect()` fazia antes da 2.0.4 |

Então código que chama `connect()` não precisa de nenhuma mudança para ser testado. O `main` continua conectando no banco real, e a mesma chamada dentro de testes `#[cfg(test)]` recebe um banco isolado na memória, sem precisar de `DATABASE_URL`:

```rust
// src/main.rs
#[path = "../dinoco/mod.rs"]
mod database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = database::connect().await?; // o banco real
    // ...
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::database;

    #[tokio::test]
    async fn each_call_is_isolated() -> anyhow::Result<()> {
        let first = database::connect().await?;  // ambiente de teste
        let second = database::connect().await?; // outro ambiente de teste, vazio

        dinoco::insert_into::<Project>().value(&Project::new("A".to_string())).execute(&first).await?;
        assert!(dinoco::find_many::<Project>().execute(&second).await?.is_empty());

        Ok(())
    }
}
```

Quando o schema usa `config.workspace`, `connect_test()` aplica o workspace para o qual os models foram gerados (`.workspace("dev")`) — o mesmo em que `connect_database()` conecta.

O que saber sobre o `cfg(test)`:

- Ele só é ativado enquanto o Rust compila **os testes do próprio crate**: módulos `#[cfg(test)]` e funções `#[test]` dentro de `src/`. Testes de integração em `tests/`, benches e outros crates que dependem do seu compilam ele sem `cfg(test)`, então lá o `connect()` conecta no banco real. A partir deles, chame `connect_test()` direto (por exemplo, por uma função `pub` do seu crate de biblioteca) ou `dinoco::create_test_ambient()`.
- `connect_test()` encontra o schema via `CARGO_MANIFEST_DIR`, o diretório do `Cargo.toml` do crate — onde o `dinoco init` coloca o `dinoco/`. Se o seu schema fica em outro lugar (por exemplo, na raiz de um Cargo workspace enquanto o crate é um membro), monte o client com `TestAmbient::new().schema(...)`.
- Precisa do banco real num teste unitário (digamos, um teste marcado com `#[ignore]` que você roda contra um banco de staging)? Chame `connect_database()`.

## Teste uma rota

Testes de rota montam o state da aplicação do mesmo jeito que o `main`, pelo `connect()` gerado. Para uma app Axum cujo router recebe `Arc<DinocoClient>`, um teste de rota envia uma requisição com `tower::ServiceExt::oneshot` e lê a resposta:

```toml
# Cargo.toml
[dev-dependencies]
http-body-util = "0.1"
serde_json = "1"
tower = { version = "0.5", features = ["util"] }
```

```rust
use std::sync::{Arc, Mutex};

use axum::{Router, body::Body, http::{Method, Request, StatusCode}};
use dinoco::setup_test_methods;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn app() -> anyhow::Result<(Router, Arc<dinoco::DinocoClient>)> {
    // O mesmo `connect()` do `main`: sob `cfg(test)` ele é o ambiente de teste.
    let client = Arc::new(database::connect().await?);
    Ok((routes::router(client.clone()), client))
}

async fn send(app: &Router, method: Method, uri: &str, body: Option<Value>) -> anyhow::Result<(StatusCode, Value)> {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))?;
    let response = app.clone().oneshot(request).await?;
    let status = response.status();
    let bytes = response.into_body().collect().await?.to_bytes();
    let body = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes)? };

    Ok((status, body))
}

#[tokio::test]
async fn create_project_inserts_the_project_and_its_tasks_in_one_transaction() -> anyhow::Result<()> {
    let (app, client) = app().await?;
    let inserts = Arc::new(Mutex::new(Vec::new()));

    let log = inserts.clone();
    setup_test_methods::<Project>(&client).on_insert(move |rows, query, _| {
        log.lock().unwrap().push((query.table, rows.map(<[_]>::len), query.in_transaction));
    });
    let log = inserts.clone();
    setup_test_methods::<Task>(&client).on_insert(move |rows, query, _| {
        log.lock().unwrap().push((query.table, rows.map(<[_]>::len), query.in_transaction));
    });

    let (status, body) = send(
        &app,
        Method::POST,
        "/projects",
        Some(json!({ "name": "Launch", "tasks": ["Write docs", "Ship"] })),
    )
    .await?;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["tasks"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        *inserts.lock().unwrap(),
        [("project", Some(1), true), ("task", Some(1), true), ("task", Some(1), true)],
    );

    Ok(())
}
```

A última asserção verifica algo que a resposta sozinha não prova: a rota inseriu exatamente um projeto e duas tasks, tudo dentro de uma transaction. A versão completa dessa suíte está em `examples/axum/src/tests.rs` no repositório do Dinoco. A mesma abordagem funciona com Actix Web (`actix_web::test::init_service`) ou um service Tower puro. Fora do `cfg(test)`, monte o state com `connect_test()` ou `create_test_ambient()`.

Para criar os dados que uma rota precisa, use os builders direto no mesmo client antes de enviar a requisição:

```rust
let project = insert_into::<Project>()
    .value(&Project::new("Existing".to_string()))
    .returning::<Project>()
    .execute(&*client)
    .await?;

let (status, _) = send(&app, Method::GET, &format!("/projects/{}", project.id), None).await?;
assert_eq!(status, StatusCode::OK);
```

## Observe as queries por model

### Instale callbacks

`setup_test_methods::<M>(&client)` devolve um builder que instala callbacks do model `M` naquele client. Eles rodam depois de toda operação na tabela de `M`:

- operações rodadas no próprio client (`.execute(&client)`);
- operações rodadas dentro de uma `transaction(...)` aberta nesse client (`.execute(tx)`);
- `M` carregado como relação por `.includes(...)` em outro model;
- rows de `M` inseridas como relação aninhada no insert de outro model.

```rust
setup_test_methods::<User>(&client)
    .on_insert(|inserted, query, error| println!("insert em {}: {inserted:?} {error:?}", query.table))
    .on_update(|affected, query, _| println!("{} atualizou {affected:?} row(s)", query.table))
    .on_delete(|affected, query, _| println!("{} removeu {affected:?} row(s)", query.table))
    .on_find(|rows, query, _| println!("{} retornou {rows:?} row(s)", query.sql));
```

Os callbacks são restritos ao model: operações em `Post` nunca chegam aos callbacks de `User`. E também ao client: outro `create_test_ambient()` no mesmo teste não os enxerga.

Todo método é opcional; instale só o que o teste precisa. Chamar `setup_test_methods::<M>` de novo para o mesmo model mantém os callbacks já instalados e adiciona os novos. Definir um callback que já existe o substitui:

```rust
setup_test_methods::<User>(&client).on_insert(|_, _, _| println!("A"));
setup_test_methods::<User>(&client).on_find(|_, _, _| println!("B"));   // on_insert continua imprimindo "A"
setup_test_methods::<User>(&client).on_insert(|_, _, _| println!("C")); // substitui "A"
```

### Remova callbacks

`remove_test_methods::<M>(&client)` remove todo callback de `M` do client e mantém os callbacks dos outros models. Chamar quando nada está instalado não faz nada.

```rust
setup_test_methods::<User>(&client).on_insert(|_, _, _| println!("user"));
setup_test_methods::<Post>(&client).on_insert(|_, _, _| println!("post"));

remove_test_methods::<User>(&client);
// inserir um User não imprime nada; inserir um Post continua imprimindo "post"
```

Use para observar só uma etapa de um teste mais longo — por exemplo, depois de criar os dados e antes de chamar a rota:

```rust
seed_users(&client).await?;               // não observado

setup_test_methods::<User>(&client).on_update(/* ... */);
send(&app, Method::PATCH, "/users/1", Some(body)).await?; // observado
remove_test_methods::<User>(&client);

send(&app, Method::GET, "/users", None).await?; // não observado
```

Com um ambiente novo por teste, os callbacks são descartados junto com o client no fim do teste, então removê-los só é necessário dentro de um teste.

### O que cada callback recebe

Todo callback recebe três argumentos: o resultado, a query executada e o erro. Exatamente um entre o resultado e o erro é `Some`.

| Callback | Resultado (1º argumento) | Roda depois de |
| --- | --- | --- |
| `on_insert` | `Option<&[serde_json::Value]>`: as rows inseridas, um objeto JSON por row, com os nomes das colunas como chaves | todo `INSERT` em `M`: `insert_into`, `insert_many`, com ou sem `.returning`/`.pluck`, e inserts de relações aninhadas |
| `on_update` | `Option<usize>`: rows alteradas | `update`, `update_many`, `find_and_update` |
| `on_delete` | `Option<usize>`: rows removidas | `delete`, `delete_many` |
| `on_find` | `Option<usize>`: rows retornadas | `find_first`, `find_many` e toda query de relação que carrega `M` por `.includes(...)` |

`insert_many` chama `on_insert` uma vez para o lote inteiro, com todas as rows no slice. As rows inseridas contêm os valores que o Dinoco enviou, incluindo ids gerados por `uuid()`/`snowflake()`. Valores que o próprio banco preenche (defaults `autoincrement()`, `now()`) não aparecem.

Uma query de relação retorna uma row para cada pai que aponta para uma row relacionada: carregar o autor de dois posts escritos pela mesma pessoa reporta 2 rows.

O segundo argumento é um `ExecutedQuery`:

| Campo | Tipo | Significado |
| --- | --- | --- |
| `table` | `&'static str` | Tabela alvo do statement |
| `sql` | `String` | O SQL que o Dinoco compilou para o statement (sintaxe SQLite no ambiente) |
| `params` | `Vec<DinocoValue>` | Parâmetros vinculados, em ordem |
| `in_transaction` | `bool` | `true` quando o statement rodou por um contexto `transaction(...)` |

O terceiro argumento é `Option<&DatabaseError>`:

| Método | Retorna |
| --- | --- |
| `error.constraint()` | `Option<DatabaseConstraintError>`: `UniqueViolation`, `ForeignKeyViolation`, `NotNullViolation`, `CheckViolation` |
| `error.constraint_details()` | `&ConstraintDetails`: `table`, `constraint`, `columns` como o driver os reportou |
| `error.original()` | O erro do driver (`rusqlite::Error` no ambiente) |

O callback só observa a falha. O builder continua retornando seu próprio erro tipado para quem chamou (`CreateError`, `UpdateError`, `AtomicUpdateError`, …), então o tratamento de erro da rota roda exatamente como em produção.

## Padrões comuns

### Verifique que uma rota gravou exatamente o que deveria

```rust
let inserted = Arc::new(Mutex::new(Vec::new()));
let log = inserted.clone();
setup_test_methods::<Order>(&client).on_insert(move |rows, _, _| {
    log.lock().unwrap().extend(rows.unwrap_or_default().iter().cloned());
});

send(&app, Method::POST, "/orders", Some(json!({ "sku": "A-1", "quantity": 2 }))).await?;

let inserted = inserted.lock().unwrap();
assert_eq!(inserted.len(), 1);
assert_eq!(inserted[0]["quantity"], 2);
```

### Verifique que uma rota não tocou no banco

A validação deve rejeitar uma entrada ruim antes de qualquer query rodar:

```rust
let touched = Arc::new(Mutex::new(false));
let flag = touched.clone();
setup_test_methods::<Project>(&client).on_insert(move |_, _, _| *flag.lock().unwrap() = true);

let (status, _) = send(&app, Method::POST, "/projects", Some(json!({ "name": "   " }))).await?;

assert_eq!(status, StatusCode::BAD_REQUEST);
assert!(!*touched.lock().unwrap());
```

### Pegue queries N+1

`.includes(...)` carrega uma relação para todos os pais numa única query. Contar as chamadas de `on_find` no model relacionado pega código que o carrega num loop:

```rust
let queries = Arc::new(Mutex::new(0));
let counter = queries.clone();
setup_test_methods::<Task>(&client).on_find(move |_, _, _| *counter.lock().unwrap() += 1);

send(&app, Method::GET, "/projects", None).await?;

assert_eq!(*queries.lock().unwrap(), 1, "as tasks precisam vir de uma única query de include");
```

### Verifique o erro que uma rota encontrou

```rust
let conflicts = Arc::new(Mutex::new(Vec::new()));
let log = conflicts.clone();
setup_test_methods::<User>(&client).on_insert(move |_, _, error| {
    if let Some(error) = error {
        log.lock().unwrap().push((error.constraint(), error.constraint_details().columns.clone()));
    }
});

send(&app, Method::POST, "/users", Some(json!({ "email": "taken@example.com" }))).await?;
let (status, _) = send(&app, Method::POST, "/users", Some(json!({ "email": "taken@example.com" }))).await?;

assert_eq!(status, StatusCode::CONFLICT);
assert_eq!(
    *conflicts.lock().unwrap(),
    [(Some(DatabaseConstraintError::UniqueViolation), vec!["email".to_string()])],
);
```

### Verifique as rows afetadas

```rust
let deleted = Arc::new(Mutex::new(Vec::new()));
let log = deleted.clone();
setup_test_methods::<Task>(&client).on_delete(move |affected, _, _| log.lock().unwrap().push(affected));

send(&app, Method::DELETE, &format!("/tasks/{id}"), None).await?; // 204
send(&app, Method::DELETE, &format!("/tasks/{id}"), None).await?; // 404

assert_eq!(*deleted.lock().unwrap(), [Some(1), Some(0)]);
```

### Mantenha os helpers num lugar só

Um pequeno gravador deixa os testes de rota curtos quando vários models são observados:

```rust
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<String>>>);

impl Events {
    fn record<M: dinoco::DinocoEntity>(&self, client: &DinocoClient) {
        let (insert, update, delete) = (self.clone(), self.clone(), self.clone());
        setup_test_methods::<M>(client)
            .on_insert(move |rows, q, _| insert.push(format!("insert {} {:?}", q.table, rows.map(<[_]>::len))))
            .on_update(move |n, q, _| update.push(format!("update {} {n:?}", q.table)))
            .on_delete(move |n, q, _| delete.push(format!("delete {} {n:?}", q.table)));
    }

    fn push(&self, event: String) {
        self.0.lock().unwrap().push(event);
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

let events = Events::default();
events.record::<Project>(&client);
events.record::<Task>(&client);

send(&app, Method::POST, "/projects", Some(json!({ "name": "Launch", "tasks": ["Ship"] }))).await?;
assert_eq!(events.take(), ["insert project Some(1)", "insert task Some(1)"]);
```

## Dentro de transactions

Uma `transaction(...)` aberta no client reporta suas operações para os mesmos callbacks, com `query.in_transaction` igual a `true`. Um callback roda assim que seu statement termina, não no commit, então ele também roda para trabalho que depois sofre rollback. Para verificar que uma rota que falhou não deixou nada para trás, consulte o banco depois da requisição em vez de confiar nos callbacks:

```rust
let (status, _) = send(&app, Method::POST, "/projects", Some(json!({ "name": "Launch", "tasks": [""] }))).await?;

assert_eq!(status, StatusCode::BAD_REQUEST);
assert_eq!(count::<Project>().execute(&*client).await?.total, 0);
```

## Escrevendo callbacks

- Callbacks precisam ser `Fn + Send + Sync + 'static`: acumule num `Arc<Mutex<_>>` (ou num contador atômico) do qual o teste guarda um clone, como nos exemplos acima.
- Eles rodam de forma síncrona logo depois do statement, na task que o executou. Mantenha-os curtos e não use `.await` dentro deles (eles não são async).
- Um panic dentro de um callback falha o teste quando a rota roda na própria task do teste (`oneshot`, chamadas diretas). Se o código testado cria suas próprias tasks, um panic ali pode se perder — então grave os valores no callback e faça as asserções depois da requisição.
- Callbacks nunca mudam o resultado da operação. Eles não conseguem fazer uma query falhar nem devolver rows diferentes.

## Limites

- `count`, `exists` e `find_batch` não disparam `on_find`.
- A tabela pivô de uma relação many-to-many implícita não tem model, então seus inserts e deletes (`connect`, `disconnect`) não podem ser observados. As operações das próprias pontas podem.
- O ambiente monta direto a forma final do schema. Ele não reexecuta os arquivos de `dinoco/migrations/`, então nada escrito à mão numa migration (rows de seed, triggers, SQL extra numa migration manual) está lá. Insira o que o teste precisa no próprio teste.
- SQLite não é o seu banco de produção. Recursos específicos de PostgreSQL ou MySQL, como o ranking de full-text, collations e limites exatos de tipo, se comportam do jeito do SQLite. Mantenha alguns testes contra o banco real para esses casos.

## Solução de problemas

| Mensagem | Causa |
| --- | --- |
| ``failed to compile `dinoco/schema.dinoco` for the test ambient`` | O schema não está nesse caminho relativo ao diretório atual, ou tem um erro. Rode o `cargo test` a partir do crate dono do schema, ou passe `.schema(concat!(env!("CARGO_MANIFEST_DIR"), "/dinoco/schema.dinoco"))` |
| `The main schema file must be named schema.dinoco` | `.schema(path)` precisa apontar para um arquivo chamado `schema.dinoco` |
| ``workspace `x` was not found`` | `.workspace(name)` cita um workspace que o `config` do schema não declara |
| `transaction context used outside its transaction closure` | Um handle `tx` escapou da closure de `transaction(...)`. Não é específico do ambiente |

## Referência da API

| Item | Assinatura |
| --- | --- |
| `create_test_ambient` | `async fn create_test_ambient() -> anyhow::Result<DinocoClient>` |
| `TestAmbient::new` | `fn new() -> TestAmbient` |
| `TestAmbient::schema` | `fn schema(self, path: impl AsRef<Path>) -> TestAmbient` |
| `TestAmbient::workspace` | `fn workspace(self, name: impl Into<String>) -> TestAmbient` |
| `TestAmbient::create` | `async fn create(self) -> anyhow::Result<DinocoClient>` |
| `setup_test_methods` | `fn setup_test_methods<M: DinocoEntity>(client: &DinocoClient) -> TestMethods<'_, M>` |
| `TestMethods::on_insert` | `fn on_insert(self, f: impl Fn(Option<&[serde_json::Value]>, &ExecutedQuery, Option<&DatabaseError>) + Send + Sync + 'static) -> Self` |
| `TestMethods::on_update` | `fn on_update(self, f: impl Fn(Option<usize>, &ExecutedQuery, Option<&DatabaseError>) + Send + Sync + 'static) -> Self` |
| `TestMethods::on_delete` | mesma assinatura de `on_update` |
| `TestMethods::on_find` | mesma assinatura de `on_update` |
| `remove_test_methods` | `fn remove_test_methods<M: DinocoEntity>(client: &DinocoClient)` |
