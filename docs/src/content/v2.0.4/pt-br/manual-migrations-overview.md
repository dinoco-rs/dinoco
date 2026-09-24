# Visão geral das migrations manuais

O Dinoco suporta dois motores de migration. Ambos partem do mesmo `schema.dinoco` e geram os mesmos models Rust — a diferença é apenas **quem escreve as mudanças no banco**.

| | `automatic` (padrão) | `manual` |
| --- | --- | --- |
| Quem escreve a mudança | O Dinoco compara o schema com o banco real | Você, em Rust |
| Artefato da migration | `up.sql` + `down.sql` em `dinoco/migrations/<nome>/` | Um arquivo `.rs` por migration em `dinoco/migrations/` |
| `dinoco migrate generate` | Planeja, pede confirmação, escreve e aplica a migration | Cria uma migration vazia e regenera os models |
| Rollback | Não suportado pela CLI | `dinoco migrate rollback` executa o seu `down` |
| Tabela de histórico | `dinoco_migrations` | `dinoco_manual_migrations` |

## Escolha o motor

```dinoco
config {
    database         = "sqlite"
    database_url     = env("DATABASE_URL")
    migration_engine = "manual"
}
```

`migration_engine` aceita `"automatic"` ou `"manual"` e assume `"automatic"` quando a chave não existe — projetos existentes continuam funcionando sem mudanças. É uma configuração do banco; com [workspaces](/pt-br/docs/orm/guide/configuration#workspaces), cada workspace pode escolher o seu motor.

Um projeto novo pode começar direto no modo manual:

```bash
dinoco init --migration-engine manual
```

> [!WARNING]
> Escolha um motor por banco. Os dois motores mantêm tabelas de histórico separadas e nenhum lê a do outro; trocar de motor num banco existente significa que o novo motor enxerga um histórico vazio. Faça o baseline você mesmo (uma primeira migration manual contendo só o que já existe, com `if_not_exists`) antes de depender dele.

## Quando escolher manual

Use `manual` quando o diff automático não é a migration que você quer publicar:

- **Migrations de dados.** Preencher uma coluna, dividir uma tabela ou reescrever valores entre duas mudanças de schema.
- **Controle preciso.** Extensões, índices parciais, triggers ou DDL específico do banco que o `schema.dinoco` não expressa — execute com `manager.execute("...")`.
- **Releases revisadas e reversíveis.** Um `down` escrito e testado por você, e `dinoco migrate rollback` para usá-lo.
- **Bancos compartilhados.** Bancos que outras ferramentas também alteram, onde um diff de schema ficaria propondo desfazer o trabalho delas.

Fique no `automatic` quando o schema é a fonte da verdade e você quer que o Dinoco planeje renomeações, remoções e mudanças de constraints.

## Layout do projeto

```text
dinoco/
├── schema.dinoco
├── mod.rs                              gerado: `pub mod models; pub mod migrations;`
├── models/                             gerado
└── migrations/
    ├── mod.rs                          registro, editado pela CLI
    ├── 20260919120000_create_users.rs
    └── 20260920093000_add_user_email.rs
```

`dinoco/migrations/mod.rs` é o registro. Ele lista todas as migrations em ordem, entre comentários marcadores mantidos pela CLI:

```rust
#![allow(unused)]

// dinoco:migrations:mods:start
#[path = "20260919120000_create_users.rs"]
mod m20260919120000_create_users;
// dinoco:migrations:mods:end

pub fn migrations() -> Vec<::dinoco::MigrationEntry> {
    vec![
        // dinoco:migrations:list:start
        ::dinoco::MigrationEntry::new("20260919120000_create_users", m20260919120000_create_users::CreateUsers),
        // dinoco:migrations:list:end
    ]
}
```

`dinoco migrate generate <nome>` adiciona uma linha em cada região marcada. Não remova os marcadores; tudo fora deles é seu para editar. A ordem da lista é a ordem de execução, e o prefixo de timestamp mantém os arquivos ordenados do mesmo jeito.

## O que é gerado para você

O `dinoco/mod.rs` gerado ganha `pub mod migrations;` (ou `#[path = "migrations/<workspace>/mod.rs"]` para um workspace), então o registro fica acessível na aplicação como `dinoco::migrations::migrations()`. Veja [Escrevendo migrations](/pt-br/docs/orm/guide/writing-migrations) em seguida, e o [fluxo da CLI](/pt-br/docs/orm/guide/migration-workflow) para os comandos.
