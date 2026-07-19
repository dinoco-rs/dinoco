# insert_many

バッチ挿入に使用されます。

---

## できること

- `.values(Vec&lt;M&gt;)`: バッチで挿入されるレコードを定義します。
- `.returning::&lt;T&gt;()`: 戻り値を、挿入されたアイテムの型付きリストに変更します。
- `.execute(&client)`: バッチ操作を実行します。

## 戻り値

`.returning::&lt;T&gt;()`がない場合、戻り値は次のようになります。

```rust
DinocoResult<()>
```

`.returning::&lt;T&gt;()`がある場合、戻り値は次のようになります。

```rust
DinocoResult<Vec<T>>
```

## シンプルな例

```rust
dinoco::insert_many::<User>()
    .values(vec![
        User { id: "u1".into(), email: "a@acme.com".into(), name: "A".into() },
        User { id: "u2".into(), email: "b@acme.com".into(), name: "B".into() },
    ])
    .execute(&client)
    .await?;
```

## 戻り値を含む例

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

## ワーカーの例

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-created", |job| async move {
        println!("バッチで作成されたユーザー: {}", job.data.len());
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

ワーカーの詳細については、[**`queues`**](/v0.0.2/orm/queues)を参照してください。

## ネストされたリレーションの例

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

## ネストされたコネクションの例

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

## ArticleConnectionの例

関連するアイテムが既に存在する場合、ペイロード内でコード生成されたenumを使用します。

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

## Extend #[insertable] の例

`insert_many`は、structが`#[insertable]`でマークされている場合、`.values(...)`を介してよりリッチなペイロードを受け取ることもできます。これは、親とそのネストされたリレーションを単一の再帰的なフローで作成するのに役立ちます。

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

## 備考

- `Extend` structの`#[insertable]`は、`.values(...)`が新しいリレーションを再帰的に挿入することを可能にします。
- 既存のレコードとのコネクションには、コード生成された`ModelConnection` enumを使用します。
- 新しいリレーションは、`Vec&lt;ModelRelacionado&gt;`または`#[insertable]`でマークされた他の`Extend`ペイロードを使用できます。
- 既存のコネクションは、多対多のシナリオや、生成されたモデルによってサポートされるフローでうまく機能します。

## 次のステップ

- [**Derives**](/v0.0.2/core/derives): `Extend`と`#[insertable]`でペイロードを構築する方法を理解します。
- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into): 単一の挿入。
- [**`update::&lt;M&gt;()`**](/v0.0.2/orm/update): フィルターによる更新。
