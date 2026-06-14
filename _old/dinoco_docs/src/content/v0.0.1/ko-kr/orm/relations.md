# 관계

관계는 Dinoco 스키마에서 모델들을 서로 연결합니다.

이 페이지에서는 `@relation(...)`, `onDelete`, `onUpdate` 및 가장 일반적인 관계 패턴을 다룹니다.

---

## @relation(...)

`@relation(...)` 속성은 두 모델이 어떻게 연결되는지 정의합니다.

지원되는 매개변수:

| 매개변수    | 용도                                              |
| :----------- | :----------------------------------------------- |
| `name`       | 관계의 명시적 이름                        |
| `fields`     | 키로 사용되는 로컬 필드                  |
| `references` | 대상 모델에서 참조되는 필드               |
| `onDelete`   | 참조된 레코드를 삭제할 때의 동작 |
| `onUpdate`   | 참조된 값을 업데이트할 때의 동작  |

전체 예시:

```dinoco
model Post {
	id       Integer @id @default(autoincrement())
	title    String
	authorId Integer?
	author   User?   @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
```

## onDelete 및 onUpdate

Dinoco는 관련 레코드가 변경되거나 제거될 때 발생하는 상황을 제어하기 위한 참조 작업을 제공합니다.

지원되는 값:

| 작업         | 의미                                      |
| :----------- | :----------------------------------------------- |
| `Cascade`    | 종속 레코드에 작업을 전파합니다. |
| `SetNull`    | 외래 키를 `null`로 설정합니다.           |
| `SetDefault` | 필드에 구성된 기본값을 설정합니다.   |

예시:

```dinoco
model Comment {
	id      Integer @id @default(autoincrement())
	postId  Integer?
	post    Post?   @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
```

로컬 필드가 선택 사항인 경우에만 `SetNull`을 사용하십시오.

## 일대다 (One to many)

이것은 가장 일반적인 관계입니다: 하나의 부모 레코드가 여러 자식 레코드를 가집니다.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	name  String
	posts Post[]
}

model Post {
	id       Integer @id @default(autoincrement())
	title    String
	authorId Integer
	author   User    @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: Cascade)
}
```

개념적 이해:

- 하나의 `User`는 여러 `Post`를 가질 수 있습니다.
- 각 `Post`는 하나의 `User`에 속합니다.

## 다대다 (Many to many)

다대다 관계에서는 양쪽 모두 목록을 가집니다.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	name  String

	roles Role[]
}

model Role {
	id    Integer @id @default(autoincrement())
	name  String

	users User[]
}
```

개념적 이해:

- 하나의 `User`는 여러 `Role`을 가질 수 있습니다.
- 하나의 `Role`은 여러 `User`에 속할 수 있습니다.

## 자기 관계 (Self relation)

자기 관계는 모델이 자기 자신과 관계를 맺을 때 발생합니다.

```dinoco
model User {
	id        Integer @id @default(autoincrement())
	name      String
	managerId Integer?
	manager   User?   @relation(name: "UserManager", fields: [managerId], references: [id], onDelete: SetNull, onUpdate: Cascade)
	reports   User[]  @relation(name: "UserManager")
}
```

개념적 이해:

- 사용자는 관리자를 가질 수 있습니다.
- 사용자는 여러 부하 직원을 가질 수 있습니다.

## 실용적인 팁

- 동일한 모델 간에 두 개 이상의 관계가 있을 때 `name`을 사용하십시오.
- 자식이 부모 없이 의미가 없을 때 `onDelete: Cascade`를 사용하십시오.
- 자식 레코드를 제거하지 않고 관계를 해제할 수 있을 때 `onDelete: SetNull`을 사용하십시오.
- 읽기 및 코드 생성을 용이하게 하기 위해 명시적인 이름을 가진 자기 관계를 사용하십시오.

## 예시를 위한 참조 스키마

아래 예시들은 다음 스키마를 사용합니다:

