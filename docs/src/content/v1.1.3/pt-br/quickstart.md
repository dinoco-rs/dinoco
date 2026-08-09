# Início rápido

Este guia começa em um crate binário vazio e termina com um banco migrado, um insert e um find tipado.

## Pré-requisitos

Use uma versão stable atual do Rust. Você também precisa de PostgreSQL, MySQL ou um diretório gravável para SQLite. O exemplo usa PostgreSQL Direct.

## 1. Instale o Dinoco

```bash
cargo install dinoco --version 1.1.3
cargo add dinoco@1.1.3 dinoco_engine@1.1.3 anyhow
cargo add tokio --features macros,rt-multi-thread
```

`dinoco` contém os derives e methods. O módulo de conexão gerado usa `dinoco_engine`. `tokio` e `anyhow` completam o fluxo assíncrono.

## 2. Inicialize o projeto

Na raiz do projeto Cargo, execute:

```bash
dinoco init
```

Escolha `postgresql` e depois `direct`. A CLI cria `dinoco/schema.dinoco` e `dinoco/migrations/`. Coloque a URL no ambiente:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/my_app"
```

URLs literais são rejeitadas pelo compiler do schema para que credenciais não acabem no repositório.

## 3. Defina o schema

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = []
}

enum Role {
    member
    admin
}

model User {
    id         String   @id @default(uuid())
    email      String   @unique
    name       String
    role       Role     @default(member)
    created_at DateTime @default(now())
}
```

O `User::new` gerado pede somente `email` e `name`. Os outros fields têm default ou geração automática.

## 4. Gere e execute a migration

```bash
dinoco migrate generate
```

A CLI compila e valida o schema, inspeciona o banco atual, verifica o resultado pretendido em tabelas isoladas `dinoco_migration_test_*` e cria:

```text
dinoco/migrations/<timestamp>_<nome>/
  up.sql
  down.sql
```

Ela aplica o `up.sql` confirmado e gera `dinoco/models/` e `dinoco/mod.rs`. Em outro ambiente, execute migrations pendentes com:

```bash
dinoco migrate run
```

## 5. Use o client gerado

```rust
mod dinoco;

use ::dinoco::{find_first, insert_into};
use dinoco::{connect, User};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;
    let user = User::new(
        "ana@example.com".to_string(),
        "Ana".to_string(),
    );

    insert_into::<User>()
        .values(&user)
        .execute(&client)
        .await?;

    let saved = find_first::<User>()
        .where_(|x| x.email.eq("ana@example.com"))
        .execute(&client)
        .await?;

    println!("{saved:#?}");
    Ok(())
}
```

`.values(&user)` usa empréstimo e não move `user`. `find_first` retorna `Result<Option<User>>`; não encontrar uma row é `None`, não um erro.
