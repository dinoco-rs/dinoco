# Extensão do VS Code

A extensão Dinoco transforma `.dinoco` numa experiência totalmente inteligente sobre a linguagem — não só texto colorido — e mantém os comandos de banco a um atalho de distância do schema sobre o qual eles operam. Ela exige um workspace local confiável, já que inicia um processo de language server de verdade e pode executar a CLI `dinoco` em seu nome.

## Abra um projeto Dinoco

Abra a raiz do projeto Cargo, depois rode **Dinoco: Open Schema** pela Command Palette. A extensão procura `dinoco/schema.dinoco` automaticamente, e ativa em qualquer arquivo `.dinoco`, independentemente de como você o abriu.

Se o projeto ainda não foi inicializado, rode **Dinoco: Initialize Project** em vez disso. Isso abre um task terminal interativo para que os prompts de banco e conexão fiquem totalmente visíveis e respondíveis, exatamente como se você tivesse rodado `dinoco init` você mesmo num terminal.

## Recursos da linguagem

O language server da v2.0.0 oferece:

- Syntax highlighting, mais semantic highlighting de verdade derivado do mesmo índice de símbolos usado por hover e completion — então o nome de um model é colorido de forma diferente dependendo se está sendo declarado ou referenciado, algo que uma gramática baseada em regex sozinha não consegue fazer de forma confiável.
- Diagnostics locais enquanto você digita, e uma compilação completa do projeto a cada save.
- Um workspace semântico com cache e tratamento seguro de grafos de import circulares.
- Completion para paths de import, para models/enums alcançáveis por um path de import válido, e para config keys, scalars, atributos, defaults, relation keys e ações referenciais.
- Hover com documentação em tipos, fields, atributos e config keys.
- Go to definition entre arquivos, find references e rename seguro — incluindo referências de field aninhadas dentro de atributos de model como `@relation(fields: [...])`.
- Document symbols (para a view Outline), folding ranges e selection ranges inteligentes.
- Quick fixes para configuração incompleta e para nomes de tipo que provavelmente são typo de um nome real.

Comandos de banco ficam tanto na Command Palette quanto no menu de contexto (botão direito) do editor de schema: gerar models, gerar uma migration e executar migrations pendentes.

## Formatação

A extensão chama exatamente a mesma crate de formatter que a CLI usa — não existe uma reimplementação separada para divergir com o tempo. Formate o schema ativo com **Dinoco: Format Schema**, ou ligue o format-on-save:

```json
{
  "[dinoco]": {
    "editor.defaultFormatter": "dinoco-rs.dinoco-vscode",
    "editor.formatOnSave": true
  }
}
```

Formatar é idempotente — rodar uma segunda vez seguida nunca muda nada. Declarações `@@...` de nível de model sempre são normalizadas para ficar depois de todos os fields, separadas por exatamente uma linha vazia.

O próprio formatter é configurável pelas settings do VS Code, não fixo a um único estilo:

```json
{
  "dinoco.formatter.maxWidth": 100,
  "dinoco.formatter.useTabs": false,
  "dinoco.formatter.useSpaces": true,
  "dinoco.formatter.indentSize": 4,
  "dinoco.formatter.removeComments": false
}
```

- `dinoco.formatter.maxWidth` — a largura de linha que o formatter tenta respeitar antes de quebrar uma lista de import longa ou um atributo com argumentos nomeados (como `@relation(...)`) em várias linhas.
- `dinoco.formatter.useTabs` / `dinoco.formatter.useSpaces` — o caractere de indentação. Os dois são mutuamente exclusivos: ativar um desativa o outro automaticamente, e a extensão mantém isso sincronizado mesmo se você editar o `settings.json` manualmente para um estado inconsistente.
- `dinoco.formatter.indentSize` — espaços por nível de indentação, ao usar spaces.
- `dinoco.formatter.removeComments` — remove todo comentário `#` e `//` ao formatar, em vez de preservá-los no lugar.

> [!NOTE]
> Os diagnostics pegam models sem primary key e models que declaram mais de uma. A mesma validação por trás disso confere nomes e tipos de field em atributos compostos, rejeita membros duplicados, e sinaliza um field tentando ser ao mesmo tempo um índice comum e full-text — tudo isso antes de você sequer rodar uma migration contra um banco de verdade.

Salvar qualquer arquivo alcançável a partir do schema raiz faz o language server recompilar o grafo de imports inteiro a partir de `dinoco/schema.dinoco`. Cada arquivo canônico é parseado exatamente uma vez por travessia — um import circular fecha uma aresta já visitada em vez de entrar em loop para sempre.

## Solução de problemas

Defina `dinoco.cli.path` quando o binário `dinoco` não estiver no `PATH` que o processo do VS Code de fato enxerga (comum no macOS quando o VS Code é aberto pelo Dock em vez de um terminal). Só defina `dinoco.server.path` para testar um binário customizado de language server — instalações empacotadas já vêm com o server certo para sua plataforma, e essa configuração ignora isso.

Use **Dinoco: Show Language Server Output** para ver logs de inicialização e do protocolo, e **Dinoco: Restart Language Server** depois de trocar por um binário de server customizado. `dinoco.trace.server` aceita `off`, `messages` ou `verbose` para diagnósticos mais profundos no nível do protocolo, quando algo não está se comportando como esperado.

> [!WARNING]
> Workspaces remotos e virtuais não são suportados. Arquivos gerados, comandos locais da CLI e prompts interativos de banco de dados todos assumem um filesystem de workspace real e local — não existe um caminho de execução remota para nenhum deles hoje.
