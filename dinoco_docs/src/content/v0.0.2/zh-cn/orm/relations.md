# 关系

关系将 Dinoco schema 中的模型相互连接起来。

本页涵盖 `@relation(...)`、`onDelete`、`onUpdate` 以及最常见的关系模式。

---

## @relation(...)

`@relation(...)` 属性定义了两个模型如何连接。

支持的参数:

| 参数         | 用途                               |
| :----------- | :--------------------------------- |
| `name`       | 关系的显式名称                     |
| `fields`     | 用作键的本地字段                   |
| `references` | 目标模型中引用的字段               |
| `onDelete`   | 删除引用记录时的行为               |
| `onUpdate`   | 更新引用值时的行为                 |

完整示例:

```dinoco
model Post {
	id       Integer @id @default(autoincrement())
	title    String
	authorId Integer?
	author   User?   @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
```

## onDelete 和 onUpdate

Dinoco 暴露了引用操作，用于控制相关记录被更改或删除时发生的情况。

支持的值:

| 操作         | 含义                               |
| :----------- | :--------------------------------- |
| `Cascade`    | 将操作传播到依赖记录               |
| `SetNull`    | 将外键设置为 `null`                |
| `SetDefault` | 将字段设置为配置的默认值           |

示例:

```dinoco
model Comment {
	id      Integer @id @default(autoincrement())
	postId  Integer?
	post    Post?   @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
```

仅当本地字段为可选时才使用 `SetNull`。

## One to many

这是最常见的关系：一个父记录拥有多个子记录。

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

心智解读:

- 一个 `User` 可以有多个 `Post`。
- 每个 `Post` 属于一个唯一的 `User`。

## Many to many

在多对多关系中，双方都有列表。

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

心智解读:

- 一个 `User` 可以有多个 `Role`。
- 一个 `Role` 可以属于多个 `User`。

## Self relation

自引用关系发生在模型与自身关联时。

```dinoco
model User {
	id        Integer @id @default(autoincrement())
	name      String
	managerId Integer?
	manager   User?   @relation(name: "UserManager", fields: [managerId], references: [id], onDelete: SetNull, onUpdate: Cascade)
	reports   User[]  @relation(name: "UserManager")
}
```

心智解读:

- 一个用户可以有一个经理。
- 一个用户可以有多个下属。

## 实用技巧

- 当相同模型之间存在多个关系时，使用 `name`。
- 当子记录在没有父记录的情况下没有意义时，使用 `onDelete: Cascade`。
- 当关系可以在不删除子记录的情况下解除时，使用 `onDelete: SetNull`。
- 使用带有显式名称的自引用关系，以便于阅读和代码生成。

## 示例的参考 Schema

以下示例使用以下 schema:

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

## 从 User 模型开始查询所有可能关系的示例

当您想从 `User` 模型开始并在同一个读取查询中加载所有直接关系时，可以将 `select::&lt;T&gt;()` 与多个 `includes(...)` 结合使用。

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

当您想在一次面向关系的读取中构建丰富的用户视图时，此模式非常有用。

## 使用 User 和 Profile 进行带关系插入的示例

要使用新 API 插入具有 1:1 关系的记录，请将关系放入标记为 `#[insertable]` 的 `Extend` payload 中。

```rust
use dinoco::insert_into;

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
#[insertable]
struct UserWithProfile {
	id: String,
	username: String,
	role: Role,
	profile: Option<Profile>,
}

let created_user = insert_into::<User>()
	.values(UserWithProfile {
		id: "user-1".to_string(),
		username: "bia".to_string(),
		role: Role::USER,
		profile: Some(Profile {
			id: "profile-1".to_string(),
			bio: Some("Engenheira de software".to_string()),
			userId: String::new(),
		}),
	})
	.execute(&client)
	.await?;
```

在此流程中:

- `User` 首先被插入。
- 相关的 `Profile` 随后被创建。
- `userId` 键由 Dinoco 的关系流程绑定。

## 使用嵌套关系进行 insert_many 的示例

当您想插入多个父记录，并且为每个父记录插入多个新的相关记录时，请使用 `#[insertable]`。

```rust
use dinoco::insert_many;

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Post)]
#[insertable]
struct PostWithComments {
	id: String,
	title: String,
	content: String,
	authorId: String,
	comments: Vec<Comment>,
}

let posts = vec![
	PostWithComments {
		id: "post-1".to_string(),
		title: "Primeiro post".to_string(),
		content: "Conteudo do primeiro post".to_string(),
		authorId: "user-1".to_string(),
		comments: vec![
			Comment {
				id: "comment-1".to_string(),
				text: "Muito bom".to_string(),
				parentId: None,
				postId: String::new(),
				authorId: "user-2".to_string(),
			},
			Comment {
				id: "comment-2".to_string(),
				text: "Gostei do exemplo".to_string(),
				parentId: None,
				postId: String::new(),
				authorId: "user-3".to_string(),
			},
		],
	},
	PostWithComments {
		id: "post-2".to_string(),
		title: "Segundo post".to_string(),
		content: "Conteudo do segundo post".to_string(),
		authorId: "user-1".to_string(),
		comments: vec![
			Comment {
				id: "comment-3".to_string(),
				text: "Quero mais detalhes".to_string(),
				parentId: None,
				postId: String::new(),
				authorId: "user-2".to_string(),
			},
		],
	},
];

insert_many::<Post>()
	.values(posts)
	.execute(&client)
	.await?;
```

在此流程中:

- 每个 `Post` 都被插入。
- Dinoco 创建嵌套的 `Comment`，并在写入期间将每个组绑定到正确的 `Post`。

## 使用嵌套连接进行 insert_many 的示例

当您想插入多个父记录并将每个父记录连接到多个已存在的记录时，请使用代码生成器生成的 `PostConnection` 枚举。

```rust
use dinoco::insert_many;

#[derive(Debug, Clone, dinoco::Extend)]
#[extend(Post)]
#[insertable]
struct PostWithTags {
	id: String,
	title: String,
	content: String,
	authorId: String,
	tags: Vec<PostConnection>,
}

insert_many::<Post>()
	.values(vec![
		PostWithTags {
			id: "post-10".to_string(),
			title: "Rust e Dinoco".to_string(),
			content: "Post sobre produtividade".to_string(),
			authorId: "user-1".to_string(),
			tags: vec![
				PostConnection::Tag("tag-1".to_string()),
				PostConnection::Tag("tag-2".to_string()),
			],
		},
		PostWithTags {
			id: "post-11".to_string(),
			title: "Relações avançadas".to_string(),
			content: "Post sobre includes e relacoes".to_string(),
			authorId: "user-1".to_string(),
			tags: vec![
				PostConnection::Tag("tag-2".to_string()),
				PostConnection::Tag("tag-3".to_string()),
			],
		},
	])
	.execute(&client)
	.await?;
```

在此流程中:

- `Post` 被插入。
- 已存在的 `Tag` 被连接到相应的 `Post`。
- 每个 payload 使用代码生成器生成的枚举定义其连接。

## 下一步

- [**枚举**](/v0.0.2/orm/enums)：了解如何在 schema 中表示受控值。
- [**模型**](/v0.0.2/orm/models)：了解字段结构以及使用 Dinoco API 进行查询、插入、更新和删除的示例。
