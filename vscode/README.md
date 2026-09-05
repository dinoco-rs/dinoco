# Dinoco for Visual Studio Code

Dinoco provides first-class editing and database workflows for `.dinoco` schemas.

## Language intelligence

- Live local diagnostics plus complete project validation on save
- Circular-safe, cached indexing of every reachable imported schema
- Context-aware completion for import paths, imported models/enums, scalar types, defaults, relations, local keys, referenced keys, and referential actions
- Hover documentation for types, fields, attributes, generators, configuration, and relation behavior
- Cross-file go to definition, find references, safe rename, symbol outline, folding, and smart selection ranges
- Semantic highlighting that reflects each identifier's real role (type declaration vs. reference, field, enum member, attribute) on top of the base TextMate grammar
- Formatting powered by the same formatter used by the Dinoco toolchain, and configurable from VS Code settings
- Quick fixes for incomplete configuration and close type-name typos

## Database workflows

Use the Command Palette or the schema editor context menu to:

- Initialize a Dinoco project
- Generate Rust models
- Generate and review a migration
- Run pending migrations

Commands run in an interactive VS Code task terminal, preserving migration warnings and confirmation prompts.

## Configuration

- `dinoco.cli.path`: path to the Dinoco CLI executable
- `dinoco.server.path`: optional custom language server binary
- `dinoco.models.generateOnSave`: generate models after saving a schema
- `dinoco.trace.server`: language-server protocol tracing

### Formatter configuration

The formatter is AST-based (it parses the schema and re-emits it, rather than
rewriting text), deterministic, and idempotent: formatting a file twice always
produces the same output. It's configurable from VS Code settings:

```json
{
    "dinoco.formatter.maxWidth": 100,
    "dinoco.formatter.useTabs": false,
    "dinoco.formatter.useSpaces": true,
    "dinoco.formatter.indentSize": 4,
    "dinoco.formatter.removeComments": false
}
```

- `dinoco.formatter.maxWidth`: maximum line width before the formatter breaks
  supported expressions (long import lists, attributes with named arguments
  such as `@relation(...)`) into multiple lines.
- `dinoco.formatter.useTabs` / `dinoco.formatter.useSpaces`: choose the
  indentation character. **These are mutually exclusive** — enabling one
  automatically disables the other, and the extension keeps them in sync if
  you edit `settings.json` by hand. If both ever end up `true` at once, tabs
  win.
- `dinoco.formatter.indentSize`: number of spaces per indentation level when
  using spaces. Has no effect when `useTabs` is enabled (one tab per level).
- `dinoco.formatter.removeComments`: remove all `#` and `//` comments from a
  schema when formatting it, instead of preserving them.

Set Dinoco as the default formatter for `.dinoco` files and enable format on
save with:

```json
{
    "[dinoco]": {
        "editor.defaultFormatter": "dinoco-rs.dinoco-vscode",
        "editor.formatOnSave": true
    }
}
```

(these are already the extension's activation defaults for the language).

The extension supports PostgreSQL, MySQL, and SQLite schemas.

## Learn more

Full documentation for the Dinoco ORM, its schema language, CLI, and query
API lives at [docs.dinoco.io](https://docs.dinoco.io).
