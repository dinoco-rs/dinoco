# Primi passi

Per iniziare a usare Dinoco, devi installare la nostra CLI in modo da poter manipolare le migrazioni e altri sistemi!

```bash
cargo install dinoco-cli
```

Per creare l'ambiente Dinoco, eseguiamo il seguente comando:

```bash
dinoco init
```

Dopo aver scelto il database e tutte le configurazioni necessarie, verrà creata la cartella dinoco nella radice del tuo progetto.

Questa cartella conterrà:

- **Migrations:** La cronologia delle modifiche del tuo database.
- **Schema:** La definizione centrale della tua struttura dati.
- **Models:** Le rappresentazioni tipizzate per l'uso nel tuo codice Rust.

## Come funziona?

### 1. Definisci il tuo schema e la tua connessione

Lo Schema Dinoco definisce il contenuto dei tuoi modelli e le configurazioni del database.

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

### 2. Crea la migrazione

Generando la migrazione con --apply, verrà applicata al database e i modelli verranno generati automaticamente!

```bash
dinoco migrate generate --apply
```

### 3. Query con DinocoClient

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

    // Inserisci un utente con un post correlato.
    insert_into::<User>()
        .values(User { id: 0, email: "bia@dinoco.rs".to_string(), name: Some("Bia".to_string()) })
        .with_relation(Post { id: 0, title: "Il mio primo post".to_string(), published: true, authorId: None })
        .execute(&client)
        .await?;

    // Cerca tutti gli utenti con i loro post.
    let users = find_many::<User>().select::<UserWithRelation>().includes(|x| x.posts()).execute(&client).await?;

    println!("{users:#?}");

    // risultato:
    // [
    // 	UserWithRelation {
    // 		email: "bia@dinoco.rs",
    // 		posts: [
    // 			Post {
    // 				title: "Il mio primo post",
    // 				published: true,
    // 			},
    // 		],
    // 	},
    // ]

    Ok(())
}
```

## Prossimi passi

- [**Dinoco schema**](/v0.0.1/orm/introduction-dinoco): Comprendi meglio la struttura e la proposta di Dinoco.
- [**Dinoco client**](/v0.0.1/orm/first-steps): Rivedi questo flusso completo del client.
