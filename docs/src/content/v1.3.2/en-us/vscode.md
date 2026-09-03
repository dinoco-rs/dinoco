# VS Code extension

The Dinoco extension turns `.dinoco` files into a language-aware editor and keeps database commands close to the schema. It requires a local, trusted workspace because it starts a language server and can execute the CLI.

## Open a Dinoco project

Open the Cargo project root, then run **Dinoco: Open Schema** from the Command Palette. The extension looks for `dinoco/schema.dinoco` and activates automatically for any `.dinoco` file.

If the project has not been initialized, run **Dinoco: Initialize Project**. The command opens an interactive task terminal so database and connection questions remain visible.

## Language features

The v1.3.2 language server provides:

- syntax and semantic highlighting;
- local diagnostics while editing and complete project compilation on save;
- a cached semantic workspace with safe support for circular import graphs;
- completion for import paths and for models/enums declared by a valid path, plus config keys, scalars, attributes, defaults, relation keys, and referential actions;
- hover documentation;
- cross-file go to definition, references, and safe rename, including field references inside model attributes;
- document symbols, folding ranges, and selection ranges;
- quick fixes for incomplete config and close type-name mistakes.

Database commands are also available from the Command Palette and the schema editor context menu: generate models, generate a migration, and run pending migrations.

## Formatting

The extension uses the same formatter crate as the CLI. Format the current schema with **Dinoco: Format Schema** or enable format on save:

```json
{
  "[dinoco]": {
    "editor.defaultFormatter": "dinoco-rs.dinoco-vscode",
    "editor.formatOnSave": true
  }
}
```

Because formatter output is idempotent, a second format pass produces no changes. Model-level `@@...` declarations are normalized after every field, with one blank line separating the two sections.

Schema diagnostics report models without a primary key and models that declare more than one key. The same parser validates composite attribute field names, types, duplicate members, and standard/full-text conflicts before a migration can run.

Saving any file reachable from the main schema makes the LSP compile the graph from `dinoco/schema.dinoco`. Each canonical file is read once per traversal; cycles close an already visited edge instead of restarting the load.

## Troubleshooting

Set `dinoco.cli.path` when `dinoco` is not on VS Code's process `PATH`. Set `dinoco.server.path` only to test a custom language-server binary; packaged builds use their bundled server.

Use **Dinoco: Show Language Server Output** for startup and protocol logs, and **Dinoco: Restart Language Server** after changing a custom server binary. `dinoco.trace.server` accepts `off`, `messages`, or `verbose` for deeper protocol diagnostics.

Remote and virtual workspaces are not supported in v1.3.2 because generated files, local commands, and database prompts require a real workspace filesystem.
