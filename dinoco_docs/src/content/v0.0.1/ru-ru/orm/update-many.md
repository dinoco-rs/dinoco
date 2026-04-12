# update_many

Используется для одновременного обновления нескольких записей.

---

## Что вы можете сделать

- `.cond(...)`: ограничивает, какие записи могут быть обновлены.
- `.values(Vec&lt;M&gt;)`: определяет элементы, используемые при пакетном обновлении.
- `.returning::&lt;T&gt;()`: возвращает обновленные записи в виде типизированного списка.
- `.execute(&client)`: выполняет пакетное обновление.

## Возвращаемое значение

Без `.returning::&lt;T&gt;()` возвращаемое значение:

```rust
DinocoResult<()>
```

С `.returning::&lt;T&gt;()` возвращаемое значение становится:

```rust
DinocoResult<Vec<T>>
```

## Базовый пример

```rust
dinoco::update_many::<User>()
    .values(vec![
        User { id: 1, email: "a@acme.com".into(), name: "Ana".into() },
        User { id: 2, email: "b@acme.com".into(), name: "Bia".into() },
    ])
    .execute(&client)
    .await?;
```

## Пример с возвращаемым значением

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

## Пример с фильтром

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

## Следующие шаги

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): традиционное обновление с условием.
- [**`find_and_update::&lt;M&gt;()`**](/v0.0.1/orm/find-and-update): атомарное обновление одной записи.
