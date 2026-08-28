# Dinoco for Visual Studio Code

Dinoco provides first-class editing and database workflows for `.dinoco` schemas.

## Language intelligence

- Live local diagnostics plus complete project validation on save
- Circular-safe, cached indexing of every reachable imported schema
- Context-aware completion for import paths, imported models/enums, scalar types, defaults, relations, local keys, referenced keys, and referential actions
- Hover documentation for types, fields, attributes, generators, configuration, and relation behavior
- Cross-file go to definition, find references, safe rename, symbol outline, folding, and smart selection ranges
- Formatting powered by the same formatter used by the Dinoco toolchain
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

The extension supports PostgreSQL, MySQL, and SQLite schemas.
