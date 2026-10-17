# Referência da CLI

O binário `dinoco` é o ponto de entrada para todo workflow do projeto, seja num terminal, no CI ou num pipeline de deploy. Rode-o a partir da raiz do projeto Cargo: caminhos como `dinoco/schema.dinoco` são sempre resolvidos relativos ao diretório atual.

## Opções globais

Estas opções funcionam com todos os comandos:

| Opção | Efeito |
| --- | --- |
| `--non-interactive` | Nunca pergunta. Um comando que precisa de uma decisão para com exit code `2` e diz qual opção a fornece. |
| `--no-color` | Desliga as cores da saída. A variável de ambiente `NO_COLOR` faz o mesmo. |
| `-v`, `--version` | Imprime a versão. |
| `-h`, `--help` | Imprime a ajuda do comando, com exemplos. |

## Uso interativo e não interativo

A CLI só faz perguntas quando a entrada padrão e a saída de erro são terminais, `--non-interactive` não foi passado e a variável de ambiente `CI` não está definida. Fora disso, ela nunca espera por entrada. Cada decisão vem de uma opção de linha de comando e, quando falta uma, o comando para com exit code `2` e um erro que diz qual opção passar:

```text
error: This migration needs approval.

help: Review the plan above, then pass --yes to approve it.
```

Respostas escritas num pipe (`echo y | dinoco migrate generate`) não são lidas. Passe a opção correspondente.

A CLI nunca escolhe um workspace, um rename ou uma substituição de valor de enum por você. Quando uma dessas decisões é necessária e não há a quem perguntar, o comando falha antes de escrever ou aplicar qualquer coisa.

## Saída, cores e erros

A saída da CLI é em inglês. Os resultados vão para a saída padrão: títulos de seção, campos alinhados e mudanças planejadas marcadas com `+` (adição), `~` (modificação) e `-` (remoção). Avisos, erros e progresso vão para a saída de erro, então redirecionar a saída padrão captura só os resultados.

Cores só são usadas quando a stream é um terminal. `NO_COLOR` e `--no-color` desligam as cores, e `CLICOLOR_FORCE=1` as liga para saída redirecionada. Saída redirecionada não contém códigos ANSI, a menos que você os force.

Os erros seguem um formato: o que falhou, os fatos sobre o que falhou, o motivo (quando é conhecido) e o que fazer a seguir.

```text
error: Unable to connect to the database.

  Database:  PostgreSQL
  Location:  postgresql://***@127.0.0.1:5432/app

Caused by:
  Failed to create the configured minimum PostgreSQL connections
  Error occurred while creating a new object: error connecting to server
  error connecting to server
  Connection refused (os error 61)

help: Check that the database is running and reachable from this machine.
help: Check that the connection URL and its credentials are correct.
```

As linhas sob `Caused by:` vêm do driver do banco, sem alteração além das credenciais ocultas.

URLs de conexão são sempre impressas com as credenciais ocultas: a parte `usuario:senha@` e os parâmetros de query com cara de senha (`password`, `pass`, `pwd`, `sslpassword`, `token`, `secret`) viram `***`.

## Exit codes

| Código | Significado |
| --- | --- |
| `0` | Sucesso, inclusive "nenhuma mudança encontrada". |
| `1` | A operação falhou: erro de conexão, de schema, de SQL ou de arquivo. |
| `2` | A linha de comando é inválida, ou uma decisão é necessária e nenhuma opção a forneceu. |
| `130` | Você cancelou uma pergunta (Esc ou Ctrl+C). Nada foi alterado. |

## dinoco init

```bash
dinoco init
dinoco init --database postgresql --connection direct
```

Cria `dinoco/migrations/` e um `dinoco/schema.dinoco` inicial já formatado, com `database_url = env("DATABASE_URL")` já conectado.

| Opção | Valores |
| --- | --- |
| `--database` | `postgresql`, `mysql` ou `sqlite`. Perguntado quando omitido; obrigatório sem terminal. |
| `--connection` | `direct` ou `pgbouncer`. Só para PostgreSQL. Perguntado quando omitido; `direct` sem terminal. |

> [!NOTE]
> Se um schema já existir nesse caminho, `init` o deixa intocado e avisa, em vez de sobrescrever seu trabalho. Rodar de novo é sempre seguro.

