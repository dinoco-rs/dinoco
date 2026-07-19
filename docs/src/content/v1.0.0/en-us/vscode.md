# VS Code extension

The Dinoco extension turns `.dinoco` files into a language-aware editor and keeps database commands close to the schema. It requires a local, trusted workspace because it starts a language server and can execute the CLI.

## Open a Dinoco project

Open the Cargo project root, then run **Dinoco: Open Schema** from the Command Palette. The extension looks for `dinoco/schema.dinoco` and activates automatically for any `.dinoco` file.

If the project has not been initialized, run **Dinoco: Initialize Project**. The command opens an interactive task terminal so database and connection questions remain visible.

## Language features

The v1.0.0 language server provides:

- syntax and semantic highlighting;
- live parser and schema diagnostics;
- context-aware completion for config keys, scalars, models, enums, defaults, relation keys, and referential actions;
- hover documentation;
- go to definition, references, and safe rename;
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

Because formatter output is idempotent, a second format pass produces no changes.

## Troubleshooting

Set `dinoco.cli.path` when `dinoco` is not on VS Code's process `PATH`. Set `dinoco.server.path` only to test a custom language-server binary; packaged builds use their bundled server.

Use **Dinoco: Show Language Server Output** for startup and protocol logs, and **Dinoco: Restart Language Server** after changing a custom server binary. `dinoco.trace.server` accepts `off`, `messages`, or `verbose` for deeper protocol diagnostics.

Remote and virtual workspaces are not supported in v1.0.0 because generated files, local commands, and database prompts require a real workspace filesystem.
