# Dinoco

Dinoco é um ORM orientado a schema para Rust. Você descreve seu banco de dados uma única vez, em um arquivo `dinoco/schema.dinoco`, e o Dinoco cuida do resto: valida essa descrição, planeja e aplica migrations específicas para cada adaptador, gera entidades Rust simples e dá a essas entidades uma API de consultas totalmente tipada e `async`.

A ideia central é simples: o SQL continua visível e inspecionável (toda migration gerada é SQL de verdade, que você pode ler e revisar), enquanto nomes de tabelas, nomes de colunas, tipos de valores e caminhos de relação passam a viver em código Rust que o compilador verifica para você. Um typo no nome de um campo, uma relação apontando para o model errado ou um valor do tipo errado — tudo isso falha na compilação, em vez de falhar em produção.

> [!NOTE]
> Dinoco é propositalmente restrito em escopo. É um ORM e uma ferramenta de migrations para PostgreSQL, MySQL e SQLite — não é um framework web, não é uma linguagem de query própria, e não é uma "abstração genérica de banco" que esconde o SQL atrás de um tipo de linha opaco. Se você já conhece SQL, boa parte do que vem a seguir vai parecer familiar.

## Por que Dinoco

A maioria das bibliotecas de banco de dados em Rust pede que você escolha entre dois extremos: escrever SQL à mão e perder segurança de tipos, ou adotar uma abstração pesada que esconde o que está realmente sendo executado. O Dinoco tenta ficar no meio-termo:

- **Um schema, três bancos.** O mesmo schema `.dinoco` compila para PostgreSQL, MySQL ou SQLite. Trocar de adaptador é uma mudança de configuração, não uma reescrita.
- **Models gerados, não escritos à mão.** Structs Rust, accessors de relação e query builders são gerados a partir do schema, então nunca podem divergir dele. Edite o schema, gere de novo, e o compilador te diz exatamente o que quebrou.
- **Migrations de verdade, sem mágica.** `dinoco migrate generate` introspecta seu banco ao vivo, compara com o schema e escreve arquivos `up.sql`/`down.sql` simples para você revisar antes de rodar.
- **Sem N+1 escondido.** Includes de relação são sempre feitos em lote (um número limitado de queries extras por nível de aninhamento), nunca uma query por linha.

## O que você ganha de cara

- Models Rust gerados com `Entity`, campos tipados, metadados de relação e um construtor `new()` prático.
- Builders `async` para `find_first`, `find_many`, `count`, `insert_into`, `insert_many`, `update`, `update_many`, `find_and_update`, `delete` e `delete_many`.
- Projeções customizadas via `.select::<T>()`, para que uma query retorne exatamente os campos que você pedir.
- Includes de relação em lote (`.includes(...)`), tanto para relações "muitos" quanto "um".
- Adaptadores diretos para PostgreSQL, PgBouncer, MySQL e SQLite, além de read replicas em round-robin com override explícito de leitura na primária.
- Planejamento de migrations baseado em introspecção do banco ao vivo, com detecção de drift para que o Dinoco nunca sobrescreva silenciosamente uma mudança de schema que ele não conhece.
- Um formatter e um language server completo para arquivos `.dinoco` no VS Code: diagnostics, completion, hover, go to definition, rename e semantic highlighting.

## Como o fluxo de trabalho se encaixa

O ciclo do dia a dia tem quatro passos:

1. Edite `dinoco/schema.dinoco`.
2. Rode `dinoco migrate generate` para validar o schema, planejar a mudança no banco e revisá-la.
3. Deixe a CLI aplicar a migration quando você estiver satisfeito com o SQL gerado.
4. Importe os models gerados em `dinoco/models/` e faça queries a partir do código da sua aplicação.

A pasta `dinoco/` gerada faz parte da sua aplicação — versione-a como qualquer outro arquivo de código. Ela não é uma segunda fonte de verdade: o schema continua sendo a autoridade, e o código Rust gerado é apenas a ponte tipada que o runtime usa para conversar com ele.

## Um exemplo mínimo

```rust
mod dinoco;

use dinoco::models::User;
use dinoco::{connect, models};
use ::dinoco::{find_many, insert_into};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new("ana@example.com".to_string(), "Ana".to_string());
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>()
        .includes(|x| x.posts())
        .execute(&client)
        .await?;

    println!("{} users", users.len());
    Ok(())
}
```

## A superfície da v2.0.0

O Dinoco mantém seu workflow público deliberadamente compacto. A CLI expõe `init`, geração e execução de migrations, e geração de models — nada além disso. O runtime expõe os builders listados acima; não existe um tipo separado de "struct de criação" distinto da própria entidade, e campos de relação são preenchidos da mesma forma que campos escalares, através da entidade que você já tem.

> [!TIP]
> Se você vem de um ORM que gera tipos separados `NewX`/`XChangeset`/`UpdateX` para cada model, espere o Dinoco parecer mais leve: uma única struct gerada por model serve tanto para leitura quanto para escrita.

## Próximos passos

Continue com o [quickstart](/pt-br/docs/orm/guide/quickstart) para ir de um crate Rust vazio a um projeto funcional com um banco de dados real em poucos minutos, ou vá direto para o [exemplo completo](/pt-br/docs/orm/guide/cookbook) se preferir ler primeiro um schema pronto e um conjunto de queries finalizado.