As variáveis de ambiente `DINOCO_CLI_INIT_DATABASE` e `DINOCO_CLI_INIT_POSTGRES_CONNECTION` continuam respondendo às mesmas duas perguntas, para scripts escritos para releases anteriores.

## dinoco migrate generate

```bash
dinoco migrate generate
```

Esse é o comando completo do loop de desenvolvimento. Ele compila e valida o schema, inspeciona o banco ao vivo, mostra o plano, pede aprovação, escreve e aplica a migration, e regenera os models Rust.

A saída começa pelo **Target**: o workspace (quando existe), o banco, sua localização com as credenciais ocultas e o diretório de migrations. Todos os passos seguintes usam exatamente esse alvo. Depois vêm o **Migration Plan**, com uma linha por mudança e uma contagem por tipo, o **Data Impact** das mudanças destrutivas e as **Actions** que a aprovação vai executar. Gerar os arquivos e aplicá-los aparecem como passos separados.

Num terminal, a pergunta de aprovação oferece seguir em frente, mostrar o SQL da migration antes, ou cancelar. Quando o plano tem mudanças destrutivas, a pergunta começa em **Cancel**, então apertar Enter sozinho nunca aprova perda de dados.

### Opções

| Opção | Efeito |
| --- | --- |
| `-w`, `--workspace <NAME>` | Workspace a usar. Veja [Workspaces](#workspaces). |
| `--schema-from <NAME>` | Aplica o schema salvo para o workspace `NAME` em vez do `dinoco/schema.dinoco`. Veja [Workspaces](#workspaces). |
| `-y`, `--yes` | Aprova o plano sem perguntar. Planos com mudanças destrutivas precisam de `--allow-destructive`. |
| `--allow-destructive` | Aprova o plano, inclusive mudanças que apagam ou reescrevem dados. Implica `--yes`. |
| `--create-only` | Escreve os arquivos da migration sem aplicá-los. |
| `--accept-renames` | Confirma todo rename que o planner inferiu. |
| `--rename <MAPPING>` | Confirma um rename inferido: `TABELA.ANTIGO=NOVO` para uma coluna, `ANTIGO=NOVO` para uma tabela. Pode repetir. |
| `--no-rename <MAPPING>` | Planeja um rename inferido como remoção mais adição. Mesmo formato. Pode repetir. |
| `--replace-enum-value <MAPPING>` | Tira as linhas de um valor de enum removido: `ENUM.VALOR=SUBSTITUTO`. Pode repetir. |
| `--repair-drift` | Aprova reparar drift do schema (mudanças feitas fora das migrations do Dinoco). |
| `--adopt-baseline` | Aprova adotar como baseline das migrations tabelas existentes que já batem com o schema. |

Um `--rename` que não corresponde a nenhum rename inferido, um mapeamento mal formado ou um valor substituto não permitido falha com exit code `2`. Um erro de digitação nunca é ignorado.

Veja [Migrations](/pt-br/docs/orm/tooling/migrations) para como renames e substituições de enum são planejados.

### Crie a migration sem aplicá-la

```bash
dinoco migrate generate --create-only --yes
```

`--create-only` planeja contra o banco ao vivo e escreve `up.sql` e `down.sql`, mas não aplica a migration, não a registra e não regenera os models. Revise o SQL, depois rode `dinoco migrate run` para aplicá-la e `dinoco models generate` para atualizar o código Rust.

`--create-only` só funciona quando a migration é a mudança inteira. Quando o banco primeiro precisa de um reparo de drift, da adoção de um baseline ou da atualização de um histórico de migrations de uma release anterior, esses passos alteram o banco, então o comando para com exit code `2` e pede para rodá-lo sem `--create-only`.

### No CI

```bash
dinoco migrate generate --non-interactive --yes \
  --rename user.username=display_name \
  --replace-enum-value Status.Processing=Pending
```

Ambiente necessário: a variável que `database_url` nomeia, mais `SNOWFLAKE_NODE_ID` (ou o que o schema nomear no lugar) se algum field usar `snowflake()`. Em PostgreSQL e MySQL, o plano é calculado a partir do schema sem criar nenhuma tabela; o SQLite monta o schema desejado num arquivo de banco temporário, apagado ao final.

## dinoco migrate run

```bash
dinoco migrate run
```

Aplica cada `up.sql` pendente em ordem de diretório e registra cada um em `dinoco_migrations`, depois informa quantas foram aplicadas, ou que o banco já está em dia. Esse é o comando para um pipeline de deploy, rodado depois que os arquivos de migration gerados passaram por review. Ele nunca planeja uma migration nova e nunca pergunta nada, exceto para escolher um workspace.

Use `--workspace nome`/`-w nome` para aplicar as migrations pendentes de um workspace.

## dinoco models generate

```bash
dinoco models generate
```

Compila e valida o schema, depois reconstrói a árvore de módulos Rust gerada, sem conectar a um banco nem tocar em migrations. Se `dinoco/transform.rs` existir, ele é aplicado à saída (veja [Code transforms](/pt-br/docs/orm/guide/transforms)); o mesmo acontece no `migrate generate`. Use depois de trocar de branch, ou sempre que só o código gerado estiver desatualizado.

Esse comando também aceita `--workspace nome`/`-w nome`, e `--schema-from nome` para gerar os models a partir do schema salvo para outro workspace. Trocar o workspace para o qual você gera remove a árvore gerada anteriormente primeiro, depois a reconstrói para a configuração recém-selecionada, então o código gerado de dois workspaces nunca se mistura.

## Workspaces

Quando o schema define workspaces, todo comando precisa saber qual usar:

- `--workspace nome` o seleciona. Um nome desconhecido falha com exit code `2` e lista os workspaces definidos.
- Quando o schema define um único workspace, ele é usado, e a seção **Target** o mostra.
- Caso contrário, a CLI pergunta em qual workspace aplicar as migrations, listando cada um com a variável de ambiente de onde vem a URL do seu banco. Sem terminal, o comando falha com exit code `2` e pede `--workspace`.

`migrate generate` e `models generate` perguntam em seguida qual schema usar, quando há mais de um workspace e outro workspace já tem um [schema salvo](/pt-br/docs/orm/guide/configuration#schema-salvo-por-workspace): o `dinoco/schema.dinoco`, o schema que você edita, ou a cópia salva para outro workspace, como a do dev quando o alvo é prod. Uma cópia idêntica ao `dinoco/schema.dinoco` aparece marcada assim. A seção **Target** mostra o schema escolhido.

- `--schema-from nome` responde essa pergunta: `dinoco migrate generate -w prod --schema-from dev` aplica o schema do dev no prod. Um nome que não é workspace, ou um workspace ainda sem schema salvo, falha com exit code `2`.
- Sem terminal e sem `--schema-from`, o `dinoco/schema.dinoco` é usado.
- Quando o schema vem de outro workspace, ele é salvo como a cópia do workspace selecionado assim que o comando termina com sucesso. Da cópia vêm só os models e enums: as configurações do banco continuam vindo do workspace selecionado no `dinoco/schema.dinoco`, e esse arquivo nunca é alterado.

## Variáveis de ambiente de releases anteriores

Estas variáveis continuam funcionando, para que scripts existentes não quebrem. Scripts novos devem usar as opções.

| Variável | Equivalente |
| --- | --- |
| `DINOCO_CLI_CONFIRM_MIGRATION=true` | Aprova o plano como mostrado, inclusive mudanças destrutivas e renames inferidos, como `--allow-destructive --accept-renames`. Valores de enum removidos que linhas ainda usam ficam como estão, com um aviso. |
| `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` | O mesmo, para planos com mudanças destrutivas. |
| `DINOCO_CLI_CONFIRM_DRIFT=true` | `--repair-drift --adopt-baseline`. |
| `DINOCO_CLI_INIT_DATABASE`, `DINOCO_CLI_INIT_POSTGRES_CONNECTION` | `init --database`, `init --connection`. |

## Fluxo recomendado

```bash
# Uma vez, ao configurar o projeto
dinoco init

# Depois de toda mudança no schema
dinoco migrate generate

# Ao fazer deploy em outro ambiente
dinoco migrate run

# Quando outro workspace deve receber o schema com que o dev foi gerado
dinoco migrate generate --workspace prod --schema-from dev

# Quando só o Rust gerado está desatualizado
dinoco models generate
```

> [!TIP]
> A CLI carrega um arquivo `.env` local quando presente. Versione um `.env.example` seguro documentando quais variáveis um clone novo precisa configurar, e nunca versione o `.env` real com credenciais dentro.
