# VS Code extension

The Dinoco extension turns `.dinoco` into a fully language-aware experience — not just colored text — and keeps database commands one keystroke away from the schema they operate on. It requires a local, trusted workspace, since it starts a real language server process and can execute the `dinoco` CLI on your behalf.

## Open a Dinoco project

Open the Cargo project root, then run **Dinoco: Open Schema** from the Command Palette. The extension looks for `dinoco/schema.dinoco` automatically, and activates on any `.dinoco` file regardless of how you opened it.

If the project hasn't been initialized yet, run **Dinoco: Initialize Project** instead. It opens an interactive task terminal so the database and connection prompts stay fully visible and answerable, exactly as if you'd run `dinoco init` yourself in a terminal.

## Language features

The v2.0.0 language server provides:

- Syntax highlighting, plus real semantic highlighting derived from the same symbol index used for hover and completion — so a model name is colored differently depending on whether it's being declared or referenced, something a regex-based grammar alone can't do reliably.
- Local diagnostics while you type, and a complete project compilation on every save.
- A cached semantic workspace with safe handling of circular import graphs.
- Completion for import paths, for models/enums reachable through a valid import path, and for config keys, scalars, attributes, defaults, relation keys, and referential actions.
- Hover documentation on types, fields, attributes, and config keys.
- Cross-file go to definition, find references, and safe rename — including field references nested inside model attributes like `@relation(fields: [...])`.
- Document symbols (for the Outline view), folding ranges, and smart selection ranges.
- Quick fixes for incomplete configuration and for type names that are probably typos of a real one.

Database commands sit in both the Command Palette and the schema editor's right-click context menu: generate models, generate a migration, and run pending migrations.

## Formatting

The extension calls the exact same formatter crate the CLI does — there's no separate reimplementation to drift out of sync. Format the active schema with **Dinoco: Format Schema**, or turn on format-on-save:

```json
{
  "[dinoco]": {
    "editor.defaultFormatter": "dinoco-rs.dinoco-vscode",
    "editor.formatOnSave": true
  }
}
```

Formatting is idempotent — running it a second time in a row never changes anything. Model-level `@@...` declarations are always normalized to sit after every field, separated by exactly one blank line.

The formatter itself is configurable from VS Code settings, not fixed to one style:

```json
{
  "dinoco.formatter.maxWidth": 100,
  "dinoco.formatter.useTabs": false,
  "dinoco.formatter.useSpaces": true,
  "dinoco.formatter.indentSize": 4,
  "dinoco.formatter.removeComments": false
}
```

- `dinoco.formatter.maxWidth` — the line width the formatter targets before breaking a long import list or an attribute with named arguments (like `@relation(...)`) across multiple lines.
- `dinoco.formatter.useTabs` / `dinoco.formatter.useSpaces` — the indentation character. These two are mutually exclusive: enabling one disables the other automatically, and the extension keeps them in sync even if you hand-edit `settings.json` into an inconsistent state.
- `dinoco.formatter.indentSize` — spaces per indentation level, when using spaces.
- `dinoco.formatter.removeComments` — strip every `#` and `//` comment during formatting instead of preserving them in place.

> [!NOTE]
> Diagnostics catch models with no primary key and models that declare more than one. The same underlying validation checks composite attribute field names and types, rejects duplicate members, and flags a field trying to be both a standard and a full-text index — all before you ever get to running a migration against a real database.

Saving any file reachable from the root schema makes the language server recompile the whole import graph starting from `dinoco/schema.dinoco`. Each canonical file is parsed exactly once per traversal — a circular import closes an already-visited edge instead of looping forever.

## Troubleshooting

Set `dinoco.cli.path` when the `dinoco` binary isn't on the `PATH` VS Code's process actually sees (common on macOS when VS Code is launched from the Dock rather than a terminal). Only set `dinoco.server.path` to test a custom language server binary — packaged installs already bundle the right server for your platform, and this setting bypasses that.

Use **Dinoco: Show Language Server Output** to see startup and protocol logs, and **Dinoco: Restart Language Server** after swapping in a custom server binary. `dinoco.trace.server` accepts `off`, `messages`, or `verbose` for deeper protocol-level diagnostics when something's not behaving as expected.

> [!WARNING]
> Remote and virtual workspaces aren't supported. Generated files, local CLI commands, and interactive database prompts all assume a real, local workspace filesystem — there's no remote-execution path for any of them today.
