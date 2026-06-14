# Premiers pas

Pour commencer à utiliser Dinoco, vous devez installer notre CLI afin de pouvoir manipuler les migrations et les autres systèmes !

```bash
cargo install dinoco-cli
```

Pour créer l'environnement Dinoco, nous exécutons la commande suivante :

```bash
dinoco init
```

Après avoir choisi la base de données et toutes les configurations nécessaires, le dossier dinoco sera créé à la racine de votre projet.

Ce dossier contiendra :

- **Migrations :** L'historique des modifications de votre base de données.
- **Schéma :** La définition centrale de votre structure de données.
- **Modèles :** Les représentations typées pour une utilisation dans votre code Rust.

## Comment ça marche ?

### 1. Définissez votre schéma et votre connexion

Le schéma Dinoco définit le contenu de vos modèles et les configurations de la base de données.

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

### 2. Créez la migration

En générant la migration avec --apply, elle sera appliquée à la base de données et les modèles seront générés automatiquement !

```bash
dinoco migrate generate --apply
```

### 3. Requête avec DinocoClient

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

    // Insérez un utilisateur avec un post lié.
    insert_into::<User>()
        .values(UserWithPostInsert {
            id: 0,
            email: "bia@dinoco.rs".to_string(),
            name: Some("Bia".to_string()),
            posts: vec![Post { id: 0, title: "Meu primeiro post".to_string(), published: true, authorId: None }],
        })
        .execute(&client)
        .await?;

    // Recherchez tous les utilisateurs avec leurs publications.
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

    // résultat :
    // [
    // 	UserWithRelation {
    // 		email: "bia@dinoco.rs",
    // 		posts: [
    // 			Post {
    // 				title: "Mon premier post",
    // 				published: true,
    // 			},
    // 		],
    // 	},
    // ]

    Ok(())
}
```

## Prochaines étapes

- [**Schéma Dinoco**](/v0.0.2/orm/introduction-dinoco) : Comprenez mieux la structure et la proposition de Dinoco.
- [**find_many**](/v0.0.2/orm/find-many) : consultez les filtres, les inclusions et le cache dans les requêtes de liste.
