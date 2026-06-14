# Relações

As relações conectam models entre si no schema do Dinoco.

Esta página cobre `@relation(...)`, `onDelete`, `onUpdate` e os padrões mais comuns de relacionamento.

---

## @relation(...)

O atributo `@relation(...)` define como dois models se conectam.

Parâmetros suportados:

| Parâmetro    | Uso                                              |
| :----------- | :----------------------------------------------- |
| `name`       | Nome explícito da relação                        |
| `fields`     | Campos locais usados como chave                  |
| `references` | Campos referenciados no model alvo               |
| `onDelete`   | Comportamento ao deletar o registro referenciado |
| `onUpdate`   | Comportamento ao atualizar o valor referenciado  |

Exemplo completo:

```dinoco
model Post {
	id       Integer @id @default(autoincrement())
	title    String
	authorId Integer?
	author   User?   @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
```

## onDelete e onUpdate

O Dinoco expõe ações referenciais para controlar o que acontece quando o registro relacionado é alterado ou removido.

Valores suportados:

| Ação         | Significado                                      |
| :----------- | :----------------------------------------------- |
| `Cascade`    | Propaga a operação para os registros dependentes |
| `SetNull`    | Define a chave estrangeira como `null`           |
| `SetDefault` | Define o valor padrão configurado para o campo   |

Exemplo:

```dinoco
model Comment {
	id      Integer @id @default(autoincrement())
	postId  Integer?
	post    Post?   @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
```

Use `SetNull` apenas quando o campo local for opcional.

## Um para muitos

Esse é o relacionamento mais comum: um registro pai possui vários filhos.

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

Leitura mental:

- Um `User` pode ter vários `Post`.
- Cada `Post` pertence a um único `User`.

## Muitos para muitos

Em muitos-para-muitos, os dois lados têm listas.

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

Leitura mental:

- Um `User` pode ter várias `Role`.
- Uma `Role` pode pertencer a vários `User`.

## Auto-relação

Auto-relação acontece quando um model se relaciona com ele mesmo.

```dinoco
model User {
	id        Integer @id @default(autoincrement())
	name      String
	managerId Integer?
	manager   User?   @relation(name: "UserManager", fields: [managerId], references: [id], onDelete: SetNull, onUpdate: Cascade)
	reports   User[]  @relation(name: "UserManager")
}
```

Leitura mental:

- Um usuário pode ter um gerente.
- Um usuário pode ter vários subordinados.

## Dicas práticas

- Use `name` quando houver mais de uma relação entre os mesmos models.
- Use `onDelete: Cascade` quando o filho não faz sentido sem o pai.
- Use `onDelete: SetNull` quando o relacionamento puder ser desfeito sem remover o registro filho.
- Use auto-relações com nomes explícitos para facilitar a leitura e o codegen.

## Schema de referência para os exemplos

Os exemplos abaixo usam o seguinte schema:

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

## Exemplo de busca com todas as relações possíveis a partir de User

Quando você quiser partir do model `User` e carregar todas as relações diretas em uma mesma query de leitura, pode combinar `select::&lt;T&gt;()` com vários `includes(...)`.

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

Esse padrão é útil quando você quer montar uma visão rica do usuário em uma única leitura orientada a relações.

## Exemplo de insert com relação usando User e Profile

Para inserir um registro com uma relação 1:1, use `with_relation(...)`.

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
		bio: Some("Engenheira de software".to_string()),
		userId: String::new(),
	})
	.execute(&client)
	.await?;
```

Nesse fluxo:

- O `User` é inserido primeiro.
- O `Profile` relacionado é criado em seguida.
- A chave `userId` é vinculada pelo fluxo de relação do Dinoco.

## Exemplo de insert_many com with_relations(...)

Use `with_relations(...)` quando você quiser inserir vários registros pai e, para cada um deles, também inserir vários relacionados novos.

```rust
use dinoco::insert_many;

let posts = vec![
	Post {
		id: "post-1".to_string(),
		title: "Primeiro post".to_string(),
		content: "Conteudo do primeiro post".to_string(),
		authorId: "user-1".to_string(),
	},
	Post {
		id: "post-2".to_string(),
		title: "Segundo post".to_string(),
		content: "Conteudo do segundo post".to_string(),
		authorId: "user-1".to_string(),
	},
];

let comments_per_post = vec![
	vec![
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
	vec![
		Comment {
			id: "comment-3".to_string(),
			text: "Quero mais detalhes".to_string(),
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

Nesse fluxo:

- Cada `Post` é inserido.
- Cada grupo em `comments_per_post` corresponde ao `Post` da mesma posição.
- O Dinoco vincula os `Comment` ao post correto durante a escrita.

## Exemplo de insert_many com with_connections(...)

Use `with_connections(...)` quando você quiser inserir vários registros pai e conectar cada um deles a vários registros já existentes.

```rust
use dinoco::insert_many;

insert_many::<Post>()
	.values(vec![
		Post {
			id: "post-10".to_string(),
			title: "Rust e Dinoco".to_string(),
			content: "Post sobre produtividade".to_string(),
			authorId: "user-1".to_string(),
		},
		Post {
			id: "post-11".to_string(),
			title: "Relações avançadas".to_string(),
			content: "Post sobre includes e relacoes".to_string(),
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

Nesse fluxo:

- Os `Post` são inseridos.
- As `Tag` já existentes são conectadas aos posts correspondentes.
- Cada grupo do vetor externo representa as conexões do post da mesma posição.

## Próximos passos

- [**Enums**](/v0.0.1/orm/enums): veja como representar valores controlados no schema.
- [**Models**](/v0.0.1/orm/models): veja estrutura de campos e exemplos de busca, insert, update e delete com a API do Dinoco.
