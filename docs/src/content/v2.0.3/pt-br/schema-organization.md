# Organização do schema

Um schema Dinoco não precisa viver em um único arquivo. Conforme o projeto cresce, dividir arquivos `.dinoco` por domínio (contas, cobrança, enums compartilhados) mantém cada arquivo revisável por si só. O `dinoco/schema.dinoco` raiz continua sendo o único ponto de entrada do projeto: é o único arquivo que pode declarar `config`, pode carregar arquivos filhos inteiros via `config.imports`, e pode anexar derives Rust extras no projeto inteiro. Todo outro arquivo mantém suas dependências explícitas através de declarações nomeadas `import { ... } from "..."`.

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

Somente `schema.dinoco` pode declarar `config`; models e enums, fora isso, podem viver no arquivo raiz ou em qualquer arquivo que ele alcance. `entities/` e `shared/` aqui são só nomes escolhidos por este exemplo — organize os arquivos filhos do jeito que fizer sentido para o seu domínio.

> [!WARNING]
> Não coloque arquivos-fonte `.dinoco` dentro de `dinoco/models/` ou `dinoco/migrations/`. `models/` é completamente substituído a cada geração de código, e `migrations/` é reservado para o histórico SQL gerenciado — qualquer coisa colocada ali corre o risco de ser sobrescrita silenciosamente ou interpretada errado como artefato de migration.

## Imports do arquivo principal

Use `config.imports` no arquivo raiz quando ele precisar pegar todo model e enum que um arquivo filho declara diretamente, sem listar cada um:

```dinoco
config {
    imports = [
        "entities/account.dinoco",
        "entities/business.dinoco",
        "shared/enums.dinoco"
    ]

    database     = "postgresql"
    database_url = env("DATABASE_URL")
}
```

Não é preciso listar símbolos, o que mantém o ponto de entrada pequeno mesmo quando o schema cresce para dezenas de declarações. `imports` é uma propriedade de nível de config: com workspaces, ela vive diretamente em `config`, nunca dentro de um bloco `workspace { ... }` individual.

`imports` precisa ser um array — pode estar vazio, mas cada item presente precisa ser um caminho string entre aspas e não vazio. Identificadores, números, booleanos, objetos, arrays aninhados e chamadas `env(...)` são todos rejeitados ali.

```dinoco
config {
    imports = ["entities/account.dinoco"]

    workspace {
        dev {
            database     = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database     = "postgresql"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

`config.imports` só existe no `schema.dinoco` raiz — um arquivo filho não pode declarar seu próprio bloco `config` de jeito nenhum.

## Imports nomeados

Arquivos filhos usam um import nomeado quando precisam de um model ou enum declarado em outro lugar:

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

Todo símbolo nomeado precisa estar declarado diretamente no arquivo de destino (reexportações transitivas não acontecem implicitamente — veja [Escopo de cada arquivo](#escopo-de-cada-arquivo)). Vários símbolos são separados por vírgula, e uma vírgula final é aceita. O arquivo raiz também pode usar imports nomeados; `config.imports` costuma ser só mais conveniente ali quando ele quer tudo de um arquivo.

## Escopo de cada arquivo

Cada arquivo tem seu próprio escopo de tipos, independente — nada fica globalmente visível só porque *algum* arquivo do projeto o importa:

| Arquivo | Declarações visíveis |
| --- | --- |
| `schema.dinoco` raiz | Suas próprias declarações, seus imports nomeados, e toda declaração direta de todo arquivo em `config.imports` |
| Um arquivo filho `.dinoco` | Suas próprias declarações, mais somente os símbolos nomeados em seus próprios imports |
| Um arquivo importado por um filho | Não é reexportado automaticamente para quem importa esse filho, nem para o arquivo raiz |

Por exemplo: se `entities/business.dinoco` importa `BusinessStatus`, esse enum fica visível *dentro* de `business.dinoco`. Um model declarado diretamente em `schema.dinoco` só pode usar `BusinessStatus` se `shared/enums.dinoco` *também* estiver listado no `config.imports` da raiz, ou for importado nominalmente no próprio arquivo raiz — importá-lo em `business.dinoco` não repassa isso adiante.

> [!NOTE]
> Isso é proposital, não uma limitação para contornar. O compiler ainda consolida a árvore completa de imports alcançável para validação, migrations e geração de código — nada realmente quebra entre arquivos. O que o escopo por arquivo garante é que um arquivo nunca compila por acidente só porque outro arquivo, sem relação nenhuma, importou o tipo que faltava.

## Validação dos imports

As duas formas de import — `config.imports` e o `import { ... }` nomeado — seguem as mesmas regras de caminho:

- Os caminhos são relativos ao arquivo que declara o import.
- Os caminhos precisam ser relativos e terminar em `.dinoco` — sem caminhos absolutos, sem importar um arquivo que não seja `.dinoco`.
- Segmentos `.` e `..` são normalizados antes da detecção de duplicidade, então `./account.dinoco` e `account.dinoco` são reconhecidos como o mesmo import.
- Um arquivo ausente é erro de compilação, reportado na declaração de import.
- Imports circulares são totalmente suportados: cada arquivo é parseado e consolidado exatamente uma vez, então `Account.sessions` e `Session.account` podem viver em arquivos separados que se importam mutuamente sem recursão infinita.
- Importar o mesmo arquivo resolvido duas vezes a partir do mesmo arquivo é rejeitado.
- Símbolos duplicados, símbolos nomeados inexistentes e conflitos de nome com declarações locais são todos rejeitados.
- Os diagnósticos apontam para o arquivo e a linha de origem sempre que o compiler consegue determinar isso, não só o ponto de entrada.

A CLI sempre inicia a compilação em `dinoco/schema.dinoco`. A API do compiler que recebe só uma string (usada por tooling e testes) rejeita qualquer schema que use imports, já que ela não tem uma localização no filesystem para resolver caminhos relativos.

## Saída Rust gerada

Os enums e a lista de `mod` dos models ficam em `dinoco/models/mod.rs`; cada model ganha o próprio arquivo em `dinoco/models/`. Os arquivos gerados são completamente substituídos a cada regeneração, então nunca os edite à mão — a edição seria descartada no próximo `migrate generate` ou `models generate`.

Para adicionar derives, atributos, imports ou métodos ao que é gerado, use os [code transforms](/pt-br/docs/orm/guide/transforms): um `dinoco/transform.rs` aplicado a cada geração. O `dinoco/mod.rs` gerado também começa com `#![allow(unused)]`, que só suprime warnings de código não usado dentro daquele módulo e dos arquivos que ele inclui.

## Exemplo completo

O `dinoco/schema.dinoco` raiz fica focado na configuração global do projeto:

```dinoco
config {
    imports = ["entities/account.dinoco", "shared/enums.dinoco"]

    database     = "sqlite"
    database_url = env("DATABASE_URL")
}
```

`dinoco/entities/account.dinoco` declara exatamente do que depende:

```dinoco
import { AccountType } from "../shared/enums.dinoco"

model Account {
    id           String      @id @default(uuid())
    email        String      @unique
    account_type AccountType @default(owner)
}
```

`dinoco/shared/enums.dinoco` contém o enum em si:

```dinoco
enum AccountType {
    owner
    member
}
```

Rode `dinoco models generate` (ou o fluxo normal de migration) depois de alterar qualquer arquivo alcançável a partir da raiz — o Dinoco sempre recompila a árvore inteira, não só o arquivo que você tocou.
