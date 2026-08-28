# Organização do schema

Schemas Dinoco podem ser divididos em vários arquivos `.dinoco`. O `dinoco/schema.dinoco` continua sendo o ponto de entrada do projeto: ele contém `config`, carrega arquivos completos com `config.imports` e pode configurar derives Rust adicionais. Os arquivos filhos mantêm suas dependências explícitas com declarações nomeadas `import { ... } from "..."`.

## Estrutura de projeto recomendada

```text
dinoco/
  schema.dinoco
  entities/
    account.dinoco
    business.dinoco
  shared/
    enums.dinoco
```

Somente `schema.dinoco` pode declarar `config`. Models e enums podem ser declarados no arquivo principal ou em qualquer arquivo filho alcançável.

`entities/` e `shared/` são diretórios de código-fonte escolhidos pela aplicação. Não armazene arquivos-fonte `.dinoco` em `dinoco/models/` nem em `dinoco/migrations/`: `models/` é substituído pelo codegen, enquanto `migrations/` é reservado para o histórico gerenciado de migrations SQL.

## Imports do arquivo principal

Use `config.imports` no arquivo principal quando ele precisar carregar todos os models e enums declarados diretamente em outro arquivo:

```dinoco
config {
    imports = [
        "entities/account.dinoco",
        "entities/business.dinoco",
        "shared/enums.dinoco"
    ]

    database = "postgresql"
    database_url = env("DATABASE_URL")
}
```

Não é necessário listar símbolos. Assim, o ponto de entrada permanece pequeno mesmo com muitas declarações. `imports` é uma propriedade global do projeto; ao usar `workspace`, declare-a diretamente dentro de `config`, nunca dentro de um workspace individual:

O valor de `imports` deve ser um array. O array pode estar vazio, mas cada item presente deve ser um caminho string entre aspas e não vazio. Identificadores, números, booleanos, objetos, arrays aninhados e valores `env(...)` são rejeitados.

```dinoco
config {
    imports = ["entities/account.dinoco"]

    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database = "postgresql"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

`config.imports` está disponível somente no `schema.dinoco` principal. Um arquivo filho não pode declarar seu próprio bloco `config`.

## Imports nomeados

Arquivos filhos usam imports nomeados quando referenciam models ou enums declarados em outro arquivo:

```dinoco
import { AccountType, BusinessStatus } from "../shared/enums.dinoco"

model Account {
    id           String      @id @default(uuid())
    account_type AccountType
}

model Business {
    id     String         @id @default(uuid())
    status BusinessStatus
}
```

Cada símbolo nomeado deve estar declarado diretamente no arquivo de destino. Separe vários símbolos por vírgulas; uma vírgula final também é aceita. Imports nomeados também funcionam no arquivo principal, embora `config.imports` normalmente seja mais conciso nele.

## Escopo de cada arquivo

Cada arquivo possui um escopo de tipos independente:

| Arquivo | Declarações visíveis |
| --- | --- |
| `schema.dinoco` principal | Suas declarações, imports nomeados e todas as declarações diretas dos arquivos em `config.imports` |
| Arquivo filho `.dinoco` | Suas declarações e somente os símbolos presentes em seus imports nomeados |
| Arquivo importado por um filho | Não é reexportado automaticamente para o parent do filho nem para o arquivo principal |

Por exemplo, se `entities/business.dinoco` importar `BusinessStatus`, esse enum fica visível dentro de `business.dinoco`. Um model declarado em `schema.dinoco` só pode usar o enum quando `shared/enums.dinoco` também estiver em `config.imports`, ou quando o enum for importado nominalmente no arquivo principal.

O compiler ainda consolida toda a árvore de imports alcançável para validação, migrations e codegen. Os escopos isolados impedem que um arquivo compile apenas porque outro arquivo não relacionado importou o tipo ausente.

## Validação dos imports

As duas formas de import seguem as mesmas regras de caminho:

- caminhos são relativos ao arquivo que declara o import;
- caminhos devem ser relativos e terminar em `.dinoco`;
- segmentos `.` e `..` são normalizados antes da detecção de duplicidade;
- arquivos ausentes geram erro de compilação;
- imports circulares são permitidos: cada arquivo é parseado e consolidado uma única vez, inclusive quando models em arquivos diferentes se relacionam entre si;
- importar o mesmo arquivo resolvido duas vezes em um arquivo é rejeitado;
- símbolos duplicados, símbolos nomeados inexistentes e conflitos com declarações locais são rejeitados;
- sempre que possível, os diagnósticos indicam o arquivo e a linha de origem.

A CLI inicia a compilação em `dinoco/schema.dinoco`. A API de compilação que recebe apenas uma string rejeita imports porque não possui um caminho-base para resolver os arquivos.

## Custom derives

Use `config.custom_derives` para adicionar macros derive a todos os enums gerados ou a todos os structs de model gerados:

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

Assim como `imports`, `custom_derives` é global. Ele fica diretamente no bloco `config` principal e fora dos blocos de workspace individuais.

## Campos de custom derive

Cada item de `custom_derives` é um objeto com três propriedades string obrigatórias:

| Propriedade | Valor aceito | Efeito |
| --- | --- | --- |
| `into` | `"enum"` ou `"struct"` | Seleciona todos os enums gerados ou todos os structs de model gerados |
| `derive` | Um caminho Rust como `ZodSchema` ou `crate::ZodSchema` | Adiciona a macro ao `#[derive(...)]` gerado |
| `import` | Uma única declaração Rust `use ...`, em uma linha | Adiciona o import da macro ao módulo Rust gerado |

