# 시작하기

Dinoco를 사용하려면 마이그레이션 및 기타 시스템을 조작할 수 있도록 CLI를 설치해야 합니다!

```bash
cargo install dinoco-cli
```

Dinoco 환경을 생성하려면 다음 명령을 실행합니다:

```bash
dinoco init
```

데이터베이스와 필요한 모든 설정을 선택하면 프로젝트 루트에 dinoco 폴더가 생성됩니다.

이 폴더에는 다음이 포함됩니다:

- **Migrations:** 데이터베이스 변경 내역.
- **Schema:** 데이터 구조의 중앙 정의.
- **Models:** Rust 코드에서 사용할 타입화된 표현.

## 작동 방식은?

### 1. 스키마 및 연결 정의

Dinoco 스키마는 모델의 내용과 데이터베이스 구성을 정의합니다.

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

### 2. 마이그레이션 생성

--apply 옵션으로 마이그레이션을 생성하면 데이터베이스에 적용되고 모델이 자동으로 생성됩니다!

```bash
dinoco migrate generate --apply
```

### 3. DinocoClient로 쿼리

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

    // 관련 게시물과 함께 사용자 삽입.
    insert_into::<User>()
        .values(User { id: 0, email: "bia@dinoco.rs".to_string(), name: Some("Bia".to_string()) })
        .with_relation(Post { id: 0, title: "내 첫 번째 게시물".to_string(), published: true, authorId: None })
        .execute(&client)
        .await?;

    // 모든 사용자와 해당 게시물을 가져옵니다.
    let users = find_many::<User>().select::<UserWithRelation>().includes(|x| x.posts()).execute(&client).await?;

    println!("{users:#?}");

    // 결과:
    // [
    // 	UserWithRelation {
    // 		email: "bia@dinoco.rs",
    // 		posts: [
    // 			Post {
    // 				title: "내 첫 번째 게시물",
    // 				published: true,
    // 			},
    // 		],
    // 	},
    // ]

    Ok(())
}
```

## 다음 단계

- [**Dinoco schema**](/v0.0.1/orm/introduction-dinoco): Dinoco의 구조와 목적을 더 잘 이해하십시오.
- [**Dinoco client**](/v0.0.1/orm/first-steps): 클라이언트의 전체 흐름을 검토하십시오.
