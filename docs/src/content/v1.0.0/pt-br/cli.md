# Referência da CLI

O binário `dinoco` centraliza o fluxo do projeto. Rode os comandos na raiz do crate, pois `dinoco/schema.dinoco` é resolvido pelo diretório atual.

## dinoco init

```bash
dinoco init
```

Pergunta entre PostgreSQL, MySQL e SQLite. PostgreSQL abre uma segunda seleção para Direct ou PgBouncer. Cria `dinoco/migrations/` e um schema formatado com `database_url = env("DATABASE_URL")`.

Se o schema já existir, ele é preservado. Em automação, use:

```bash
DINOCO_CLI_INIT_DATABASE=postgresql \
DINOCO_CLI_INIT_POSTGRES_CONNECTION=direct \
dinoco init
```

## dinoco migrate generate

```bash
dinoco migrate generate
```

Compila e valida o schema, inspeciona o banco, planeja e confirma mudanças, gera e aplica a migration e, por fim, gera os models Rust.

Ambiente relevante: a variável indicada por `database_url` e `SNOWFLAKE_NODE_ID` quando houver `snowflake()`. Em PostgreSQL/MySQL, a validação do dialeto usa tabelas isoladas com o prefixo reservado `dinoco_migration_test_` no próprio banco.

## dinoco migrate run

```bash
dinoco migrate run
```

Executa cada `up.sql` pendente e registra o nome em `dinoco_migrations`. É o comando recomendado em deploy depois do review dos arquivos.

## dinoco models generate

```bash
dinoco models generate
```

Compila o schema e recria os models sem conectar ao banco ou produzir migration. É útil após trocar de branch ou quando somente o código gerado ficou desatualizado.

## Fluxo recomendado

```bash
# Uma vez
dinoco init

# Depois de alterar o schema
dinoco migrate generate

# Em outro ambiente
dinoco migrate run

# Apenas para atualizar Rust gerado
dinoco models generate
```

A CLI carrega `.env` quando o arquivo existe. Versione um `.env.example` seguro, nunca as credenciais reais.
