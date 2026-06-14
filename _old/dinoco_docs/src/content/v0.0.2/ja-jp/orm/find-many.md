# find_many

レコードのリストを取得するために使用されます。

---

## できること

- `.select::&lt;T&gt;()`: モデルのデフォルトのプロジェクションをカスタムプロジェクションに置き換えます。
- `.cond(...)`: クエリにフィルター条件を追加します。
- `.take(...)`: 返されるレコードの数を制限します。
- `.skip(...)`: 結果を返す前に、指定された数のレコードをスキップします。
- `.order_by(...)`: クエリの順序を定義します。
- `.includes(...)`: メインレコードと一緒にリレーションをロードします。
- `.count(...)`: リレーションのカウンターを計算し、`posts_count`のようなフィールドを埋めます。
- `.cache(...)`: 指定されたキーを使用してRedisを最初にクエリします。キャッシュミスの場合、クエリを実行し、結果を保存して返します。キャッシュヒットの場合、クエリロガーは`CACHE HIT key=...`を記録します。
- `.cache_with_expiration(...)`: 標準キャッシュと同じ動作ですが、TTLを秒単位で設定して保存します。
- `.read_in_primary()`: レプリカを使用せず、プライマリデータベースからの読み取りを強制します。
- `.execute(&client)`: データベースでクエリを実行します。

## 戻り値

`select::&lt;T&gt;()`がない場合、戻り値は次のとおりです。

```rust
DinocoResult<Vec<M>>
```

`select::&lt;T&gt;()`がある場合、戻り値は次のようになります。

```rust
DinocoResult<Vec<T>>
```

## 基本的な例

```rust
let users = dinoco::find_many::<User>()
    .execute(&client)
    .await?;
```

## フィルター付きの例

```rust
let users = dinoco::find_many::<User>()
    .cond(|w| w.email.eq("ana@acme.com"))
    .execute(&client)
    .await?;
```

## ページネーションとソートの例

```rust
let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .skip(20)
    .take(10)
    .execute(&client)
    .await?;
```

## カスタムセレクトの例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserListItem {
    id: i64,
    name: String,
}

let users = dinoco::find_many::<User>()
    .select::<UserListItem>()
    .execute(&client)
    .await?;
```

## シンプルなインクルードの例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPosts>()
    .includes(|i| i.posts())
    .execute(&client)
    .await?;
```

## フィルター付きインクルードの例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPublishedPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPublishedPosts>()
    .includes(|i| i.posts().cond(|w| w.published.eq(true)))
    .execute(&client)
    .await?;
```

## ネストされたインクルードの例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Comment)]
struct CommentListItem {
    id: i64,
    text: String,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Post)]
struct PostWithComments {
    id: i64,
    title: String,
    comments: Vec<CommentListItem>,
    comments_count: usize,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<PostWithComments>,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPosts>()
    .includes(|i| {
        i.posts()
            .includes(|post| post.comments().take(3))
            .count(|post| post.comments())
    })
    .execute(&client)
    .await?;
```

## リレーションのカウントの例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPostsCount {
    id: i64,
    name: String,
    posts_count: usize,
}

let users = dinoco::find_many::<User>()
    .select::<UserWithPostsCount>()
    .count(|i| i.posts())
    .execute(&client)
    .await?;
```

## プライマリデータベースからの読み取りの例

```rust
let users = dinoco::find_many::<User>()
    .read_in_primary()
    .take(5)
    .execute(&client)
    .await?;
```

## ワーカーの例

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-read", |job| async move {
        println!("{}人のユーザーが読み込まれたバッチ", job.data.len());
        job.success();
    })
    .run()
    .await?;

let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .take(20)
    .enqueue("user.batch-read")
    .execute(&client)
    .await?;
```

ワーカーの詳細については、[**`queues`**](/v0.0.2/orm/queues)を参照してください。

## キャッシュの例

このメソッドは、スキーマの`config {}`に`redis`がある場合にのみ生成されます。

```rust
use database::*;

let users = dinoco::find_many::<User>()
    .order_by(|w| w.name.asc())
    .cache("users:list")
    .execute(&client)
    .await?;
```

## キャッシュと有効期限の例

```rust
use database::*;

let users = dinoco::find_many::<User>()
    .take(20)
    .cache_with_expiration("users:top-20", 60)
    .execute(&client)
    .await?;
```

## 次のステップ

- [**`find_first::&lt;M&gt;()`**](/v0.0.2/orm/find-first): 最大1つのレコードを検索するバージョン。
- [**`count::&lt;M&gt;()`**](/v0.0.2/orm/count): フィルター付きレコードのカウント。
