# Extensão do VS Code

A extensão Dinoco transforma `.dinoco` em uma linguagem inteligente e aproxima os comandos do banco do schema. Ela exige um workspace local confiável porque inicia um language server e pode executar a CLI.

## Abra um projeto Dinoco

Abra a raiz do projeto Cargo e rode **Dinoco: Open Schema** pela Command Palette. A extensão procura `dinoco/schema.dinoco` e também ativa automaticamente em qualquer arquivo `.dinoco`.

Para um projeto novo, use **Dinoco: Initialize Project**. Os prompts aparecem em um task terminal interativo.

## Recursos da linguagem

A v1.3.2 oferece:

- syntax e semantic highlight;
- diagnostics locais durante a edição e compilação completa do projeto ao salvar;
- workspace semântico com cache e suporte seguro a grafos de imports circulares;
- completion para paths de import e para os models/enums declarados por um path válido, além de config, scalars, atributos, defaults, relation keys e ações referenciais;
- hover com documentação;
- go to definition, references e rename seguro entre arquivos importados, inclusive para referências de fields em atributos de model;
- symbols, folding e selection ranges;
- quick fixes para config incompleta e nomes de tipos próximos.

Os comandos de models e migrations também estão na Command Palette e no menu de contexto do schema.

## Formatação

O editor usa o mesmo formatter da toolchain. Rode **Dinoco: Format Schema** ou habilite format on save:

```json
{
  "[dinoco]": {
    "editor.defaultFormatter": "dinoco-rs.dinoco-vscode",
    "editor.formatOnSave": true
  }
}
```

O formatter é idempotente: formatar de novo não deve alterar o resultado. Declarações `@@...` são normalizadas depois de todos os fields, com uma linha vazia entre as seções.

Os diagnostics apontam models sem primary key e models com mais de uma chave. O mesmo parser valida nomes, tipos e duplicações nos grupos compostos, além de conflitos entre índices comuns e full-text, antes de uma migration.

Ao salvar qualquer arquivo alcançável pelo schema principal, o LSP recompila o grafo a partir de `dinoco/schema.dinoco`. Cada arquivo canônico é lido uma única vez por travessia; ciclos fecham uma aresta já visitada em vez de reiniciar o carregamento.

## Solução de problemas

Defina `dinoco.cli.path` se o executável não estiver no `PATH` visto pelo VS Code. `dinoco.server.path` serve para testar um server customizado; builds empacotados usam o server bundled.

Use **Dinoco: Show Language Server Output** para logs e **Dinoco: Restart Language Server** após trocar um binário. `dinoco.trace.server` aceita `off`, `messages` e `verbose`.
