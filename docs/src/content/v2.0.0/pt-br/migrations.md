# Migrations

Uma migration do Dinoco é um artefato SQL produzido a partir de um diff validado entre seu schema e o estado real do banco — nunca de um "schema anterior" imaginado ou em cache. O `SqlCompiler` do adapter ativo escreve os statements de enum, tabela, coluna e foreign key específicos do dialeto necessários para fechar essa diferença.

## Ciclo de uma migration

1. Compilar `dinoco/schema.dinoco` — exigindo exatamente uma primary key por model, e validando tipos, atributos de model e relações antes de tocar no banco.
2. Conectar ao banco primary e introspectar sua estrutura real e atual.
3. Construir a estrutura desejada do schema dentro de tabelas de teste isoladas, nesse mesmo banco.
4. Comparar as estruturas atual e desejada.
5. Imprimir cada passo planejado, sinalizando qualquer coisa insegura ou destrutiva.
6. Pedir confirmação sempre que dados possam ser perdidos.
7. Escrever `up.sql` e `down.sql`, aplicar o `up.sql`, registrar a migration e regenerar os models Rust.

> [!NOTE]
> O próprio banco — não um snapshot binário de schema em cache de uma execução anterior — é sempre a fonte de verdade para o "estado atual" nessa comparação. Não existe um arquivo de metadados separado que possa divergir da realidade.

## Gere uma migration

Você só precisa da URL do banco primary configurada:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
dinoco migrate generate
```

Em PostgreSQL e MySQL, a CLI materializa o schema desejado diretamente no banco primary, dentro de tabelas isoladas com o prefixo `dinoco_migration_test_`. Essas tabelas, suas foreign keys e quaisquer enums auxiliares são todos removidos assim que o planejamento termina — inclusive quando o planejamento falha com um erro. O SQLite valida através de um arquivo temporário em vez disso.

> [!WARNING]
> O prefixo `dinoco_migration_test_` é reservado pelo Dinoco. Não nomeie uma tabela da aplicação com esse prefixo — o planner de migration vai colidir com ela. Nenhum banco ou URL shadow separado é necessário para nenhum adapter.

## Como as alterações são detectadas

O planner compara, exaustivamente:

- Tabelas criadas e removidas.
- Colunas adicionadas, removidas, renomeadas ou alteradas.
- Mudanças de tipo escalar e mudanças de tipo nativo do banco.
- Transições de opcional-para-obrigatório e obrigatório-para-opcional.
- Defaults, primary keys e unicidade como representados no banco.
- Enums criados, alterados e removidos.
- Foreign keys e ações referenciais adicionadas, mudadas ou removidas.
- Índices comuns (`@index`/`@@indexes`), grupos unique (`@@uniques`), índices full-text (`@fulltext`/`@@fulltexts`), índices de primary key fornecidos pelas próprias constraints, e índices automáticos de foreign key.
- Relações adicionadas ou removidas, via suas constraints físicas.

> [!WARNING]
> A detecção de rename é estrutural, não algo que você declara explicitamente — o planner infere um `RenameColumn` quando consegue casar com confiança um field antigo com um novo. Sempre confira uma renomeação inferida no SQL gerado antes de aplicá-la; um drop de verdade seguido de um add sem relação às vezes pode parecer estruturalmente similar a um rename.

## Migrations de índices

O planner trata índices comuns, unique e full-text como três preocupações separadas. `@index`/`@@indexes` geram statements B-tree não únicos. Um `@@uniques` composto gera `CREATE UNIQUE INDEX`. `@fulltext`/`@@fulltexts` geram um índice GIN no PostgreSQL, um índice `FULLTEXT` no MySQL, e nenhum índice no SQLite (veja [Busca full-text](/pt-br/docs/orm/orm/full-text-search) para entender por quê).

Primary keys (`@id`/`@@ids`) aparecem no modelo de comparação do schema desejado, mas a própria constraint já satisfaz o índice — o Dinoco nunca emite um `CREATE INDEX` duplicado e redundante para ela. Toda foreign key também ganha seu índice automático, relações compostas e os dois lados de uma pivô many-to-many implícita inclusos.

Qualquer mudança no nome, colunas, ordem ou tipo de um índice produz os passos de drop/create correspondentes. Veja [Índices e constraints](/pt-br/docs/orm/guide/indexes) para as regras no nível do schema por trás de tudo isso.

## Alterações perigosas

Remover uma tabela ou coluna populada, estreitar o tipo de uma coluna, remover valores de enum, e tornar uma coluna nullable obrigatória podem todos destruir dados ou falhar de imediato contra rows existentes. O Dinoco imprime um aviso destacado para esses casos e deixa o prompt de confirmação com padrão **No** — você precisa optar ativamente por continuar.

A primeiríssima migration contra um banco que já tem tabelas de usuário, mas ainda não tem uma tabela `dinoco_migrations`, também exige confirmação — isso protege especificamente um banco que não era gerenciado pelo Dinoco antes de uma primeira migration não revisada assumir que é dona de tudo.

> [!NOTE]
> Em CI, `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` pode responder ao prompt de mudança destrutiva de forma não interativa. Trate isso como uma configuração privilegiada e deliberada de pipeline de release — não como um default de conveniência para deixar ligado globalmente só para parar de ser perguntado.

## Revise o SQL gerado

Cada migration ganha seu próprio diretório:

```text
dinoco/migrations/1721320123456_generated/
  up.sql
  down.sql
