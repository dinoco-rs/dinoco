# First steps

To start using Dinoco, you must install our CLI so you can manage migrations and other systems!

```bash
cargo install dinoco-cli
```

To create the Dinoco environment, we run the following command:

```bash
dinoco init
```

After choosing the database and all necessary configurations, the `dinoco` folder will be created at the root of your project.

This folder will contain:

- **Migrations:** The history of changes to your database.
- **Schema:** The central definition of your data structure.
- **Models:** The typed representations for use in your Rust code.

## How does it work?

### 1. Define your schema and connection

The Dinoco Schema defines the content of your models and database configurations.

```dinoco
config {
	database = "postgresql"
	database_url = env("DATABASE_URL")
	redis = {
		url = env("REDIS_URL")
	}
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

### 2. Create the migration

When generating the migration with `--apply`, it will be applied to the database and the models will be generated automatically!

```bash
dinoco migrate generate --apply
```

### 3. Query with DinocoClient

```rust
use dinoco::{DinocoClientConfig, DinocoQueryLogger, DinocoQueryLoggerOptions, Extend, find_many, insert_into};

#[path = "../dinoco/mod.rs"]
mod database;

use database::*;
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

#[derive(Debug, Clone, Extend)]
#[extend(User)]
#[insertable]
struct UserWithPostInsert {
    id: i64,
    email: String,
    name: Option<String>,
    posts: Vec<Post>,
}

#[tokio::main]
async fn main() -> dinoco::DinocoResult<()> {
    let _ = dotenvy::dotenv();

    let config = DinocoClientConfig::default()
        .with_snowflake_node_id(7)
        .with_query_logger(DinocoQueryLogger::stdout(DinocoQueryLoggerOptions::verbose()));

    let client = database::create_connection(config).await?;

    // Insert a user with a related post.
    insert_into::<User>()
        .values(UserWithPostInsert {
            id: 0,
            email: "bia@dinoco.rs".to_string(),
            name: Some("Bia".to_string()),
            posts: vec![Post { id: 0, title: "Meu primeiro post".to_string(), published: true, authorId: None }],
        })
        .execute(&client)
        .await?;

    // Fetch all users with their posts.
    let users = find_many::<User>().select::<UserWithRelation>().includes(|x| x.posts()).execute(&client).await?;

    let cached_users = find_many::<User>()
        .select::<UserWithRelation>()
        .includes(|x| x.posts())
        .cache_with_expiration("users:with-posts", 30)
        .execute(&client)
        .await?;

    let cached_direct = client.cache().get::<Vec<UserWithRelation>>("users:with-posts").await?;

    println!("{users:#?}");
    println!("{cached_users:#?}");
    println!("{cached_direct:#?}");

    // result:
    // [
    // 	UserWithRelation {
    // 		email: "bia@dinoco.rs",
    // 		posts: [
    // 			Post {
    // 				title: "Meu primeiro post",
    // 				published: true,
    // 			},
    // 		],
    // 	},
    // ]

    Ok(())
}
```

## Next steps

- [**Dinoco schema**](/v0.0.2/orm/introduction-dinoco): Understand the structure and purpose of Dinoco better.
- [**find_many**](/v0.0.2/orm/find-many): see filters, includes, and cache in list queries.
