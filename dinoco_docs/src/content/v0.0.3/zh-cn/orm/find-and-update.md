# find_and_update

用于定位单个记录，在数据库中应用原子更新并返回更新后的项。

---

## 您可以做什么

- `.cond(...)`: 定义要定位的记录。
- `.update(...)`: 对模型的一个字段应用原子操作。
- `.execute(&client)`: 执行更新并返回更新后的记录。

## 返回值

`find_and_update` 的返回值是：

```rust
DinocoResult<M>
```

## 基本示例

```rust
let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

## Worker 示例

```rust
use database::*;

let _worker = workers()
    .on::<Task, _, _>("task.reviewed", |job| async move {
        println!("任务已更新为 {:?}", job.data.status);
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

更多关于 worker 的信息请参见 [**`queues`**](/v0.0.2/orm/queues)。

## `ModelUpdate` 中可用的操作

- `set(value)`
- `increment(value)`
- `decrement(value)`
- `multiply(value)`
- `division(value)`

## 注意事项

- 更新在一个单独的 `UPDATE` 中执行。
- 如果没有行匹配条件，将返回 `DinocoError::RecordNotFound`。
- 更新 DSL 不暴露关系。
- 目前，该流程支持使用简单主键来定位并返回更新后的记录。

## 下一步

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): 传统更新。
- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): 批量更新。
