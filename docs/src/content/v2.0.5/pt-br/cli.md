# Referência da CLI

O binário `dinoco` é o ponto de entrada para todo workflow do projeto — rodando interativamente localmente, no CI, ou num pipeline de deploy. Rode-o a partir da raiz do projeto Cargo, já que caminhos como `dinoco/schema.dinoco` são sempre resolvidos relativos ao diretório atual.

## dinoco init

```bash
dinoco init
```

Guia você na escolha entre PostgreSQL, MySQL ou SQLite; PostgreSQL adiciona uma segunda pergunta para Direct ou PgBouncer. Cria `dinoco/migrations/` e um `dinoco/schema.dinoco` inicial já formatado, com `database_url = env("DATABASE_URL")` já conectado.

> [!NOTE]
> Se um schema já existir nesse caminho, `init` o deixa intocado e imprime um aviso em vez de sobrescrever seu trabalho — sempre é seguro rodar de novo.

Para setup automatizado — CI, um template de projeto, o entrypoint de um container — os mesmos prompts podem ser respondidos de forma não interativa:

```bash
DINOCO_CLI_INIT_DATABASE=postgresql \
DINOCO_CLI_INIT_POSTGRES_CONNECTION=direct \
dinoco init
```

Passe `--migration-engine manual` para iniciar um projeto com [migrations manuais](/pt-br/docs/orm/guide/manual-migrations): o schema recebe `migration_engine = "manual"` e o `dinoco/migrations/mod.rs` é criado. O padrão é `automatic`.

```bash
dinoco init --migration-engine manual
```

## dinoco migrate generate

```bash
dinoco migrate generate
```

Com `migration_engine = "manual"` este comando recebe um nome e apenas cria o esqueleto de uma migration Rust — veja [dinoco migrate generate (motor manual)](#dinoco-migrate-generate-motor-manual) abaixo. O restante desta seção descreve o motor `automatic`, o padrão.

Esse é o comando completo do loop de desenvolvimento: compila e valida o schema, inspeciona o banco ao vivo, planeja a mudança, pede confirmação, gera e aplica a migration, e por fim regenera os models Rust — tudo numa única chamada.

Com workspaces, escolha um explicitamente com `dinoco migrate generate --workspace dev` (ou `-w dev`); a migration então vai parar em `dinoco/migrations/dev/`. Sem a flag, a CLI pergunta interativamente qual workspace usar.

Ambiente necessário para esse comando: a variável que `database_url` nomeia, mais `SNOWFLAKE_NODE_ID` (ou o que o schema nomear no lugar) se algum field usar `snowflake()`. Em PostgreSQL e MySQL, a validação de dialeto acontece em tabelas isoladas reservadas sob o prefixo `dinoco_migration_test_`, dentro do próprio banco — não existe um banco shadow separado para provisionar.

## dinoco migrate run

```bash
dinoco migrate run
```

Aplica cada `up.sql` pendente em ordem de diretório e registra cada um em `dinoco_migrations`. Com o motor manual, executa cada migration Rust pendente e registra em `dinoco_manual_migrations`. Esse é o comando que você quer num pipeline de deploy, rodado só depois que os arquivos de migration gerados já passaram por review — ele nunca planeja uma migration nova por conta própria.

Use `--workspace nome`/`-w nome` para aplicar só as migrations pendentes daquele workspace específico.

## dinoco migrate generate (motor manual)

```bash
dinoco migrate generate create_users
```

Cria `dinoco/migrations/<timestamp>_create_users.rs`, registra em `dinoco/migrations/mod.rs` e regenera os models Rust. Nunca conecta ao banco. Sem o nome, a CLI pergunta. No motor `automatic` o nome é ignorado com um aviso.

## dinoco migrate rollback

```bash
dinoco migrate rollback [--steps N]
```

Somente motor manual. Executa o `down` das últimas `N` (padrão 1) migrations aplicadas, da mais nova para a mais antiga. Veja o [fluxo das migrations manuais](/pt-br/docs/orm/guide/migration-workflow).

## dinoco migrate status

```bash
dinoco migrate status
```

Somente motor manual. Lista cada migration registrada como `[applied]` ou `[pending]`.

## dinoco models generate

```bash
dinoco models generate
```

Compila e valida o schema, depois reconstrói a árvore de módulos Rust gerada inteira — sem conectar a um banco nem tocar em migrations. Se `dinoco/transform.rs` existir, ele é aplicado à saída (veja [Code transforms](/pt-br/docs/orm/guide/transforms)); o mesmo acontece no `migrate generate`. Use isso especificamente depois de trocar de branch, ou sempre que só o código gerado estiver desatualizado em relação a um schema que na verdade não mudou o banco.

Esse comando também aceita `--workspace nome`/`-w nome`. Trocar de workspace ao gerar remove a árvore anteriormente gerada primeiro, depois a reconstrói do zero para a configuração recém-selecionada — o código gerado dos dois workspaces nunca se mistura.

## Fluxo recomendado

```bash
# Uma vez, ao configurar o projeto
dinoco init

# Depois de toda mudança no schema
dinoco migrate generate

# Ao fazer deploy em outro ambiente
dinoco migrate run

# Quando só o Rust gerado está desatualizado
dinoco models generate
```

> [!TIP]
> A CLI carrega automaticamente um arquivo `.env` local quando presente. Versione um `.env.example` seguro documentando quais variáveis um clone novo precisa configurar — nunca versione o `.env` real com credenciais de verdade dentro.
