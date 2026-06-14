# find_first

Используется для поиска не более одной записи.

---

## Что вы можете сделать

Он предоставляет те же методы, что и `find_many`:

- `select`
- `cond`
- `take`
- `skip`
- `order_by`
- `includes`
- `count`
- `read_in_primary`
- `execute`

## Описание методов

- `.select::&lt;T&gt;()`: заменяет проекцию по умолчанию на настраиваемую проекцию.
- `.cond(...)`: добавляет фильтры в поиск.
- `.take(...)`: ограничивает максимальное количество рассматриваемых записей.
- `.skip(...)`: пропускает записи перед выбором первого результата.
- `.order_by(...)`: определяет, какая запись должна быть рассмотрена первой.
- `.includes(...)`: загружает связи вместе с основным элементом.
- `.count(...)`: вычисляет счетчики связей в проекции.
- `.read_in_primary()`: принудительно считывает данные из основной базы данных.
- `.execute(&client)`: выполняет запрос и возвращает не более одного элемента.

## Возвращаемое значение

Без `select::&lt;T&gt;()` возвращаемое значение:

```rust
DinocoResult<Option<M>>
```

С `select::&lt;T&gt;()` возвращаемое значение становится:

```rust
DinocoResult<Option<T>>
```

## Базовый пример

```rust
let user = dinoco::find_first::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

## Пример с select

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

## Пример со связью

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

## Пример с сортировкой

```rust
let latest_user = dinoco::find_first::<User>()
    .order_by(|x| x.id.desc())
    .execute(&client)
    .await?;
```

## Следующие шаги

- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): поиск нескольких записей.
- [**`count::&lt;M&gt;()`**](/v0.0.1/orm/count): подсчет записей.