```dinoco
config {
	database = "sqlite"
	database_url = env("DATABASE_URL")
}

enum Role {
	ADMIN
	USER
}

model User {
	id             String     @id @default(uuid())
	username       String     @unique
	role           Role       @default(USER)

	profile        Profile?

	followers      User[]     @relation(name: "UserFollows")
	following      User[]     @relation(name: "UserFollows")

	posts          Post[]     @relation(name: "PostAuthor")
	comments       Comment[]  @relation(name: "CommentAuthor")

	likedPosts     Post[]     @relation(name: "PostLikers")
	likedComments  Comment[]  @relation(name: "CommentLikers")
}

model Profile {
	id      String   @id @default(uuid())
	bio     String?
	userId  String   @unique
	user    User     @relation(fields: [userId], references: [id])
}

model Post {
	id        String     @id @default(uuid())
	title     String
	content   String

	authorId  String
	author    User       @relation(name: "PostAuthor", fields: [authorId], references: [id])

	likers    User[]     @relation(name: "PostLikers")

	comments  Comment[]

	tags      Tag[]
}

model Comment {
	id        String     @id @default(uuid())
	text      String

	parentId  String?
	parent    Comment?   @relation(name: "CommentReplies", fields: [parentId], references: [id])
	replies   Comment[]  @relation(name: "CommentReplies")

	postId    String
	post      Post       @relation(fields: [postId], references: [id])

	authorId  String
	author    User       @relation(name: "CommentAuthor", fields: [authorId], references: [id])

	likers    User[]     @relation(name: "CommentLikers")
}

model Tag {
	id     String  @id @default(uuid())
	name   String  @unique

	posts  Post[]
}
```

## User로부터 가능한 모든 관계를 포함하는 검색 예시

`User` 모델에서 시작하여 단일 읽기 쿼리에서 모든 직접 관계를 로드하려면 `select::&lt;T&gt;()`와 여러 `includes(...)`를 결합할 수 있습니다.

```rust
use dinoco::{Extend, find_many};

#[derive(Debug, Clone, Extend)]
#[extend(Profile)]
struct ProfileView {
	id: String,
	bio: Option<String>,
}

#[derive(Debug, Clone, Extend)]
#[extend(User)]
struct UserRelationItem {
	id: String,
	username: String,
}

#[derive(Debug, Clone, Extend)]
#[extend(Tag)]
struct TagView {
	id: String,
	name: String,
}

#[derive(Debug, Clone, Extend)]
#[extend(Comment)]
struct CommentView {
	id: String,
	text: String,
	replies: Vec<CommentReplyView>,
	likers: Vec<UserRelationItem>,
}

#[derive(Debug, Clone, Extend)]
#[extend(Comment)]
struct CommentReplyView {
	id: String,
	text: String,
}

#[derive(Debug, Clone, Extend)]
#[extend(Post)]
struct PostView {
	id: String,
	title: String,
	content: String,
	likers: Vec<UserRelationItem>,
	tags: Vec<TagView>,
	comments: Vec<CommentView>,
}

#[derive(Debug, Clone, Extend)]
#[extend(User)]
struct UserWithAllRelations {
	id: String,
	username: String,
	profile: Option<ProfileView>,
	followers: Vec<UserRelationItem>,
	following: Vec<UserRelationItem>,
	posts: Vec<PostView>,
	comments: Vec<CommentView>,
	likedPosts: Vec<PostView>,
	likedComments: Vec<CommentView>,
}

let users = find_many::<User>()
	.select::<UserWithAllRelations>()
	.includes(|user| user.profile())
	.includes(|user| user.followers())
	.includes(|user| user.following())
	.includes(|user| {
		user.posts()
			.includes(|post| post.likers())
			.includes(|post| post.tags())
			.includes(|post| {
				post.comments()
					.includes(|comment| comment.replies())
					.includes(|comment| comment.likers())
			})
	})
	.includes(|user| {
		user.comments()
			.includes(|comment| comment.replies())
			.includes(|comment| comment.likers())
	})
	.includes(|user| {
		user.likedPosts()
			.includes(|post| post.likers())
			.includes(|post| post.tags())
			.includes(|post| {
				post.comments()
					.includes(|comment| comment.replies())
					.includes(|comment| comment.likers())
			})
	})
	.includes(|user| {
		user.likedComments()
			.includes(|comment| comment.replies())
			.includes(|comment| comment.likers())
	})
	.execute(&client)
	.await?;
```

이 패턴은 관계 지향적인 단일 읽기에서 사용자에 대한 풍부한 뷰를 구성하려는 경우에 유용합니다.

