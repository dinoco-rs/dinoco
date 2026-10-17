# Migrations

Uma migration do Dinoco é um artefato SQL produzido a partir de um diff validado entre seu schema e o estado real do banco — nunca de um "schema anterior" imaginado ou em cache. O `SqlCompiler` do adapter ativo escreve os statements de enum, tabela, coluna e foreign key específicos do dialeto necessários para fechar essa diferença.

## Ciclo de uma migration

1. Compilar `dinoco/schema.dinoco` — exigindo exatamente uma primary key por model, e validando tipos, atributos de model e relações antes de tocar no banco.
2. Conectar ao banco primary e introspectar sua estrutura real e atual.
3. Construir a estrutura desejada do schema: em memória para PostgreSQL e MySQL, num arquivo de banco temporário para SQLite.
4. Comparar as estruturas atual e desejada.
5. Imprimir cada passo planejado, sinalizando qualquer coisa insegura ou destrutiva.
6. Tomar as decisões que o plano precisa: confirmar ou rejeitar renames inferidos, escolher substitutos para valores de enum removidos que linhas ainda usam, e aprovar o plano. Planos destrutivos precisam de aprovação explícita.
7. Escrever `up.sql` e `down.sql`, aplicar o `up.sql`, registrar a migration e regenerar os models Rust. Com `--create-only`, parar depois de escrever os arquivos.

> [!NOTE]
> O próprio banco — não um snapshot binário de schema em cache de uma execução anterior — é sempre a fonte de verdade para o "estado atual" nessa comparação. Não existe um arquivo de metadados separado que possa divergir da realidade.

## Gere uma migration

Você só precisa da URL do banco primary configurada:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
dinoco migrate generate
```

Em PostgreSQL e MySQL, a estrutura desejada é calculada a partir do schema, sem criar nada no banco. O SQLite a monta num arquivo de banco temporário, apagado quando o planejamento termina. Nenhum banco ou URL shadow é necessário para nenhum adapter.

Antes de qualquer decisão, o comando imprime o **Target** em que vai trabalhar: o workspace (quando existe), o banco, sua localização com as credenciais ocultas e o diretório de migrations.

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

## Renames

A detecção de rename é estrutural: quando uma tabela ou coluna removida corresponde a exatamente uma adicionada, o planner infere um rename, para que os dados existentes acompanhem a mudança. Uma remoção de verdade seguida de uma adição sem relação pode parecer igual, então um rename inferido nunca é aplicado sem uma decisão.

Num terminal, o Dinoco pergunta sobre cada rename inferido:

```text
Migration Planning
A possible column rename was detected.

  Table    user
  Removed  username
  Added    display_name

? Is this a column rename?
> Yes, rename `username` to `display_name` and keep its values
  No, drop `username` (its values are deleted) and add `display_name`
  Cancel migration planning
```

Sem terminal, passe a decisão como opção. O erro lista cada rename que precisa de uma:

```bash
# Manter os dados: renomear a coluna
dinoco migrate generate --yes --rename user.username=display_name

# Planejar remoção mais adição (os valores antigos são apagados)
dinoco migrate generate --allow-destructive --no-rename user.username=display_name

# Confirmar todo rename inferido
dinoco migrate generate --yes --accept-renames
```

Os mapeamentos são `TABELA.ANTIGO=NOVO` para uma coluna e `ANTIGO=NOVO` para uma tabela, com o nome de tabela que o plano mostra; a mensagem de erro imprime o mapeamento exato a passar. Um rename confirmado não é uma mudança destrutiva, então `--yes` basta. Quando um nome novo corresponde a mais de um removido, o plano é ambíguo: o Dinoco para antes de escrever qualquer coisa e pede para você mudar o schema em passos menores.

## Valores de enum removidos

Remover um valor de um enum falha, ou perde dados, quando linhas ainda o usam. Antes de aplicar essa mudança, o Dinoco conta as linhas que guardam cada valor removido. Quando há alguma, ele pede um substituto:

```text
Enum Migration
The enum value "PROCESSING" is being removed, but existing rows still use it.

  Enum   Status
  Value  PROCESSING
  Usage  12 rows in task.status

? Select the replacement value:
> ACTIVE
  DONE
  Cancel migration planning