As três chaves são obrigatórias em cada objeto. Um `{}` vazio, um objeto com apenas uma ou duas chaves ou um objeto com valor vazio/não-string é rejeitado; o Dinoco nunca aplica um custom derive parcialmente especificado. Propriedades desconhecidas ou repetidas, caminhos Rust inválidos e imports que não sejam uma declaração `use` também são rejeitados durante a compilação do schema.

A crate que fornece o custom derive deve estar nas dependências da aplicação. O Dinoco não instala essa crate nem verifica se todos os fields gerados implementam os traits exigidos pela macro. Como cada alvo é global, use um derive somente quando ele for válido para todos os enums gerados ou para todos os models gerados.

## Saída Rust gerada

Imports e derives de enum são emitidos em `dinoco/models/mod.rs`. Imports e derives de struct são emitidos em cada arquivo de model gerado. Declarações de import repetidas aparecem uma única vez por alvo, e derives com o mesmo segmento final do caminho Rust são deduplicados, inclusive derives já fornecidos pelo Dinoco, como `Clone` ou `Debug`.

Por exemplo, uma configuração de enum com `derive = "ZodSchema"` produz uma saída equivalente a:

```rust
use zod_rs::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, ZodSchema)]
pub enum BusinessStatus {
    Active,
    Inactive,
}
```

Os arquivos gerados são substituídos quando os models são gerados novamente. Por isso, configure os derives no schema em vez de editar o Rust gerado. O `dinoco/mod.rs` gerado também começa com `#![allow(dead_code)]`, evitando warnings para helpers gerados que ainda não são utilizados.

## Exemplo completo

O `dinoco/schema.dinoco` principal fica focado nas configurações globais do projeto:

```dinoco
config {
    imports = ["entities/account.dinoco", "shared/enums.dinoco"]

    database = "sqlite"
    database_url = env("DATABASE_URL")

    custom_derives = [
        {
            into   = "enum"
            derive = "ZodSchema"
            import = "use zod_rs::prelude::*;"
        }
    ]
}
```

`dinoco/entities/account.dinoco` declara sua dependência explicitamente:

```dinoco
import { AccountType } from "../shared/enums.dinoco"

model Account {
    id           String      @id @default(uuid())
    email        String      @unique
    account_type AccountType @default(OWNER)
}
```

`dinoco/shared/enums.dinoco` contém o enum:

```dinoco
enum AccountType {
    OWNER
    MEMBER
}
```

Execute `dinoco models generate` ou o fluxo normal de migrations depois de alterar qualquer arquivo da árvore de imports.
