# Модели

Модели (`model`) определяют центральные сущности вашего приложения в схеме Dinoco. Каждая `model` обычно представляет таблицу в базе данных и служит основой для генерации кода, типизированных запросов и операций с API Dinoco.

---

## Что представляет собой модель

Модель (`model`) описывает:

- Имя сущности.
- Поля, хранящиеся в базе данных.
- Какие поля являются обязательными или необязательными.
- Какие поля являются уникальными или идентификаторами.
- Как эти данные будут использоваться генератором кода и API.

Пример:

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique
	name  String?
}
```

В этом примере:

- `User` — это модель.
- `id`, `email` и `name` — это скалярные поля.
- `id` — это основной идентификатор.
- `email` имеет ограничение уникальности.

## Полный пример

Простая схема с моделью обычно выглядит так:

```dinoco
config {
	database = "postgresql"
	database_url = env("DATABASE_URL")
}

model User {
	id        Integer  @id @default(autoincrement())
	email     String   @unique
	name      String?
	active    Boolean  @default(true)
	createdAt DateTime @default(now())
}

model Post {
	id        Integer  @id @default(autoincrement())
	title     String
	content   String?
	published Boolean  @default(false)
	createdAt DateTime @default(now())
}
```

## Структура поля

Каждое поле модели состоит из:

- Имени
- Типа
- Необязательного модификатора
- Необязательных атрибутов

Пример:

```dinoco
email String @unique
```

В этой строке:

- `email` — это имя поля.
- `String` — это тип.
- `@unique` — это атрибут.

## Типы полей

Поля могут представлять базовые значения схемы, такие как текст, числа, булевы значения и даты.

### Скалярные поля

Это поля, которые хранят прямые значения, такие как текст, числа, булевы значения и даты.

```dinoco
model Product {
	id          Integer  @id @default(autoincrement())
	name        String
	description String?
	price       Float
	active      Boolean  @default(true)
	createdAt   DateTime @default(now())
}
```

## Модификаторы типа

Dinoco поддерживает два основных модификатора:

| Модификатор | Значение          | Пример          |
| :---------- | :---------------- | :-------------- |
| `?`         | Необязательное поле | `name String?`  |
| `[]`        | Список            | `tags String[]` |

### Необязательное поле

```dinoco
model User {
	id   Integer @id @default(autoincrement())
	name String?
}
```

`name` может быть нулевым или отсутствовать, в зависимости от базы данных и сгенерированного слоя.

### Поле-список

```dinoco
model Article {
	id   Integer  @id @default(autoincrement())
	tags String[]
}
```

Этот формат представляет список значений, если база данных и рабочий процесс поддерживают такой тип структуры.

## Наиболее распространённые атрибуты

Атрибуты изменяют поведение полей и модели.

| Атрибут         | Использование                       |
| :-------------- | :---------------------------------- |
| `@id`           | Определяет основной идентификатор   |
| `@default(...)` | Определяет значение по умолчанию    |
| `@unique`       | Гарантирует уникальность            |

### `@id`

Определяет поле, которое однозначно идентифицирует запись.

```dinoco
id Integer @id @default(autoincrement())
```

Каждая модель должна иметь чёткий идентификатор, чтобы сгенерированный API мог безопасно работать.

### `@default(...)`

Определяет значение по умолчанию для поля.

```dinoco
active    Boolean  @default(true)
createdAt DateTime @default(now())
id        Integer  @default(autoincrement())
```

Общие функции и значения:

| Пример                      | Использование             |
| :-------------------------- | :------------------------ |
| `@default(false)`           | Булево значение по умолчанию |
| `@default(now())`           | Текущая дата              |
| `@default(autoincrement())` | Инкрементное целое число  |
| `@default(uuid())`          | Идентификатор UUID        |

### `@unique`

Гарантирует, что значение поля не повторяется.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique
}
```

Этот атрибут идеально подходит для таких полей, как email, username и внешние коды.

## Декораторы модели

Помимо атрибутов для отдельных полей, Dinoco также поддерживает декораторы, применяемые ко всему блоку модели.