```

Leia o `up.sql` em busca de comportamento de lock, reescrita de dados, impacto em índices e tratamento de enum específico do adapter antes de aplicá-lo contra qualquer coisa que importe. Leia o `down.sql` também, antes de contar com ele para um rollback: uma operação que o Dinoco não consegue reverter com segurança (porque os valores anteriores ou os dados apagados não podem ser reconstruídos) é escrita ali como um comentário SQL explicativo, em vez de um statement funcional.

Faça commit dos dois arquivos no controle de versão. Nunca edite à mão uma migration que já foi aplicada em algum lugar — crie uma nova para a mudança seguinte, para que todo ambiente mantenha exatamente o mesmo histórico, na mesma ordem.

## Execute migrations pendentes

```bash
dinoco migrate run
```

Isso cria `dinoco_migrations` se ainda não existir, ordena os diretórios de migration, pula o que já está registrado, e aplica os `up.sql` pendentes em ordem — nada aqui planeja uma migration nova, ele só aplica o que já está em disco.

## Executar ao iniciar um SQLite local

O `connect()` abre a primeira conexão SQLite de forma antecipada e cria o arquivo do banco se ele ainda não existir — mas isso sozinho nunca aplica nenhuma migration. A geração de código separadamente embute os arquivos `up.sql` do workspace ativo diretamente no módulo `dinoco` gerado, e exporta um helper que os aplica através do mesmo client:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;

if report.changed() {
    println!("Migrations aplicadas: {:?}", report.applied);
}
```

> [!NOTE]
> Chamar `connect()` sozinho nunca aplica migrations nem cria tabelas da aplicação — chame `migrate(&client)` explicitamente, e só nos lugares específicos onde você de fato quer que a própria aplicação seja dona do gerenciamento do schema do banco local (um app desktop distribuindo seu próprio arquivo SQLite é o caso clássico; um servidor conversando com um PostgreSQL gerenciado centralmente geralmente não é).

Você também pode chamar `dinoco::runtime::run_migrations` diretamente para mais controle. Esse caminho em runtime nunca compila o schema nem gera models — o SQL já está embutido no binário via `include_str!` em tempo de build. As migrations ainda rodam ordenadas, dentro de transactions SQLite de verdade, e são registradas com checksum; remover ou alterar uma migration já aplicada é tratado como erro, não aceito silenciosamente.

## Models gerados

`migrate generate` sempre regenera `dinoco/mod.rs` e `dinoco/models/`, mesmo quando o planner não encontra nenhuma mudança de banco para fazer — os models ainda são atualizados antes do comando terminar. Isso mantém a saída Rust gerada alinhada com mudanças de schema ou codegen que não chegam a precisar tocar no banco.
