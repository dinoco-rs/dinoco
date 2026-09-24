# Use Dinoco with AI

Dinoco's documentation is available to AI assistants in three ways, so an assistant can answer from the current docs instead of guessing from training data: an **MCP server**, an **`llms.txt`** index, and plain Markdown you can paste into a prompt.

## MCP server

The docs site exposes a read-only [Model Context Protocol](https://modelcontextprotocol.io) server:

```text
https://dinoco.io/mcp
```

It uses Streamable HTTP in stateless JSON mode: every request is one JSON-RPC message sent with `POST`, and needs no session or login. It only serves the public documentation, in English (`en-us`) and Portuguese (`pt-br`).

## Connect a client

Claude Code:

```bash
claude mcp add --transport http dinoco-docs https://dinoco.io/mcp
```

Cursor, VS Code, and other clients that read an `mcp.json` file:

```json
{
  "mcpServers": {
    "dinoco-docs": {
      "url": "https://dinoco.io/mcp"
    }
  }
}
```

In VS Code the top-level key is `servers` and the entry also needs `"type": "http"`. Any client that speaks Streamable HTTP works the same way — point it at the URL above.

## Available tools

| Tool | Arguments | What it returns |
| --- | --- | --- |
| `search_docs` | `query`, optional `locale`, `group`, `limit` (1–25) | The best matching pages, each with an id, URL, and a snippet |
| `get_doc` | `id`, optional `locale`, `section` | One page as Markdown, or just the section under a heading |
| `list_docs` | optional `locale`, `group` | Every page grouped by area, with descriptions |

A page **id** is `group/item`, for example `orm/find-many` or `guide/writing-migrations`. `search_docs` and `list_docs` return ids, and `get_doc` also accepts a full page URL. The groups are `guide`, `orm`, `tooling`, and `reference`.

A typical assistant session searches first, then reads:

```text
search_docs  { "query": "where_complex count" }
get_doc      { "id": "orm/where-complex", "section": "Supported builders" }
```

Links inside returned pages are absolute, so an assistant can follow them. Bad arguments come back as a tool error that says what to fix, so the model can retry.

## Resources

Every page is also an MCP **resource** with the URI `dinoco://docs/{locale}/{group}/{item}` and the type `text/markdown`. Clients that let you attach resources — for example to pin the migrations guide to a conversation — list them with `resources/list`.

## llms.txt

Two plain-text files follow the [llms.txt](https://llmstxt.org) convention:

| URL | Content |
| --- | --- |
| `https://dinoco.io/llms.txt` | A short index: one line per page, with its description and link |
| `https://dinoco.io/llms-full.txt` | The full Markdown of every page in one document |

Both default to English; add `?locale=pt-br` for Portuguese. `llms-full.txt` is large, so prefer the MCP tools when your client supports them and use the file for tools that only take a URL.

## Tips for prompts

- Name the feature the way the docs do (`where_complex`, `migration_engine`, `DinocoTransformer`), so search finds the right page.
- Ask the assistant to read the page first: "Read `guide/writing-migrations` and write a migration that adds an `email` column."
- Say which version you use. The docs describe the version shown in the site header; an older project may differ.
- Generated code in `dinoco/models/` is an output, not a source of truth. Point the assistant at `schema.dinoco` and, for customizations, `dinoco/transform.rs`.
