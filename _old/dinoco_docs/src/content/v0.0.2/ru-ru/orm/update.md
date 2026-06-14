# update

Используется для обновления отфильтрованных записей.

---

## Что вы можете сделать

- `.cond(...)`: определяет, какие записи будут обновлены.
- `.values(item)`: указывает новые значения записи.
- `.connect(...)`: создает поддерживаемые связи для записи.
- `.disconnect(...)`: удаляет поддерживаемые связи для записи.
- `.returning::&lt;T&gt;()`: возвращает обновленные записи в типизированной проекции.
- `.execute(&client)`: выполняет обновление в базе данных.

## Возвращаемое значение

Без `.returning::&lt;T&gt;()`, возвращаемое значение:

```rust
DinocoResult<()>
```

С `.returning::&lt;T&gt;()`, возвращаемое значение становится:

```rust
DinocoResult<Vec<T>>
```

Примечание:

- `update().returning()` не поддерживает записи отношений с `.connect(...)` или `.disconnect(...)`.

## Пример обновления полей

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .values(User {
        id: 10,
        email: "novo@acme.com".to_string(),
        name: "Novo Nome".to_string(),
    })
    .execute(&client)
    .await?;
```

## Пример с connect(...)

Используется для подключения поддерживаемых связей для записи, обычно Many to Many.

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .connect(|r| r.roles().slug.eq("admin"))
    .execute(&client)
    .await?;
```

## Пример с disconnect(...)

Используется для отключения связей.

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .disconnect(|r| r.roles().slug.eq("guest"))
    .execute(&client)
    .await?;
```

## Пример с воркером

```rust
use database::*;

let _worker = workers()
    .on::<User, _, _>("user.updated", |job| async move {
        println!("Пользователь обновлен: {}", job.data.name);
        job.success();
    })
    .run()
    .await?;

dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .values(User {
        id: 10,
        email: "novo@acme.com".to_string(),
        name: "Novo Nome".to_string(),
    })
    .enqueue("user.updated")
    .execute(&client)
    .await?;
```

Подробнее о воркерах см. в [**`queues`**](/v0.0.2/orm/queues).

## Доступные фильтры в connect и disconnect

- `eq`
- `neq`
- `gt`
- `gte`
- `lt`
- `lte`
- `in_values`
- `not_in_values`
- `is_null`
- `is_not_null`
- `includes`
- `starts_with`
- `ends_with`

## Следующие шаги

- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): пакетное обновление.
- [**`find_and_update::&lt;M&gt;()`**](/v0.0.1/orm/find-and-update): атомарное обновление с возвратом.
