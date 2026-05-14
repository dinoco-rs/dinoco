# cache

`client.cache()` は、`find_first` および `find_many` に組み込まれたキャッシュヘルパーに依存することなく、`DinocoClient` に設定されたRedisへの直接アクセスを公開します。

## できること

- `.get::&lt;T&gt;(...)` でキーを読み取る
- `.set(...)` でキーを保存する
- `.set_with_ttl(...)` を使用して有効期限付きで保存する
- `.delete(...)` でキーを削除する

## いつ使うか

`client.cache()` は次の場合に使用します。

- 手動でキャッシュを構築する
- 書き込み操作後にキーを無効にする
- 複数のクエリ間でペイロードを共有する
- 高速読み取りのために準備された構造を保存する

## 仕組み

このメソッドは、`DinocoClientConfig::with_redis(...)` で設定された Redis を使用します。

クライアントに Redis が設定されていない場合、操作はエラーを返します。

## 利用可能なメソッド

- `.get::&lt;T&gt;(key)`: 値を `T` として取得し、逆シリアル化します
- `.set(key, &value)`: シリアル化して TTL なしで保存します
- `.set_with_ttl(key, &value, ttl_seconds)`: シリアル化して秒単位の有効期限付きで保存します
- `.delete(key)`: キーを削除します

## 基本的な例

```rust
use database::*;

let cache = client.cache();

cache.set("users:count", &42_i64).await?;

let count = cache.get::<i64>("users:count").await?;

println!("{count:?}");
```

## 型付きリストの例

```rust
use database::*;

let users = vec![
    User { id: 1, name: "マテウス".to_string() },
    User { id: 2, name: "アナ".to_string() },
];

client.cache().set("users:list", &users).await?;

let cached = client.cache().get::<Vec<User>>("users:list").await?;
```

## TTL の例

```rust
use database::*;

client.cache().set_with_ttl("users:top-10", &vec![1, 2, 3], 60).await?;
```

## 無効化の例

```rust
use database::*;

dinoco::update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .values(User { id: 1, name: "新しい名前".to_string() })
    .execute(&client)
    .await?;

client.cache().delete("users:1").await?;
client.cache().delete("users:list").await?;
```

## サポートされている型

値は JSON にシリアル化されるため、型は `serde` と互換性がある必要があります。

一般的な例：

- `Vec&lt;User&gt;`
- `Option&lt;User&gt;`
- `String`
- `bool`
- `i64`
- シリアル化可能な構造体

## 注意事項

- `client.cache()` は手動キャッシュであり、データベースクエリは実行しません。
- クエリに統合されたキャッシュには、`find_first().cache(...)` と `find_many().cache(...)` を使用します。
- `client.cache()` は何度でも呼び出すことができます。これは軽量なラッパーを作成するだけです。

## 次のステップ

- [**`find_first::&lt;M&gt;()`**](/v0.0.2/orm/find-first)
- [**`find_many::&lt;M&gt;()`**](/v0.0.2/orm/find-many)
- [**`queues`**](/v0.0.2/orm/queues)
