# Migrations

Migrations do Dinoco são artefatos SQL produzidos a partir de um diff validado. O `SqlCompiler` do adapter escreve statements de enum, tabela, coluna e foreign key no dialeto correto.

## Ciclo de uma migration

1. Compilar `dinoco/schema.dinoco`, exigir exatamente uma primary key por model e validar tipos, atributos de model e relações.
2. Conectar ao primary e inspecionar a estrutura real.
3. Construir o schema desejado em tabelas de teste isoladas no próprio banco.
4. Comparar as estruturas atual e desejada.
5. Mostrar cada passo e todo risco detectado.
6. Pedir confirmação para alterações destrutivas.
7. Gerar `up.sql` e `down.sql`, aplicar o up, registrar a migration e gerar models.

O estado atual vem do banco por introspecção, não de um `schema.bin` antigo.

## Gere uma migration

Configure apenas a URL do banco principal:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
dinoco migrate generate
```

Em PostgreSQL e MySQL, a CLI materializa o schema desejado no próprio banco usando tabelas isoladas com o prefixo `dinoco_migration_test_`. Essas tabelas, suas foreign keys e os enums auxiliares são removidos assim que o planejamento termina, inclusive quando ocorre um erro. Em SQLite, a validação continua usando um arquivo temporário.

O prefixo `dinoco_migration_test_` é reservado pelo Dinoco e não deve ser usado por tabelas da aplicação. Nenhuma URL shadow adicional é necessária.

## Como as alterações são detectadas

O planner compara:

- tabelas criadas e removidas;
- colunas adicionadas, removidas, renomeadas ou alteradas;
- mudanças de tipo escalar e tipo nativo;
- opcional para obrigatório e obrigatório para opcional;
- defaults, primary keys e constraints observáveis;
- enums criados, alterados e removidos;
- foreign keys e ações referenciais adicionadas, mudadas ou removidas;
- índices comuns declarados com `@index` ou `@@indexes`, grupos unique declarados com `@@uniques`, índices full-text declarados com `@fulltext` ou `@@fulltexts`, índices de primary keys fornecidos pelas próprias constraints e índices automáticos de foreign keys;
- relações adicionadas ou removidas por suas constraints físicas.

A detecção de rename é estrutural. Sempre confirme a inferência, pois um drop e um add independentes podem ter forma parecida.

## Migrations de índices

O planner trata separadamente índices comuns, unique e full-text. `@index` e `@@indexes` geram statements B-tree não únicos. `@@uniques` composto gera `CREATE UNIQUE INDEX`. `@fulltext` e `@@fulltexts` geram GIN no PostgreSQL, `FULLTEXT` no MySQL e nenhum índice no SQLite.

Primary keys declaradas com `@id` ou `@@ids` aparecem no schema desejado, mas a própria constraint satisfaz o índice e evita um `CREATE INDEX` duplicado. Toda foreign key recebe um índice automático, inclusive relações compostas e os dois lados de uma pivot many-to-many implícita.

Mudanças de nome, colunas, ordem ou tipo geram os passos de drop/create correspondentes. Consulte [Índices e constraints](/v1.2.4/guide/indexes) para as regras do schema.

## Alterações perigosas

Remover tabela ou coluna populada, estreitar tipo, remover enum value ou tornar uma coluna nullable obrigatória pode perder dados ou falhar para rows atuais. A CLI destaca o risco e deixa a resposta padrão da confirmação como `No`.

Um banco com tabelas de usuário, mas sem `dinoco_migrations`, também exige confirmação. Em CI, `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` pode confirmar riscos; trate isso como configuração privilegiada de release.

## Revise o SQL gerado

```text
dinoco/migrations/1721320123456_generated/
  up.sql
  down.sql
```

Revise locks, reescrita de dados, impacto de índices e comportamento de enums. Operações irreversíveis aparecem como comentários explicativos no `down.sql`, porque dados apagados não podem ser recriados com segurança.

Faça commit dos dois arquivos e nunca edite uma migration já aplicada; gere outra para manter o histórico igual em todos os ambientes.

## Execute migrations pendentes

```bash
dinoco migrate run
```

O comando cria `dinoco_migrations`, ordena diretórios, ignora migrations já registradas e aplica os `up.sql` pendentes.

## Executar ao iniciar um SQLite local

O `connect()` abre imediatamente a primeira conexão SQLite, criando o arquivo quando ele ainda não existe. O codegen incorpora os `up.sql` do workspace selecionado no módulo `dinoco` e exporta um helper para aplicá-los no mesmo client:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;

if report.changed() {
    println!("Migrations aplicadas: {:?}", report.applied);
}
```

Chamar apenas `connect()` nunca aplica migrations nem cria tabelas da aplicação. Chame `migrate()` explicitamente somente onde a aplicação deve gerenciar o schema do banco local.

Também é possível chamar diretamente `dinoco::runtime::run_migrations`. Em runtime não há compilação do schema nem geração de models: os arquivos SQL já estão embutidos no binário com `include_str!`. As migrations são ordenadas, executadas em transações SQLite e registradas com checksum; remover ou alterar uma migration já aplicada produz erro.

## Models gerados

`migrate generate` sempre atualiza `dinoco/mod.rs` e `dinoco/models/`. Mesmo sem mudança de banco, os models são regenerados antes de encerrar.
