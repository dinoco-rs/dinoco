# Quickstart

Este guia leva você de um crate Rust binário vazio a um banco PostgreSQL migrado, com um insert e uma busca tipados, em cerca de cinco minutos. Se preferir ler primeiro um schema pronto e mais realista, vá direto para o [exemplo completo](/pt-br/docs/orm/guide/cookbook).

## Pré-requisitos

- Um toolchain Rust stable atual (`rustup show` deve mostrar um canal `stable`).
- Acesso a um banco PostgreSQL, MySQL ou SQLite. Este guia usa PostgreSQL com conexão direta; a página de [clients e adapters](/pt-br/docs/orm/orm/clients-adapters) cobre os outros dois.

## 1. Instale o Dinoco

Instale o binário da CLI e adicione os crates de runtime dos quais seu código gerado vai depender:

```bash
cargo install dinoco --version 2.0.1
cargo add dinoco@2.0.1 dinoco_engine@2.0.1 anyhow
cargo add tokio --features macros,rt-multi-thread
```

Cada crate tem uma função:

- `dinoco` — a API de consultas (`find_many`, `insert_into` etc.) e a derive `Entity` usada pelos seus models gerados.
- `dinoco_engine` — os adaptadores e o pool de conexões usados pela função `connect()` gerada.
- `anyhow` / `tokio` — usados pelo código gerado e pelo exemplo abaixo para tratamento de erros e runtime assíncrono.

> [!TIP]
> Instale também a [extensão do VS Code](/pt-br/docs/orm/tooling/vscode) antes de continuar — você vai ganhar diagnostics e completion inline ao editar `dinoco/schema.dinoco` no próximo passo.

## 2. Inicialize o projeto

Na raiz do seu projeto Cargo, rode o inicializador interativo:

```bash
dinoco init
```

Escolha `postgresql`, depois `direct`. O Dinoco escreve:

```text
dinoco/
  migrations/
  schema.dinoco
```

O `schema.dinoco` é criado com um bloco `config` inicial; nada mais é gerado até você rodar uma migration. Credenciais de banco nunca são escritas no próprio arquivo de schema — o Dinoco as lê do ambiente em tempo de execução:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/my_app"
```

> [!WARNING]
> `database_url = env("DATABASE_URL")` no schema é uma referência ao NOME de uma variável de ambiente, não um lugar para colar uma connection string real. Commitar um schema com uma URL literal não é só uma má prática aqui — o compilador rejeita isso.

## 3. Defina o schema

Substitua o `dinoco/schema.dinoco` gerado por este:

```dinoco
config {
    database     = "postgresql"
    connection   = "direct"
    database_url = env("DATABASE_URL")
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

Quatro campos (`id`, `email`, `name`, `role`) mais um timestamp — mas repare que só `email` e `name` não têm `@default(...)`. Isso importa para o próximo passo: o construtor `User::new` gerado só pede argumentos para os campos sem default, já que o Dinoco (e o banco) já sabem como preencher o resto.

## 4. Gere e execute a migration

```bash
dinoco migrate generate
```

Esse único comando faz cinco coisas, em ordem:

1. Compila e valida o schema (tipos desconhecidos, primary keys ausentes e relações inválidas são todos pegos aqui, antes de tocar no banco).
2. Conecta em `DATABASE_URL` e introspecta a estrutura atual do banco.
3. Planeja o SQL necessário para ir dessa estrutura até a descrita pelo seu schema.
4. Verifica o plano em tabelas isoladas `dinoco_migration_test_*` no mesmo banco — sem precisar de um banco shadow separado.
5. Escreve a migration, aplica ela e regenera os models Rust.

A migration em si vai para o disco como SQL puro, que você pode ler e rodar de novo depois:

```text
dinoco/migrations/<timestamp>_<nome>/
  up.sql
  down.sql
```

Os models gerados ficam em `dinoco/models/`, e `dinoco/mod.rs` expõe uma função `connect()` assíncrona já conectada ao adaptador descrito no seu bloco `config`. Para aplicar migrations que já existem em disco (por exemplo, no CI ou em outra máquina) sem gerar uma nova:

```bash
dinoco migrate run
```

> [!NOTE]
> **Estrutura do projeto.** Tudo que o Dinoco gera vive dentro de `dinoco/`, ao lado do seu schema — trate como parte da sua aplicação e versione, do mesmo jeito que você versionaria bindings gerados de Protobuf ou GraphQL. A página de [organização do schema](/pt-br/docs/orm/guide/schema-organization) cobre schemas multi-arquivo e layouts de projeto maiores.

## 5. Use o client gerado

```rust
mod dinoco;

use ::dinoco::{find_first, insert_into};
use dinoco::{connect, User};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new("ana@example.com".to_string(), "Ana".to_string());

    insert_into::<User>().values(&user).execute(&client).await?;

    let saved = find_first::<User>()
        .where_(|x| x.email.eq("ana@example.com"))
        .execute(&client)
        .await?;

    println!("{saved:#?}");
    Ok(())
}
```

Alguns detalhes que valem a pena internalizar cedo, porque aparecem em toda query que você vai escrever:

- `.values(&user)` recebe a entidade **por referência** — o insert não consome `user`, então você pode reusá-la depois (para logar, fazer um segundo insert, o que precisar).
- `find_first` retorna `anyhow::Result<Option<User>>`. Não encontrar uma linha é um `None` normal e esperado — não é um erro. Use `find_first` quando zero-ou-um for um resultado válido, e trate um `Err` como uma falha de verdade (uma conexão quebrada, uma query malformada).

## Próximos passos

- [Exemplo completo](/pt-br/docs/orm/guide/cookbook) — um schema maior e copiável, com relações e um write many-to-many.
- [Models e fields](/pt-br/docs/orm/guide/models) — a lista completa de tipos escalares e atributos de campo.
- [Visão geral de queries](/pt-br/docs/orm/orm/find) — como escolher entre `find_first`, `find_many` e os outros builders de leitura.
