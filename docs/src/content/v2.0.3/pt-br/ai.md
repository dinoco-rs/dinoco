# Usar o Dinoco com IA

A documentação do Dinoco está disponível para assistentes de IA de três formas, para que um assistente responda a partir das docs atuais em vez de adivinhar com base no treinamento: um **servidor MCP**, um índice **`llms.txt`** e Markdown puro que você pode colar num prompt.

## Servidor MCP

O site de docs expõe um servidor [Model Context Protocol](https://modelcontextprotocol.io) somente leitura:

```text
https://dinoco.io/mcp
```

Ele usa Streamable HTTP em modo JSON sem estado: cada requisição é uma mensagem JSON-RPC enviada com `POST` e não precisa de sessão nem login. Serve apenas a documentação pública, em inglês (`en-us`) e português (`pt-br`).

## Conectar um cliente

Claude Code:

```bash
claude mcp add --transport http dinoco-docs https://dinoco.io/mcp
```

Cursor, VS Code e outros clientes que leem um arquivo `mcp.json`:

```json
{
  "mcpServers": {
    "dinoco-docs": {
      "url": "https://dinoco.io/mcp"
    }
  }
}
```

No VS Code a chave de topo é `servers` e a entrada também precisa de `"type": "http"`. Qualquer cliente que fale Streamable HTTP funciona do mesmo jeito — aponte para a URL acima.

## Ferramentas disponíveis

| Ferramenta | Argumentos | O que retorna |
| --- | --- | --- |
| `search_docs` | `query`, opcionais `locale`, `group`, `limit` (1–25) | As páginas que melhor combinam, cada uma com id, URL e um trecho |
| `get_doc` | `id`, opcionais `locale`, `section` | Uma página em Markdown, ou só a seção sob um título |
| `list_docs` | opcionais `locale`, `group` | Todas as páginas agrupadas por área, com descrições |

O **id** de uma página é `group/item`, por exemplo `orm/find-many` ou `guide/writing-migrations`. `search_docs` e `list_docs` retornam ids, e `get_doc` também aceita a URL completa da página. Os grupos são `guide`, `orm`, `tooling` e `reference`. Use `locale: "pt-br"` para a versão em português.

Uma sessão típica de assistente busca primeiro e depois lê:

```text
search_docs  { "query": "where_complex count", "locale": "pt-br" }
get_doc      { "id": "orm/where-complex", "locale": "pt-br" }
```

Os links dentro das páginas retornadas são absolutos, então o assistente pode segui-los. Argumentos inválidos voltam como um erro de ferramenta que diz o que corrigir, para o modelo tentar de novo.

## Resources

Toda página também é um **resource** MCP com a URI `dinoco://docs/{locale}/{group}/{item}` e o tipo `text/markdown`. Clientes que permitem anexar resources — por exemplo, fixar o guia de migrations numa conversa — os listam com `resources/list`.

## llms.txt

Dois arquivos de texto seguem a convenção [llms.txt](https://llmstxt.org):

| URL | Conteúdo |
| --- | --- |
| `https://dinoco.io/llms.txt` | Um índice curto: uma linha por página, com descrição e link |
| `https://dinoco.io/llms-full.txt` | O Markdown completo de todas as páginas num só documento |

Ambos usam inglês por padrão; adicione `?locale=pt-br` para português. O `llms-full.txt` é grande, então prefira as ferramentas MCP quando o cliente suportar e use o arquivo para ferramentas que só aceitam uma URL.

## Dicas para prompts

- Cite o recurso como as docs o chamam (`where_complex`, `migration_engine`, `DinocoTransformer`), para a busca achar a página certa.
- Peça ao assistente para ler a página antes: "Leia `guide/writing-migrations` e escreva uma migration que adiciona uma coluna `email`."
- Diga qual versão você usa. As docs descrevem a versão mostrada no cabeçalho do site; um projeto mais antigo pode diferir.
- O código gerado em `dinoco/models/` é uma saída, não a fonte da verdade. Aponte o assistente para o `schema.dinoco` e, para customizações, para o `dinoco/transform.rs`.
