# Первые шаги

Чтобы начать использовать Dinoco, вам необходимо установить наш CLI, чтобы вы могли управлять миграциями и другими системами!

```bash
cargo install dinoco-cli
```

Чтобы создать среду Dinoco, выполните следующую команду:

```bash
dinoco init
```

После выбора базы данных и всех необходимых настроек в корне вашего проекта будет создана папка `dinoco`.

Эта папка будет содержать:

- **Миграции:** История изменений вашей базы данных.
- **Схема:** Центральное определение вашей структуры данных.
- **Модели:** Типизированные представления для использования в вашем коде Rust.

## Как это работает?

### 1. Определите свою схему и подключение

Схема Dinoco определяет содержимое ваших моделей и конфигурации базы данных.

```dinoco
config {
	database = "postgresql"
	database_url = env("DATABASE_URL")
}

model User {
	id    Integer     @id @default(autoincrement())
	email String  @unique
	name  String?

	posts Post[]
}

model Post {
	id        Integer     @id @default(autoincrement())
	title     String
	published Boolean @default(false)

	author    User?   @relation(fields: [authorId], references: [id])
	authorId  Integer?
}
```

### 2. Создайте миграцию

При генерации миграции с флагом `--apply` она будет применена к базе данных, и модели будут сгенерированы автоматически!

```bash
dinoco migrate generate --apply
```

### 3. Запрос с DinocoClient

```rust
use dinoco::{DinocoClientConfig, DinocoQueryLogger, DinocoQueryLoggerOptions, Extend, find_many, insert_into};

#[path = "../dinoco/mod.rs"]
mod database;

use database::models::*;

#[derive(Debug, Clone, Extend)]
#[extend(User)]
struct UserWithRelation {
    id: i64,
    email: String,
    posts: Vec<PostSimple>,
}

#[derive(Debug, Clone, Extend)]
#[extend(Post)]
struct PostSimple {
    title: String,
    published: bool,
}

#[tokio::main]
async fn main() -> dinoco::DinocoResult<()> {
    let _ = dotenvy::dotenv();

    let config = DinocoClientConfig::default()
        .with_snowflake_node_id(7)
        .with_query_logger(DinocoQueryLogger::stdout(DinocoQueryLoggerOptions::verbose()));

    let client = database::create_connection(config).await?;

    // Вставьте пользователя со связанным постом.
    insert_into::<User>()
        .values(User { id: 0, email: "bia@dinoco.rs".to_string(), name: Some("Bia".to_string()) })
        .with_relation(Post { id: 0, title: "Мой первый пост".to_string(), published: true, authorId: None })
        .execute(&client)
        .await?;

    // Найдите всех пользователей с их постами.
    let users = find_many::<User>().select::<UserWithRelation>().includes(|x| x.posts()).execute(&client).await?;

    println!("{users:#?}");

    // результат:
    // [
    // 	UserWithRelation {
    // 		email: "bia@dinoco.rs",
    // 		posts: [
    // 			Post {
    // 				title: "Мой первый пост",
    // 				published: true,
    // 			},
    // 		],
    // 	},
    // ]

    Ok(())
}
```

## Следующие шаги

- [**Схема Dinoco**](/v0.0.1/orm/introduction-dinoco): Лучше поймите структуру и назначение Dinoco.
- [**Клиент Dinoco**](/v0.0.1/orm/first-steps): Просмотрите этот полный рабочий процесс клиента.
