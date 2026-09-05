# Configuração

O bloco `config` diz à CLI e ao code generator qual dialeto SQL usar e como chegar ao banco. É a primeira coisa que o Dinoco lê, e é o único lugar num schema onde valores próximos de segredo (connection strings, URLs de réplica) podem aparecer — e mesmo assim, só como referência a uma variável de ambiente, nunca como literal.

> [!TIP]
> Mantenha o `config` no topo do `dinoco/schema.dinoco`. Quem abrir o arquivo pela primeira vez deve conseguir saber com qual banco ele conversa antes de chegar no primeiro `model`.

## Bloco de configuração

```dinoco
config {
    database       = "postgresql"
    connection     = "direct"
    database_url   = env("DATABASE_URL")
    read_replicas  = [env("DATABASE_REPLICA_1"), env("DATABASE_REPLICA_2")]
}
```

- `database` — `"postgresql"`, `"mysql"` ou `"sqlite"`. É a única opção que muda qual dialeto SQL é gerado.
- `connection` — só relevante para PostgreSQL; `"direct"` ou `"pgbouncer"`. Assume `"direct"` quando omitido.
- `database_url` — sempre `env("NOME")`, nunca uma string literal. Veja [Variáveis de ambiente](#variaveis-de-ambiente) abaixo.
- `read_replicas` — um array opcional de entradas `env("NOME")`. Veja [Réplicas de leitura](#replicas-de-leitura).

## Imports de arquivos do schema

Um schema grande não precisa viver em um único arquivo. O `dinoco/schema.dinoco` raiz pode carregar arquivos filhos inteiros sem relistar cada model e enum que eles declaram:

```dinoco
config {
    imports      = ["entities/accounts.dinoco", "entities/businesses.dinoco"]
    database     = "postgresql"
    database_url = env("DATABASE_URL")
}
```

Todo model e enum declarado diretamente em um arquivo listado fica visível para o `schema.dinoco`, como se tivesse sido declarado ali. Algumas regras mantêm isso previsível:

- Os caminhos são relativos ao schema principal e precisam apontar para arquivos `.dinoco` reais.
- O mesmo caminho não pode aparecer duas vezes, e ciclos de import são rejeitados.
- `imports` é uma configuração de nível de config: com workspaces, ela vive diretamente em `config`, não dentro de um bloco `workspace { ... }` individual.
- Só o `schema.dinoco` **raiz** pode usar `config.imports`. Um arquivo filho que precisa de algo de outro lugar usa um import nomeado explícito:

```dinoco
import { AccountStatus } from "../enums.dinoco"

model Account {
    id     String        @id @default(uuid())
    status AccountStatus
}
```

> [!NOTE]
> Um arquivo filho só enxerga suas próprias declarações mais o que ele importa explicitamente — ele **não** herda o escopo de `config.imports` do arquivo raiz. Isso é proposital: mantém o ponto de entrada compacto e ao mesmo tempo mantém toda dependência entre arquivos filhos visível no ponto onde é usada, em vez de implícita através de um escopo global compartilhado. Veja [Organização do schema](/pt-br/docs/orm/guide/schema-organization) para layouts de projeto multi-arquivo, regras de escopo e um exemplo completo.

## Custom derives

`custom_derives` anexa macros derive Rust adicionais a todo enum ou struct de model gerado, no projeto inteiro:

```dinoco
config {
    database       = "sqlite"
    database_url   = env("DATABASE_URL")
    custom_derives = [
        {
            into   = "enum"
            derive = "ZodSchema"
            import = "use zod_rs::prelude::*;"
        },
        {
            into   = "struct"
            derive = "Validate"
            import = "use validator::Validate;"
        }
    ]
}
```

Cada entrada precisa dos três campos: `into` é `"enum"` ou `"struct"`, `derive` é um caminho Rust válido, e `import` é uma declaração `use ...;` de uma linha trazendo esse caminho para o escopo. A crate que fornece o derive continua sendo uma dependência da sua aplicação para adicionar — o Dinoco só conecta a anotação e o `use` no código gerado. Mantenha `custom_derives` no nível principal de `config`, fora de um workspace. Veja [Organização do schema](/pt-br/docs/orm/guide/schema-organization#custom-derives) para o que exatamente o codegen produz.

## Workspaces

Use `workspace` quando um schema precisa rodar contra mais de uma configuração de banco — tipicamente um SQLite local em desenvolvimento e um PostgreSQL de verdade em produção:

```dinoco
config {
    workspace {
        dev {
            database     = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database     = "postgresql"
            connection   = "pgbouncer"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

Cada workspace nomeado é uma configuração **completa** e independente — incluindo seus próprios `read_replicas` opcionais — e todo workspace precisa declarar pelo menos `database` e `database_url`.

> [!WARNING]
> Um bloco `config` é ou configurações de banco no nível principal, ou um bloco `workspace`, nunca os dois. Misturar `database`/`database_url` no nível principal com um `workspace { ... }` é rejeitado pelo compiler.

Selecione um workspace com `--workspace dev`/`-w dev` em `migrate generate`, `migrate run` e `models generate`. Sem a flag, a CLI pergunta interativamente. As migrations de cada workspace ficam em seu próprio diretório, `dinoco/migrations/<workspace>/`, então os históricos de `dev` e `prod` nunca colidem.

## Variáveis de ambiente

`database_url`, cada item de `read_replicas` e `snowflake_node_id` aceitam **somente** `env("NOME")` — uma string literal em qualquer uma dessas posições é erro de compilação, não um aviso. Isso não é só estilo: é o que mantém um schema seguro para versionar.

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
export DATABASE_REPLICA_1="postgres://reader:secret@replica-1:5432/app"
```

A variável nomeada é resolvida duas vezes, de forma independente: uma pela CLI quando ela conecta para planejar ou aplicar uma migration, e outra pela função `dinoco::connect()` gerada em runtime. As duas precisam da variável definida em seus respectivos ambientes.

## PostgreSQL

Use `connection = "direct"` para uma connection string PostgreSQL comum:

```dinoco
config {
    database     = "postgresql"
    connection   = "direct"
    database_url = env("DATABASE_URL")
}
```

Use `connection = "pgbouncer"` quando o `DATABASE_URL` na verdade aponta para um endpoint do PgBouncer, e não para o PostgreSQL diretamente:

```dinoco
config {
    database     = "postgresql"
    connection   = "pgbouncer"
    database_url = env("DATABASE_URL")
}
```

Os dois modos compartilham o mesmo compiler SQL e a mesma API de queries gerada — a diferença está inteiramente na conexão e no tratamento de statements, nunca na sintaxe do schema ou no formato do código gerado.

## Logger de queries e pool Direct

`with_logger` faz o client gerado imprimir o SQL e os parâmetros de cada query que executa. O default é `false`, e pode ser definido tanto no nível principal quanto dentro de um workspace individual:

```dinoco
config {
    database       = "postgresql"
    connection     = "direct"
    database_url   = env("DATABASE_URL")
    with_logger    = true
    min_connection = 2
    max_connection = 10
}
```

`min_connection` e `max_connection` só se aplicam ao PostgreSQL Direct. Os defaults são `2` e `10`, ambos precisam ser inteiros positivos, e `min_connection` não pode ser maior que `max_connection`. O Dinoco abre o mínimo configurado imediatamente na inicialização e nunca cresce o pool além do máximo configurado.

> [!WARNING]
> Os parâmetros de query logados podem conter dados da aplicação — emails de usuário, tokens embutidos em um filtro, o que a query tocar. Habilite `with_logger` só em ambientes onde isso for aceitável, e nunca deixe ligado por padrão em produção.

## MySQL

O MySQL tem um único modo de conexão — não há distinção Direct/PgBouncer a fazer:

```dinoco
config {
    database     = "mysql"
    database_url = env("DATABASE_URL")
}
```

Uma connection string típica é `mysql://usuario:senha@localhost:3306/banco`.

## SQLite

Para SQLite, `DATABASE_URL` é um caminho de arquivo, não um endereço de rede. Caminhos relativos partem da pasta do projeto Dinoco, o que é uma forma conveniente de manter o arquivo do banco ao lado do schema durante o desenvolvimento:

```dinoco
config {
    database     = "sqlite"
    database_url = env("DATABASE_URL")
}
```

```bash
export DATABASE_URL="database.sqlite"
```

## Réplicas de leitura

```dinoco
read_replicas = [env("DATABASE_REPLICA_1"), env("DATABASE_REPLICA_2")]
```

O `connect()` gerado resolve e constrói um adapter para cada réplica que o workspace ativo declara. Em runtime, leituras de `find_first`/`find_many` se intercalam entre elas em round-robin; quando a lista está vazia, as leituras simplesmente vão para a primária. Duas coisas nunca tocam uma réplica, por design:

- **Writes.** Todo insert, update e delete sempre executa na primária.
- **Uma leitura que opta por sair disso.** Chame `.read_in_primary()` em `find_first`/`find_many` quando uma leitura precisa observar um write que acabou de acontecer — o lag de replicação tornaria essa leitura pouco confiável. `find_and_update` é, ele mesmo, um write, então sempre roda na primária de qualquer forma.

> [!NOTE]
> Comandos de migration da CLI nunca usam réplicas. `migrate generate` e `migrate run` conectam apenas ao `database_url` primary do workspace ativo; as réplicas devem se atualizar sozinhas através do próprio mecanismo de replicação do banco.

## IDs Snowflake

Um schema que usa `@default(snowflake())` em qualquer lugar também precisa declarar de onde vem o node ID:

```dinoco
config {
    database          = "postgresql"
    database_url      = env("DATABASE_URL")
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}
```

```bash
export SNOWFLAKE_NODE_ID="7"
```

> [!DANGER]
> Todo processo concorrente que gera Snowflakes precisa usar um node ID **distinto**. Dois processos compartilhando o mesmo node ID podem gerar IDs colidentes sob carga — este é o único erro de configuração nesta página que corrompe dados silenciosamente, em vez de falhar de forma visível.