| Декоратор             | Использование                               |
| :-------------------- | :------------------------------------------ |
| `@@ids([...])`        | Определяет составной первичный ключ         |
| `@@table_name("...")` | Сопоставляет реальное имя таблицы в базе данных |

### `@@ids([...])`

Используйте `@@ids`, когда идентификация записи зависит от нескольких полей.

```dinoco
model Membership {
	userId Integer
	teamId Integer
	role   String

	@@ids([userId, teamId])
}
```

Этот формат полезен в ассоциативных таблицах и сценариях, где естественная уникальность уже является составной.

### `@@table_name("...")`

Используйте `@@table_name()`, когда вы хотите сохранить более удобное имя модели в схеме, но сопоставить его с другим физическим именем в базе данных.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique

	@@table_name("users")
}
```

В этом случае:

- Модель продолжает называться `User` в схеме и в сгенерированном API.
- Физическая таблица в базе данных становится `users`.

## Пример модели пользователя

```dinoco
model User {
	id        Integer  @id @default(autoincrement())
	email     String   @unique
	name      String?
	active    Boolean  @default(true)
	createdAt DateTime @default(now())
}
```

После генерации кода эта модель может быть использована непосредственно с API Dinoco.

## Пример поиска пользователей с помощью API Dinoco

### Поиск одной записи

```rust
let user = dinoco::find_first::<User>()
    .cond(|x| x.id.eq(1_i64))
    .execute(&client)
    .await?;
```

### Поиск нескольких записей

```rust
let users = dinoco::find_many::<User>()
    .cond(|x| x.name.includes("Ana"))
    .order_by(|x| x.id.asc())
    .take(10)
    .execute(&client)
    .await?;
```

## Пример создания пользователя с помощью API Dinoco

```rust
dinoco::insert_into::<User>()
    .values(User {
        id: 0,
        email: "bia@dinoco.rs".to_string(),
        name: Some("Bia".to_string()),
        active: true,
        createdAt: dinoco::Utc::now(),
    })
    .execute(&client)
    .await?;
```

## Пример обновления пользователя с помощью API Dinoco

```rust
dinoco::update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .values(User {
        id: 1,
        email: "bia@dinoco.rs".to_string(),
        name: Some("Beatriz".to_string()),
        active: true,
        createdAt: dinoco::Utc::now(),
    })
    .execute(&client)
    .await?;
```

Если вы хотите атомарные обновления одного поля, то рабочий процесс `find_and_update` обычно ещё более прямолинеен:

```rust
let user = dinoco::find_and_update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .update(|x| x.name.set("Beatriz"))
    .execute(&client)
    .await?;
```

## Пример удаления пользователя с помощью API Dinoco

```rust
dinoco::delete::<User>()
    .cond(|x| x.id.eq(1_i64))
    .execute(&client)
    .await?;
```

Для пакетного удаления:

```rust
dinoco::delete_many::<User>()
    .cond(|x| x.active.eq(false))
    .execute(&client)
    .await?;
```

## Краткое резюме

| Концепция       | Пример                 | Цель                      |
| :-------------- | :--------------------- | :------------------------ |
| Model           | `model User { ... }`   | Представлять сущность     |
| Скалярное поле  | `email String`         | Хранить простое значение  |
| Необязательное поле | `name String?`         | Разрешать отсутствие значения |
| Поле-список     | `tags String[]`        | Хранить несколько значений |
| ID              | `id Integer @id`       | Однозначно идентифицировать |
| Default         | `@default(now())`      | Автоматически заполнять   |
| Unique          | `email String @unique` | Избегать дублирования     |

## Когда создавать новую модель

Обычно вы создаёте новую модель (`model`), когда сущности вашего приложения требуется:

- Быть сохранённой в базе данных.
- Иметь собственную идентичность.
- Запрашиваться изолированно.
- Иметь собственные правила чтения и записи.

Распространённые примеры:

- `User`
- `Post`
- `Comment`
- `Category`
- `Order`
- `Invoice`
