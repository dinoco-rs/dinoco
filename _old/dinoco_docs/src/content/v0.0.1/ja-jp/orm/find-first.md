# find_first

最大1つのレコードを検索するために使用されます。

---

## できること

`find_many` と同じメソッドを公開しています。

- `select`
- `cond`
- `take`
- `skip`
- `order_by`
- `includes`
- `count`
- `read_in_primary`
- `execute`

## メソッドの説明

- `.select::&lt;T&gt;()`: デフォルトのプロジェクションをカスタムプロジェクションに置き換えます。
- `.cond(...)`: 検索にフィルターを追加します。
- `.take(...)`: 考慮されるレコードの最大数を制限します。
- `.skip(...)`: 最初の結果を選択する前にレコードをスキップします。
- `.order_by(...)`: どのレコードを最初に考慮すべきかを定義します。
- `.includes(...)`: メインアイテムと一緒にリレーションをロードします。
- `.count(...)`: プロジェクション内のリレーションのカウンターを計算します。
- `.read_in_primary()`: プライマリデータベースからの読み取りを強制します。
- `.execute(&client)`: クエリを実行し、最大1つのアイテムを返します。

## 戻り値

`select::&lt;T&gt;()` を使用しない場合、戻り値は次のようになります。

```rust
DinocoResult<Option<M>>
```

`select::&lt;T&gt;()` を使用する場合、戻り値は次のようになります。

```rust
DinocoResult<Option<T>>
```

## 基本的な例

```rust
let user = dinoco::find_first::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

## select を使用した例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: i64,
    name: String,
}

let user = dinoco::find_first::<User>()
    .select::<UserSummary>()
    .cond(|w| w.id.eq(1_i64))
    .execute(&client)
    .await?;
```

## リレーションを使用した例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let user = dinoco::find_first::<User>()
    .select::<UserWithPosts>()
    .cond(|x| x.id.eq(1_i64))
    .includes(|x| x.posts())
    .execute(&client)
    .await?;
```

## ソートを使用した例

```rust
let latest_user = dinoco::find_first::<User>()
    .order_by(|x| x.id.desc())
    .execute(&client)
    .await?;
```

## 次のステップ

- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): 複数のレコードを検索します。
- [**`count::&lt;M&gt;()`**](/v0.0.1/orm/count): レコードのカウント。