```

Só aparecem valores que existem antes da migration e continuam nela, porque as linhas não podem passar para um valor que o banco ainda não aceita. Sem terminal, passe `--replace-enum-value ENUM.VALOR=SUBSTITUTO`:

```bash
dinoco migrate generate --allow-destructive --replace-enum-value Status.PROCESSING=DONE
```

O plano então mostra uma seção **Data Changes** (`~ Replace `PROCESSING` with `DONE` in `task.status` (12 rows)`), e o `up.sql` começa com o `UPDATE` correspondente, antes que qualquer coluna deixe de aceitar o valor antigo. Como os valores originais são sobrescritos, o `down.sql` explica que a migration não pode ser revertida em vez de gerar SQL.

Isso vale para MySQL e SQLite, que guardam os valores de enum por coluna. O PostgreSQL não consegue remover um valor de um tipo enum, então um plano que remove um é recusado com uma explicação antes de qualquer pergunta sobre substitutos; escreva uma migration customizada e revisada para isso.

## Migrations de índices

O planner trata índices comuns, unique e full-text como três preocupações separadas. `@index`/`@@indexes` geram statements B-tree não únicos. Um `@@uniques` composto gera `CREATE UNIQUE INDEX`. `@fulltext`/`@@fulltexts` geram um índice GIN no PostgreSQL, um índice `FULLTEXT` no MySQL, e nenhum índice no SQLite (veja [Busca full-text](/pt-br/docs/orm/orm/full-text-search) para entender por quê).

Primary keys (`@id`/`@@ids`) aparecem no modelo de comparação do schema desejado, mas a própria constraint já satisfaz o índice — o Dinoco nunca emite um `CREATE INDEX` duplicado e redundante para ela. Toda foreign key também ganha seu índice automático, relações compostas e os dois lados de uma pivô many-to-many implícita inclusos.

Qualquer mudança no nome, colunas, ordem ou tipo de um índice produz os passos de drop/create correspondentes. Veja [Índices e constraints](/pt-br/docs/orm/guide/indexes) para as regras no nível do schema por trás de tudo isso.

## Alterações perigosas

Remover uma tabela ou coluna populada, estreitar o tipo de uma coluna, remover valores de enum, e tornar uma coluna nullable obrigatória podem todos destruir dados ou falhar de imediato contra rows existentes. O Dinoco lista esses casos em **Data Impact** e, num terminal, a pergunta de aprovação começa em **Cancel**, então você precisa optar ativamente por continuar. Sem terminal, `--yes` não basta: passe `--allow-destructive`.

Aprovar não basta para uma mudança que as rows existentes não aceitam. Antes de perguntar, e antes de escrever qualquer arquivo, o `migrate generate` confere as rows: uma coluna que vira obrigatória enquanto alguma row guarda `NULL` nela, ou uma coluna obrigatória sem default adicionada a uma tabela com rows, para o comando com a tabela, a coluna e o número de rows, e nada é escrito nem alterado. Preencha esses valores, ou mantenha a coluna opcional (ou dê a ela um `@default`) até as rows serem preenchidas. Isso importa sobretudo no MySQL, que confirma cada comando DDL sozinho e, sem a verificação, ficaria migrado pela metade. O `--create-only` pula a verificação, já que não aplica nada.

A primeiríssima migration contra um banco que já tem tabelas de usuário, mas ainda não tem uma tabela `dinoco_migrations`, também precisa de aprovação (`--adopt-baseline` sem terminal). Ela só é possível quando as tabelas existentes batem exatamente com o schema, o que protege um banco que não era gerenciado pelo Dinoco de uma primeira migration não revisada que assume ser dona de tudo. Reparar drift, mudanças feitas no banco fora das migrations do Dinoco, também precisa de aprovação (`--repair-drift`).

> [!NOTE]
> Trate `--allow-destructive` no CI como uma configuração privilegiada e deliberada de pipeline de release, não como algo adicionado a toda chamada para parar de ser perguntado. A variável `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` de releases anteriores continua funcionando do mesmo jeito.

## Crie uma migration sem aplicá-la

```bash
dinoco migrate generate --create-only --yes
```

`--create-only` toma as mesmas decisões e escreve os mesmos `up.sql` e `down.sql`, mas não aplica nem registra a migration e não regenera os models. Revise os arquivos, depois aplique-os com `dinoco migrate run` e atualize o código Rust com `dinoco models generate`.

Quando o banco primeiro precisa de um reparo de drift, da adoção de um baseline ou da atualização de um histórico de migrations de uma release anterior, `--create-only` é recusado, já que esses passos alteram o banco.

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

`migrate generate` sempre regenera `dinoco/mod.rs` e `dinoco/models/`, mesmo quando o planner não encontra nenhuma mudança de banco para fazer — os models ainda são atualizados antes do comando terminar. A exceção é `--create-only`, que deixa isso para o `dinoco models generate` depois que a migration é aplicada. Isso mantém a saída Rust gerada alinhada com mudanças de schema ou codegen que não chegam a precisar tocar no banco.
