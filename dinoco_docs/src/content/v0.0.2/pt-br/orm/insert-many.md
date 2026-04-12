# insert_many

Usado para inserção em lote.

---

## O que você pode fazer

- `.values(Vec&lt;M&gt;)`: define os registros que serão inseridos em lote.
- `.returning::&lt;T&gt;()`: muda o retorno para uma lista tipada dos itens inseridos.
- `.execute(&client)`: executa a operação em lote.

## Retorno

Sem `.returning::&lt;T&gt;()`, o retorno é:

```rust
DinocoResult<()>
```

Com `.returning::&lt;T&gt;()`, o retorno passa a ser:

```rust
DinocoResult<Vec<T>>
```

## Exemplo simples

```rust
dinoco::insert_many::<User>()
    .values(vec![
        User { id: "u1".into(), email: "a@acme.com".into(), name: "A".into() },
        User { id: "u2".into(), email: "b@acme.com".into(), name: "B".into() },
    ])
    .execute(&client)
    .await?;
```

## Exemplo com retorno

```rust
let created = dinoco::insert_many::<User>()
    .values(vec![
        User { id: 2, name: "Ana".to_string() },
        User { id: 3, name: "Caio".to_string() },
    ])
    .returning::<User>()
    .execute(&client)
    .await?;
```

## Exemplo com worker

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-created", |job| async move {
        println!("Usuários criados no lote: {}", job.data.len());
        job.success();
    })
    .run()
    .await?;

dinoco::insert_many::<User>()
    .values(vec![
        User { id: 2, name: "Ana".to_string() },
        User { id: 3, name: "Caio".to_string() },
    ])
    .enqueue("user.batch-created")
    .execute(&client)
    .await?;
```

Veja mais sobre workers em [**`queues`**](/v0.0.2/orm/queues).

## Exemplo com relações aninhadas

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Team)]
#[insertable]
struct TeamWithMembers {
    id: String,
    name: String,
    members: Vec<Member>,
}

dinoco::insert_many::<Team>()
    .values(vec![
        TeamWithMembers {
            id: "team-11".into(),
            name: "Data".into(),
            members: vec![
                Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() },
                Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
            ],
        },
        TeamWithMembers {
            id: "team-12".into(),
            name: "DX".into(),
            members: vec![
                Member { id: "member-13".into(), name: "Caio".into(), teamId: "legacy".into() },
            ],
        },
    ])
    .execute(&client)
    .await?;
```

## Exemplo com conexões aninhadas

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}

dinoco::insert_many::<Article>()
    .values(vec![
        ArticleWithLabels {
            id: "article-11".into(),
            title: "Connect Multiple".into(),
            labels: vec![
                ArticleConnection::Label("label-11".into()),
                ArticleConnection::Label("label-12".into()),
            ],
        },
        ArticleWithLabels {
            id: "article-12".into(),
            title: "Connect Batch".into(),
            labels: vec![
                ArticleConnection::Label("label-10".into()),
            ],
        },
    ])
    .execute(&client)
    .await?;
```

## Exemplo com ArticleConnection

Quando os itens relacionados já existem, use o enum gerado pelo codegen dentro do payload.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}

let values = vec![
    ArticleWithLabels {
        id: "article-21".into(),
        title: "Connect Multiple".into(),
        labels: vec![
            ArticleConnection::Label("label-11".into()),
            ArticleConnection::Label("label-12".into()),
        ],
    },
    ArticleWithLabels {
        id: "article-22".into(),
        title: "Connect Batch".into(),
        labels: vec![ArticleConnection::Label("label-10".into())],
    },
];

dinoco::insert_many::<Article>()
    .values(values)
    .execute(&client)
    .await?;
```

## Exemplo com Extend #[insertable]

`insert_many` também pode receber payloads mais ricos via `.values(...)` quando a struct estiver marcada com `#[insertable]`. Isso é útil para criar os pais e suas relações aninhadas em um fluxo recursivo só.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<Label>,
}

dinoco::insert_many::<Article>()
    .values(vec![
        ArticleWithLabels {
            id: "article-11".into(),
            title: "Connect Multiple".into(),
            labels: vec![
                Label { id: "label-11".into(), name: "orm".into() },
                Label { id: "label-12".into(), name: "rust".into() },
            ],
        },
        ArticleWithLabels {
            id: "article-12".into(),
            title: "Connect Batch".into(),
            labels: vec![
                Label { id: "label-10".into(), name: "backend".into() },
            ],
        },
    ])
    .execute(&client)
    .await?;
```

## Observações

- `#[insertable]` em uma struct `Extend` permite que `.values(...)` insira relações novas de forma recursiva.
- Para conexões com registros já existentes, use o enum `ModelConnection` gerado pelo codegen.
- Relações novas podem usar `Vec&lt;ModelRelacionado&gt;` ou outros payloads `Extend` marcados com `#[insertable]`.
- Conexões existentes funcionam bem em cenários many-to-many e também em fluxos suportados pelo model gerado.

## Próximos passos

- [**Derives**](/v0.0.2/core/derives): entenda como montar payloads com `Extend` e `#[insertable]`.
- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into): inserção única.
- [**`update::&lt;M&gt;()`**](/v0.0.2/orm/update): atualização com filtro.
