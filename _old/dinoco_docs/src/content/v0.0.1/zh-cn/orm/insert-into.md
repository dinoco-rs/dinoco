# insert_into

用于插入一条记录。

---

## 您可以做什么

- 使用 `.values(item)` 传递值
- 通过 `.with_relation(related)` 插入关联
- 通过 `.with_connection(connected)` 连接现有关系
- 使用 `.execute(&client)` 执行

## 方法说明

- `.values(item)`: 定义要插入的记录。
- `.with_relation(related)`: 与父记录一起插入新的关联记录。
- `.with_connection(connected)`: 将插入的记录连接到现有项，通常在支持连接的关系流中。
- `.returning::&lt;T&gt;()`: 将返回值更改为插入项的类型化投影。
- `.execute(&client)`: 执行数据库写入操作。

## 返回值

如果没有 `.returning::&lt;T&gt;()`，返回值为：

```rust
DinocoResult<()>
```

使用 `.returning::&lt;T&gt;()`，返回值变为：

```rust
DinocoResult<T>
```

## 简单示例

```rust
dinoco::insert_into::<User>()
    .values(User {
        id: "usr_1".to_string(),
        email: "ana@acme.com".to_string(),
        name: "Ana".to_string(),
    })
    .execute(&client)
    .await?;
```

## 关联示例

当生成的模型支持同时插入父级和关联项时，使用 `.with_relation(...)`。

```rust
dinoco::insert_into::<User>()
    .values(user)
    .with_relation(profile)
    .execute(&client)
    .await?;
```

## 连接示例

当您想插入一个项并连接一个现有关系时，使用 `.with_connection(...)`。

```rust
dinoco::insert_into::<User>()
    .values(new_user)
    .with_connection(existing_team)
    .execute(&client)
    .await?;
```

## 带类型返回值的示例

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: i64,
    name: String,
}

let created = dinoco::insert_into::<User>()
    .values(User { id: 1, name: "Matheus".to_string() })
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

## 下一步

- [**`insert_many::&lt;M&gt;()`**](/v0.0.1/orm/insert-many): 批量插入。
- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): 记录更新。
