# update_many

用于一次性更新多个记录。

---

## 您可以做什么

- `.cond(...)`: 限制哪些记录可以被更新。
- `.values(Vec&lt;M&gt;)`: 定义用于批量更新的项。
- `.returning::&lt;T&gt;()`: 将更新后的记录作为类型化列表返回。
- `.execute(&client)`: 执行批量更新。

## 返回值

如果没有 `.returning::&lt;T&gt;()`，返回值为：

```rust
DinocoResult<()>
```

如果有 `.returning::&lt;T&gt;()`，返回值为：

```rust
DinocoResult<Vec<T>>
```

## 基本示例

```rust
dinoco::update_many::<User>()
    .values(vec![
        User { id: 1, email: "a@acme.com".into(), name: "Ana".into() },
        User { id: 2, email: "b@acme.com".into(), name: "Bia".into() },
    ])
    .execute(&client)
    .await?;
```

## 带返回值的示例

```rust
let updated = dinoco::update_many::<User>()
    .values(vec![
        User { id: 2, name: "Ana Batch".to_string() },
        User { id: 3, name: "Caio Batch".to_string() },
    ])
    .returning::<User>()
    .execute(&client)
    .await?;
```

## 带过滤器的示例

```rust
dinoco::update_many::<User>()
    .cond(|x| x.active.eq(true))
    .values(vec![
        User { id: 10, email: "a@acme.com".into(), name: "Ana".into() },
        User { id: 11, email: "b@acme.com".into(), name: "Bia".into() },
    ])
    .execute(&client)
    .await?;
```

## 下一步

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): 带条件的传统更新。
- [**`find_and_update::&lt;M&gt;()`**](/v0.0.1/orm/find-and-update): 对单个记录进行原子更新。
