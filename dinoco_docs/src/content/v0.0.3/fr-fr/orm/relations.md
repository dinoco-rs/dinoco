# Relations

Les relations connectent les modèles entre eux dans le schéma Dinoco.

Cette page couvre `@relation(...)`, `onDelete`, `onUpdate` et les modèles de relations les plus courants.

---

## @relation(...)

L'attribut `@relation(...)` définit comment deux modèles se connectent.

Paramètres supportés :

| Paramètre    | Utilisation                                              |
| :----------- | :------------------------------------------------------- |
| `name`       | Nom explicite de la relation                             |
| `fields`     | Champs locaux utilisés comme clé                         |
| `references` | Champs référencés dans le modèle cible                   |
| `onDelete`   | Comportement lors de la suppression de l'enregistrement référencé |
| `onUpdate`   | Comportement lors de la mise à jour de la valeur référencée |

Exemple complet :

```dinoco
model Post {
	id       Integer @id @default(autoincrement())
	title    String
	authorId Integer?
	author   User?   @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
```

## onDelete et onUpdate

Dinoco expose des actions référentielles pour contrôler ce qui se passe lorsque l'enregistrement lié est modifié ou supprimé.

Valeurs supportées :

| Action         | Signification                                      |
| :----------- | :------------------------------------------------- |
| `Cascade`    | Propager l'opération aux enregistrements dépendants |
| `SetNull`    | Définir la clé étrangère comme `null`              |
| `SetDefault` | Définir la valeur par défaut configurée pour le champ |

Exemple :

```dinoco
model Comment {
	id      Integer @id @default(autoincrement())
	postId  Integer?
	post    Post?   @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
```

Utilisez `SetNull` uniquement lorsque le champ local est optionnel.

## Un à plusieurs

C'est la relation la plus courante : un enregistrement parent possède plusieurs enfants.

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

Lecture mentale :

- Un `User` peut avoir plusieurs `Post`.
- Chaque `Post` appartient à un seul `User`.

## Plusieurs à plusieurs

Dans les relations plusieurs-à-plusieurs, les deux côtés ont des listes.

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

Lecture mentale :

- Un `User` peut avoir plusieurs `Role`.
- Une `Role` peut appartenir à plusieurs `User`.

## Auto-relation

L'auto-relation se produit lorsqu'un modèle se relie à lui-même.

```dinoco
model User {
	id        Integer @id @default(autoincrement())
	name      String
	managerId Integer?
	manager   User?   @relation(name: "UserManager", fields: [managerId], references: [id], onDelete: SetNull, onUpdate: Cascade)
	reports   User[]  @relation(name: "UserManager")
}
```

Lecture mentale :

- Un utilisateur peut avoir un manager.
- Un utilisateur peut avoir plusieurs subordonnés.

## Conseils pratiques

- Utilisez `name` lorsqu'il y a plus d'une relation entre les mêmes modèles.
- Utilisez `onDelete: Cascade` lorsque l'enfant n'a pas de sens sans le parent.
- Utilisez `onDelete: SetNull` lorsque la relation peut être rompue sans supprimer l'enregistrement enfant.
- Utilisez des auto-relations avec des noms explicites pour faciliter la lecture et le codegen.

## Schéma de référence pour les exemples

Les exemples ci-dessous utilisent le schéma suivant :

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

## Exemple de recherche avec toutes les relations possibles à partir de User

Lorsque vous souhaitez partir du modèle `User` et charger toutes les relations directes dans une même requête de lecture, vous pouvez combiner `select::&lt;T&gt;()` avec plusieurs `includes(...)`.

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

Ce modèle est utile lorsque vous souhaitez créer une vue riche de l'utilisateur en une seule lecture orientée relations.

## Exemple d'insertion avec relation utilisant User et Profile

Pour insérer un enregistrement avec une relation 1:1 en utilisant la nouvelle API, placez la relation dans une charge utile `Extend` marquée avec `#[insertable]`.

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
			bio: Some("Ingénieure logicielle".to_string()),
			userId: String::new(),
		}),
	})
	.execute(&client)
	.await?;
```

Dans ce flux :

- Le `User` est inséré en premier.
- Le `Profile` lié est créé ensuite.
- La clé `userId` est liée par le flux de relation de Dinoco.

## Exemple d'insert_many avec des relations imbriquées

Utilisez `#[insertable]` lorsque vous souhaitez insérer plusieurs enregistrements parents et, pour chacun d'eux, également insérer plusieurs nouveaux enregistrements liés.

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
		title: "Premier article".to_string(),
		content: "Contenu du premier article".to_string(),
		authorId: "user-1".to_string(),
		comments: vec![
			Comment {
				id: "comment-1".to_string(),
				text: "Très bien".to_string(),
				parentId: None,
				postId: String::new(),
				authorId: "user-2".to_string(),
			},
			Comment {
				id: "comment-2".to_string(),
				text: "J'ai aimé l'exemple".to_string(),
				parentId: None,
				postId: String::new(),
				authorId: "user-3".to_string(),
			},
		],
	},
	PostWithComments {
		id: "post-2".to_string(),
		title: "Deuxième article".to_string(),
		content: "Contenu du deuxième article".to_string(),
		authorId: "user-1".to_string(),
		comments: vec![
			Comment {
				id: "comment-3".to_string(),
				text: "Je veux plus de détails".to_string(),
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

Dans ce flux :

- Chaque `Post` est inséré.
- Dinoco crée les `Comment` imbriqués et lie chaque groupe au `Post` correct pendant l'écriture.

## Exemple d'insert_many avec des connexions imbriquées

Utilisez l'énumération `PostConnection` générée par le codegen lorsque vous souhaitez insérer plusieurs enregistrements parents et connecter chacun d'eux à plusieurs enregistrements déjà existants.

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
			title: "Rust et Dinoco".to_string(),
			content: "Article sur la productivité".to_string(),
			authorId: "user-1".to_string(),
			tags: vec![
				PostConnection::Tag("tag-1".to_string()),
				PostConnection::Tag("tag-2".to_string()),
			],
		},
		PostWithTags {
			id: "post-11".to_string(),
			title: "Relations avancées".to_string(),
			content: "Article sur les includes et les relations".to_string(),
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

Dans ce flux :

- Les `Post` sont insérés.
- Les `Tag` déjà existants sont connectés aux articles correspondants.
- Chaque charge utile définit ses connexions en utilisant l'énumération générée par le codegen.

## Prochaines étapes

- [**Enums**](/v0.0.2/orm/enums) : découvrez comment représenter des valeurs contrôlées dans le schéma.
- [**Models**](/v0.0.2/orm/models) : découvrez la structure des champs et des exemples de recherche, d'insertion, de mise à jour et de suppression avec l'API Dinoco.