## User 및 Profile을 사용한 관계 삽입 예시

1:1 관계를 가진 레코드를 삽입하려면 `with_relation(...)`을 사용하십시오.

```rust
use dinoco::insert_into;

let created_user = insert_into::<User>()
	.values(User {
		id: "user-1".to_string(),
		username: "bia".to_string(),
		role: Role::USER,
	})
	.with_relation(Profile {
		id: "profile-1".to_string(),
		bio: Some("소프트웨어 엔지니어".to_string()),
		userId: String::new(),
	})
	.execute(&client)
	.await?;
```

이 흐름에서:

- `User`가 먼저 삽입됩니다.
- 관련된 `Profile`이 이어서 생성됩니다.
- `userId` 키는 Dinoco의 관계 흐름에 의해 연결됩니다.

## with_relations(...)를 사용한 insert_many 예시

여러 부모 레코드를 삽입하고 각 부모 레코드에 대해 여러 개의 새로운 관련 레코드를 삽입하려면 `with_relations(...)`를 사용하십시오.

```rust
use dinoco::insert_many;

let posts = vec![
	Post {
		id: "post-1".to_string(),
		title: "첫 번째 게시물".to_string(),
		content: "첫 번째 게시물의 내용".to_string(),
		authorId: "user-1".to_string(),
	},
	Post {
		id: "post-2".to_string(),
		title: "두 번째 게시물".to_string(),
		content: "두 번째 게시물의 내용".to_string(),
		authorId: "user-1".to_string(),
	},
];

let comments_per_post = vec![
	vec![
		Comment {
			id: "comment-1".to_string(),
			text: "아주 좋음".to_string(),
			parentId: None,
			postId: String::new(),
			authorId: "user-2".to_string(),
		},
		Comment {
			id: "comment-2".to_string(),
			text: "예시가 마음에 듦".to_string(),
			parentId: None,
			postId: String::new(),
			authorId: "user-3".to_string(),
		},
	],
	vec![
		Comment {
			id: "comment-3".to_string(),
			text: "더 많은 세부 정보 원함".to_string(),
			parentId: None,
			postId: String::new(),
			authorId: "user-2".to_string(),
		},
	],
];

insert_many::<Post>()
	.values(posts)
	.with_relations(comments_per_post)
	.execute(&client)
	.await?;
```

이 흐름에서:

- 각 `Post`가 삽입됩니다.
- `comments_per_post`의 각 그룹은 동일한 위치의 `Post`에 해당합니다.
- Dinoco는 쓰기 중에 `Comment`를 올바른 게시물에 연결합니다.

## with_connections(...)를 사용한 insert_many 예시

여러 부모 레코드를 삽입하고 각 부모 레코드를 여러 기존 레코드에 연결하려면 `with_connections(...)`를 사용하십시오.

```rust
use dinoco::insert_many;

insert_many::<Post>()
	.values(vec![
		Post {
			id: "post-10".to_string(),
			title: "Rust와 Dinoco".to_string(),
			content: "생산성에 대한 게시물".to_string(),
			authorId: "user-1".to_string(),
		},
		Post {
			id: "post-11".to_string(),
			title: "고급 관계".to_string(),
			content: "포함 및 관계에 대한 게시물".to_string(),
			authorId: "user-1".to_string(),
		},
	])
	.with_connections(vec![
		vec![
			Tag { id: "tag-1".to_string(), name: "rust".to_string() },
			Tag { id: "tag-2".to_string(), name: "orm".to_string() },
		],
		vec![
			Tag { id: "tag-2".to_string(), name: "orm".to_string() },
			Tag { id: "tag-3".to_string(), name: "sqlite".to_string() },
		],
	])
	.execute(&client)
	.await?;
```

이 흐름에서:

- `Post`가 삽입됩니다.
- 기존 `Tag`는 해당 게시물에 연결됩니다.
- 외부 벡터의 각 그룹은 동일한 위치의 게시물 연결을 나타냅니다.

## 다음 단계

- [**열거형**](/v0.0.1/orm/enums): 스키마에서 제어되는 값을 표현하는 방법을 확인하십시오.
- [**모델**](/v0.0.1/orm/models): Dinoco API를 사용한 필드 구조 및 검색, 삽입, 업데이트, 삭제 예시를 확인하십시오.
