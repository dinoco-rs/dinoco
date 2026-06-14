# find_and_update

単一のレコードを検索し、データベースにアトミックな更新を適用し、更新されたアイテムを返すために使用されます。

---

## できること

- `.cond(...)`: どのレコードを検索するかを定義します。
- `.update(...)`: モデルのフィールドにアトミックな操作を適用します。
- `.execute(&client)`: 更新を実行し、更新されたレコードを返します。

## 戻り値

`find_and_update` の戻り値は次のとおりです。

```rust
DinocoResult<M>
```

## 基本的な例

```rust
let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

## ワーカーの例

```rust
use database::*;

let _worker = workers()
    .on::<Task, _, _>("task.reviewed", |job| async move {
        println!("タスクが更新されました {:?}", job.data.status);
        job.success();
    })
    .run()
    .await?;

let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .enqueue("task.reviewed")
    .execute(&client)
    .await?;
```

ワーカーの詳細については、[**`queues`**](/v0.0.2/orm/queues) を参照してください。

## `ModelUpdate` で利用可能な操作

- `set(value)`
- `increment(value)`
- `decrement(value)`
- `multiply(value)`
- `division(value)`

## 注意事項

- 更新は単一の `UPDATE` で実行されます。
- 条件に一致する行がない場合、戻り値は `DinocoError::RecordNotFound` になります。
- 更新DSLはリレーションを公開しません。
- 現在、このフローは、更新されたレコードを検索して返すための単純な主キーをサポートしています。

## 次のステップ

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): 従来の更新。
- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): バッチ更新。
