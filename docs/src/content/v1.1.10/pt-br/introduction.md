# Dinoco v1.1.10

O Dinoco é um ORM orientado por schema para Rust. Você descreve o banco uma vez em `dinoco/schema.dinoco`; a ferramenta valida essa descrição, produz migrations específicas para cada adapter, gera as entities em Rust e oferece uma API tipada para consultá-las.

A proposta é manter o comportamento do SQL visível, mas levar nomes de tabelas, colunas, tipos e caminhos de relações para o compilador do Rust verificar.

## O que o Dinoco entrega

- Um schema para PostgreSQL, MySQL e SQLite.
- Models gerados com `Entity`, fields tipados, relações e um construtor `new()` prático.
- Builders assíncronos de find, count, insert, update, update atômico e delete.
- Projeções customizadas com `EntityExtend` e `.select::<T>()`.
- Includes `many` em batch e includes `one` por left join, evitando N+1.
- PostgreSQL Direct, PgBouncer, MySQL, SQLite e réplicas de leitura.
- Planejamento de migrations por introspecção e tabelas de validação isoladas.
- Formatter e extensão inteligente para schemas `.dinoco` no VS Code.

Cada adapter converte sua row nativa diretamente para as entities. O SQL também é produzido pelo `SqlCompiler` do adapter, sem uma row genérica no meio do caminho.

## Como o fluxo se encaixa

O ciclo normal tem quatro passos:

1. Edite `dinoco/schema.dinoco`.
2. Rode `dinoco migrate generate` para validar e planejar a alteração.
3. Revise `up.sql` e `down.sql`; a CLI aplica a migration confirmada.
4. Importe os models em `dinoco/models/` e use a API tipada.

```rust
mod dinoco;

use ::dinoco::{find_many, insert_into};
use dinoco::{connect, User};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new(
        "ana@example.com".to_string(),
        "Ana".to_string(),
    );
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>()
        .includes(|x| x.posts())
        .execute(&client)
        .await?;

    println!("{} users", users.len());
    Ok(())
}
```

## A superfície da v1.1.10

A v1.1.10 mantém um fluxo público pequeno e previsível. A CLI inicializa o projeto, gera e executa migrations e regenera models. O runtime expõe builders como `find_many`, `insert_into`, `update_many` e `delete`.

APIs experimentais antigas, como `#[insertable]`, structs separadas de create, `with_relation`, reset de banco, restore de schema, filas e cache, não fazem parte desta versão. O próprio model é o payload de insert, inclusive com suas relações.
