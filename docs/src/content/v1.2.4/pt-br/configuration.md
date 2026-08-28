# Configuração

O bloco `config` informa à CLI e ao codegen qual dialeto SQL e estratégia de conexão o projeto usa.

## Bloco de configuração

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = [env("DATABASE_REPLICA_1")]
}
```

`database` aceita `postgresql`, `mysql` ou `sqlite`. Para PostgreSQL, `connection` aceita `direct` ou `pgbouncer` e assume Direct quando não informado.

## Imports de arquivos do schema

O arquivo principal `dinoco/schema.dinoco` pode carregar arquivos completos do schema sem listar o nome de cada model e enum. Declare os caminhos no bloco `config` principal:

```dinoco
config {
    imports = ["entities/accounts.dinoco", "entities/businesses.dinoco"]
    database = "postgresql"
    database_url = env("DATABASE_URL")
}
```

Todos os models e enums declarados diretamente em cada arquivo listado ficam visíveis para o `schema.dinoco`. Os caminhos são relativos ao schema principal, devem apontar para arquivos `.dinoco` e não podem estar duplicados nem formar ciclos. `imports` é uma propriedade global: ao usar workspaces, mantenha-a diretamente em `config`, fora dos blocos de cada workspace.

Somente o arquivo principal `schema.dinoco` aceita `config.imports`. Arquivos filhos continuam usando imports explícitos e nomeados quando dependem de declarações de outro arquivo:

```dinoco
import { AccountStatus } from "../enums.dinoco"

model Account {
    id     String        @id @default(uuid())
    status AccountStatus
}
```

Um arquivo filho enxerga suas próprias declarações e os símbolos nomeados em seus próprios imports; ele não herda o escopo de imports do arquivo principal. Assim, o arquivo de entrada fica compacto sem tornar implícitas as dependências entre os arquivos filhos.

Consulte [Organização do schema](/v1.2.4/guide/schema-organization) para estruturas de projeto, imports nomeados, regras de escopo, validação de caminhos e um exemplo multi-arquivo completo.

## Custom derives

`custom_derives` adiciona macros derive Rust globais a todos os enums ou structs de model gerados:

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
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

Cada objeto exige `into`, `derive` e `import`. O alvo deve ser `"enum"` ou `"struct"`, o derive deve ser um caminho Rust válido e o import deve ser uma declaração `use ...` em uma única linha. Mantenha `custom_derives` no nível principal de `config`, fora dos workspaces. As crates que fornecem as macros continuam sendo dependências da aplicação. Consulte [Organização do schema](/v1.2.4/guide/schema-organization#custom-derives) para a saída gerada e todas as regras de validação.

## Workspaces

Use `workspace` quando o mesmo schema de models precisa operar com mais de uma configuração de banco:

```dinoco
config {
    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database = "postgresql"
            connection = "pgbouncer"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

Cada workspace contém uma configuração completa, incluindo seus próprios `read_replicas` opcionais. Não misture as propriedades de banco no nível principal de `config` com o bloco `workspace`. Selecione o ambiente com `--workspace nome` ou `-w nome`; sem essa opção, a CLI abre uma seleção interativa. As migrations ficam isoladas em `dinoco/migrations/<workspace>/`.

O bloco `workspace` deve conter pelo menos um workspace nomeado, e cada workspace deve declarar `database` e `database_url`.

## Variáveis de ambiente

`database_url`, cada item de `read_replicas` e `snowflake_node_id` aceitam somente `env("NOME")`. O compiler rejeita valores literais.

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
export DATABASE_REPLICA_1="postgres://reader:secret@replica:5432/app"
```

O `database_url` é resolvido pela CLI e pelo `connect()` gerado. O `connect()` também constrói automaticamente os adapters de réplica declarados no workspace selecionado.

## PostgreSQL

Direct usa o pool PostgreSQL comum:

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

PgBouncer aponta para o endpoint do proxy:

```dinoco
connection = "pgbouncer"
```

Os dois usam o mesmo compiler PostgreSQL; muda a estratégia de conexão, não o schema.

## Logger de queries e pool Direct

`with_logger` habilita a exibição do SQL e dos parâmetros nos clients gerados. O default é `false`, e a opção pode ser declarada em uma configuração comum ou dentro de cada workspace:

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    with_logger = true
    min_connection = 2
    max_connection = 10
}
```

`min_connection` e `max_connection` estão disponíveis somente para PostgreSQL Direct. Os defaults são `2` e `10`; ambos devem ser inteiros positivos, e o mínimo não pode ser maior que o máximo. O Dinoco abre imediatamente o mínimo configurado e limita o pool ao máximo. Os parâmetros podem conter dados da aplicação, então habilite o logger somente onde essa saída for apropriada.

## MySQL

```dinoco
config {
    database = "mysql"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

Uma URL típica é `mysql://usuario:senha@localhost:3306/banco`.

## SQLite

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

Para SQLite, a URL é o caminho do arquivo. Caminhos relativos partem da pasta do projeto Dinoco.

```bash
export DATABASE_URL="database.sqlite"
```

## Réplicas de leitura

O `connect()` gerado resolve e constrói todos os adapters de réplica declarados pelo workspace selecionado. O `DinocoClient` intercala finds entre eles em round-robin. Sem réplicas, lê no primary. Writes sempre usam o primary, `.read_in_primary()` força um find e seus includes a lerem nele, e `find_and_update` sempre executa no primary.

Os comandos de migration da CLI nunca usam réplicas. `migrate generate` e `migrate run` conectam somente ao `database_url` primary do workspace selecionado; as réplicas devem acompanhar o primary pelo mecanismo de replicação do próprio banco.

## IDs Snowflake

`snowflake()` exige um node ID vindo do ambiente:

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_replicas = []
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}
```

Cada processo concorrente deve usar um ID distinto; repetir o mesmo node ID pode quebrar a garantia de unicidade.
