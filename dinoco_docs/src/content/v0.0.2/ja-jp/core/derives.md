# ダーライブ

Dinocoで使用されるダーライブと主要な属性の概要。

---

## Dinocoにおけるダーライブとは

Dinocoのダーライブは、手動で実装をすべて記述することなく、Rustの構造体をORMの動作に結び付けるのに役立ちます。

日常で最も一般的なものは次のとおりです。

- `#[derive(Rowable)]`
- `#[derive(Extend)]`
- `#[insertable]`

## Rowable ダーライブ

構造体がDinocoによって直接読み書きされるモデルを表す場合に`Rowable`を使用します。

```rust
#[derive(Debug, Clone, dinoco::Rowable)]
struct User {
    id: String,
    email: String,
    name: String,
}
```

このダーライブは、アダプターから返された行をRustの構造体にシリアライズします。

## Extend ダーライブ

ベースモデルとは異なるプロジェクションが必要な場合、通常は`select`、`include`、`count`、またはリッチなペイロードのために`Extend`を使用します。

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: String,
    name: String,
}
```

この場合、構造体は`User`モデルに引き続き関連付けられますが、そのフローに必要なフィールドのみを公開できます。

## #[insertable] 属性

`.values(...)`が新しいリレーションや既存の接続をペイロード内で受け入れる必要がある場合に、`Extend`と一緒に`#[insertable]`を使用します。

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}
```

これにより、Dinocoは親を挿入し、その後ネストされたアイテムを自動的に処理します。

## Rowableの例

```rust
#[derive(Debug, Clone, dinoco::Rowable)]
struct Team {
    id: String,
    name: String,
}

dinoco::insert_into::<Team>()
    .values(Team {
        id: "team-1".into(),
        name: "Platform".into(),
    })
    .execute(&client)
    .await?;
```

## Extendの例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPostsCount {
    id: String,
    name: String,
    posts_count: usize,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPostsCount>()
    .count(|user| user.posts())
    .execute(&client)
    .await?;
```

## #[insertable]の例

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
            labels: vec![ArticleConnection::Label("label-10".into())],
        },
    ])
    .execute(&client)
    .await?;
```

## それぞれの使用時期

- 生成された主要なモデル、または直接行読み取りと互換性のある構造体には`Rowable`を使用します。
- プロジェクション、カウント、インクルード、および特殊なペイロードには`Extend`を使用します。
- `Extend`がネストされた書き込みペイロードとしても機能する場合に`#[insertable]`を使用します。

## 次のステップ

- [**Traits**](/v0.0.2/core/traits): モデルによって実装されたトレイトを参照してください。
- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into): リッチなペイロードによる単一の挿入。
- [**`insert_many::&lt;M&gt;()`**](/v0.0.2/orm/insert-many): ネストされたリレーションと接続によるバッチ挿入。
