# insert_into

레코드를 삽입하는 데 사용됩니다.

---

## 수행 가능한 작업

- `.values(item)`: 삽입할 레코드를 정의합니다.
- `.returning::&lt;T&gt;()`: 반환 값을 삽입된 항목의 타입이 지정된 프로젝션으로 변경합니다.
- `.execute(&client)`: 데이터베이스에 쓰기 작업을 실행합니다.

## Extend #[insertable] 예시

`#[insertable]`를 `#[derive(Extend)]`와 함께 사용하여 `.values(...)`가 새로운 관계 및 기존 연결을 포함하는 더 풍부한 페이로드를 허용하도록 할 수 있습니다. Dinoco는 부모를 먼저 삽입한 다음 모든 것을 재귀적으로 처리합니다.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
#[insertable]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<PostWithComments>,
}

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Post)]
#[insertable]
struct PostWithComments {
    id: i64,
    title: String,
    published: bool,
    authorId: i64,
    comments: Vec<Comment>,
}

dinoco::insert_into::<User>()
    .values(UserWithPosts {
        id: 1,
        name: "Ana".to_string(),
        posts: vec![
            PostWithComments {
                id: 10,
                title: "재귀 삽입".to_string(),
                published: true,
                authorId: 0,
                comments: vec![Comment { id: 100, text: "첫 번째".to_string(), flagged: false, postId: 0 }],
            },
        ],
    })
    .execute(&client)
    .await?;
```

## 중첩 관계 예시

`#[insertable]`와 함께 Extend 페이로드를 사용하여 부모를 생성하고 동일한 흐름에서 관련 항목도 생성합니다.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
#[insertable]
struct UserWithProfile {
    id: String,
    username: String,
    role: Role,
    profile: Option<Profile>,
}

dinoco::insert_into::<User>()
    .values(UserWithProfile {
        id: "user-1".to_string(),
        username: "bia".to_string(),
        role: Role::USER,
        profile: Some(Profile {
            id: "profile-1".to_string(),
            bio: Some("소프트웨어 엔지니어".to_string()),
            userId: String::new(),
        }),
    })
    .execute(&client)
    .await?;
```

## 중첩 연결 예시

관계가 이미 존재하는 레코드를 가리킬 때, 코드 생성기는 페이로드 내에서 사용될 ArticleConnection과 같은 열거형을 생성합니다.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}

dinoco::insert_into::<Article>()
    .values(ArticleWithLabels {
        id: "article-10".into(),
        title: "기존 연결".into(),
        labels: vec![ArticleConnection::Label("label-10".into())],
    })
    .execute(&client)
    .await?;
```

## ArticleConnection 예시

이는 다대다 관계에서 기존 레코드를 연결하는 가장 일반적인 형식입니다.

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithLabels {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}

let value = ArticleWithLabels {
    id: "article-20".into(),
    title: "기존 연결".into(),
    labels: vec![
        ArticleConnection::Label("label-10".into()),
        ArticleConnection::Label("label-11".into()),
    ],
};

dinoco::insert_into::<Article>()
    .values(value)
    .execute(&client)
    .await?;
```

## 반환

`.returning::&lt;T&gt;()`가 없으면 반환 값은 다음과 같습니다.

```rust
DinocoResult<()>
```

`.returning::&lt;T&gt;()`가 있으면 반환 값은 다음과 같습니다.

```rust
DinocoResult<T>
```

## 간단한 예시

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

## 타입이 지정된 반환 예시

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

## 워커 예시

```rust
use database::*;

let _worker = workers()
    .on::<User, _, _>("user.created", |job| async move {
        println!("사용자 생성됨: {}", job.data.name);
        job.success();
    })
    .run()
    .await?;

dinoco::insert_into::<User>()
    .values(User { id: 1, name: "Matheus".to_string() })
    .enqueue("user.created")
    .execute(&client)
    .await?;
```

워커에 대한 자세한 내용은 [**`queues`**](/v0.0.2/orm/queues)를 참조하세요.

## 다음 단계

- [**Derives**](/v0.0.2/core/derives): Rowable, Extend 및 #[insertable]을 언제 사용해야 하는지 이해합니다.
- [**`insert_many::&lt;M&gt;()`**](/v0.0.2/orm/insert-many): 배치 삽입.
- [**`update::&lt;M&gt;()`**](/v0.0.2/orm/update): 레코드 업데이트.
