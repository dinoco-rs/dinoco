# count

Используется для подсчета записей.

---

## Что вы можете сделать

- Фильтровать с помощью `.cond(...)`
- Выполнять с помощью `.execute(&client)`

## Описание методов

- `.cond(...)`: ограничивает, какие записи включаются в подсчет.
- `.execute(&client)`: выполняет подсчет в базе данных.

## Возвращаемое значение

Возвращаемое значение `count` это:

```rust
DinocoResult<usize>
```

## Базовый пример

```rust
let total = dinoco::count::<User>()
    .execute(&client)
    .await?;
```

## Пример с булевым фильтром

```rust
let total = dinoco::count::<User>()
    .cond(|w| w.active.eq(true))
    .execute(&client)
    .await?;
```

## Пример с текстовым фильтром

```rust
let total = dinoco::count::<User>()
    .cond(|w| w.name.includes("Ana"))
    .execute(&client)
    .await?;
```

## Следующие шаги

- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): извлекает записи в список.
- [**`find_first::&lt;M&gt;()`**](/v0.0.1/orm/find-first): извлекает одну запись.
