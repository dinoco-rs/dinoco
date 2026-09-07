# Schema organization

A Dinoco schema doesn't have to live in a single file. As a project grows, splitting `.dinoco` files by domain (accounts, billing, shared enums) keeps each file reviewable on its own. The root `dinoco/schema.dinoco` stays the project's single entry point: it's the only file allowed to declare `config`, it can pull in whole child files through `config.imports`, and it can attach extra Rust derives project-wide. Every other file keeps its own dependencies explicit through named `import { ... } from "..."` statements.

## Recommended project layout

```text
dinoco/
  schema.dinoco
  entities/
    account.dinoco
    business.dinoco
  shared/
    enums.dinoco
```

Only `schema.dinoco` may declare `config`; models and enums can otherwise live in the root file or in any file it can reach. `entities/` and `shared/` here are just names this example picked — organize child files however makes sense for your domain.

> [!WARNING]
> Don't put `.dinoco` source files inside `dinoco/models/` or `dinoco/migrations/`. `models/` is fully replaced on every code generation, and `migrations/` is reserved for the managed SQL history — anything you put there risks being silently overwritten or misread as a migration artifact.

## Root file imports

Use `config.imports` in the root file when it should pick up every model and enum a child file declares directly, without spelling each one out:

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

No symbol list is required, which keeps the entry point small even as the schema grows to dozens of declarations. `imports` is a config-level property: with workspaces, it lives directly inside `config`, never inside an individual `workspace { ... }` block.

`imports` must be an array — it can be empty, but every entry it does contain has to be a quoted, non-empty string path. Identifiers, numbers, booleans, objects, nested arrays, and `env(...)` calls are all rejected there.

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

`config.imports` only exists in the root `schema.dinoco` — a child file can't declare its own `config` block at all.

## Named imports

Child files reach for a named import when they need a model or enum declared somewhere else:

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

Every named symbol must be declared directly in the target file (transitive re-exports don't happen implicitly — see [File scope](#file-scope)). Multiple symbols are comma-separated, and a trailing comma is fine. The root file can use named imports too; `config.imports` is just usually more convenient there when it wants everything from a file.

## File scope

Each file has its own, independent type scope — nothing is globally visible just because *some* file in the project imports it:

| File | Visible declarations |
| --- | --- |
| Root `schema.dinoco` | Its own declarations, its own named imports, and every direct declaration from every `config.imports` file |
| A child `.dinoco` file | Its own declarations, plus only the symbols named in its own imports |
| A file imported by a child | Not automatically re-exported to that child's importers, or to the root file |

For example: if `entities/business.dinoco` imports `BusinessStatus`, that enum becomes visible *inside* `business.dinoco`. A model declared directly in `schema.dinoco` can only use `BusinessStatus` if `shared/enums.dinoco` is *also* listed in the root's `config.imports`, or imported by name in the root file itself — importing it into `business.dinoco` doesn't pass it along.

> [!NOTE]
> This is deliberate, not a limitation to work around. The compiler still consolidates the full reachable import tree for validation, migrations, and code generation — nothing actually breaks across files. What per-file scoping buys you is that a file can never compile only by accident, because some unrelated file happened to import the type it was missing.

## Import validation

Both import forms — `config.imports` and named `import { ... }` — follow the same path rules:

- Paths are relative to the file that declares the import.
- Paths must stay relative and end in `.dinoco` — no absolute paths, no importing a non-`.dinoco` file.
- `.` and `..` segments are normalized before duplicate detection, so `./account.dinoco` and `account.dinoco` are recognized as the same import.
- A missing file is a compile error, reported at the import statement.
- Circular imports are fully supported: each file is parsed and consolidated exactly once, so `Account.sessions` and `Session.account` can live in separate files that import each other without infinite recursion.
- Importing the same resolved file twice from the same file is rejected.
- Duplicate symbols, unknown named symbols, and name conflicts with local declarations are all rejected.
- Diagnostics point at the originating file and line whenever the compiler can determine it, not just the entry point.

The CLI always starts compilation from `dinoco/schema.dinoco`. The compiler's string-only API (used for tooling and tests) rejects any schema that uses imports, since it has no filesystem location to resolve relative paths against.

## Custom derives

`config.custom_derives` attaches derive macros to every generated enum, or every generated model struct, project-wide:

```dinoco
config {
    database     = "sqlite"
    database_url = env("DATABASE_URL")

    custom_derives = [
        {
            into   = "enum"
            derive = "ZodSchema"
            import = "use zod_rs::prelude::*;"
        },
        {
            into   = "struct"
            derive = "Validate"
            import = "use validator::Validate;"
        }
    ]
}
```

Like `imports`, `custom_derives` is config-level: it lives directly inside the root `config` block, never inside a workspace.

## Custom derive fields

Every entry in `custom_derives` is an object with three required string properties:

| Property | Accepted value | Effect |
| --- | --- | --- |
| `into` | `"enum"` or `"struct"` | Targets every generated enum, or every generated model struct |
| `derive` | A Rust derive path, e.g. `ZodSchema` or `crate::ZodSchema` | Appended to the generated `#[derive(...)]` |
| `import` | One single-line Rust `use ...;` statement | Added to the generated module so the derive path resolves |

> [!WARNING]
> All three keys are mandatory on every entry. `{}`, an object missing a key, or one with an empty or non-string value is rejected outright — Dinoco never applies a partially specified custom derive. Unknown or duplicated properties, an invalid Rust path, or an `import` that isn't a `use` statement are rejected the same way, at compile time.

The crate providing the derive is still something *you* add to your application's `Cargo.toml` — Dinoco wires in the attribute and the `use` statement, but doesn't install dependencies or verify that every generated field satisfies whatever the macro requires. Because each entry applies globally, only reach for a custom derive when it's genuinely valid for *every* generated enum or *every* generated model — there's no per-model opt-out.

## Generated Rust output

Enum imports and derives land in `dinoco/models/mod.rs`; struct imports and derives land in every individual generated model file. A repeated `import` is only ever emitted once per target file, and derives that share a final path segment are deduplicated — including against derives Dinoco already adds itself, like `Clone` or `Debug`.

For example, an enum entry with `derive = "ZodSchema"` produces something equivalent to:

```rust
use zod_rs::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, ZodSchema)]
pub enum BusinessStatus {
    Active,
    Inactive,
}
```

Generated files are fully replaced on every regeneration, so configure derives in the schema — never by hand-editing the generated Rust, which would just be discarded on the next `migrate generate`/`models generate`. The generated `dinoco/mod.rs` also starts with `#![allow(unused)]`, which only suppresses unused-code warnings within that module and the files it includes.

## Complete example

The root `dinoco/schema.dinoco` stays focused on project-wide configuration:

```dinoco
config {
    imports = ["entities/account.dinoco", "shared/enums.dinoco"]

    database     = "sqlite"
    database_url = env("DATABASE_URL")

    custom_derives = [
        {
            into   = "enum"
            derive = "ZodSchema"
            import = "use zod_rs::prelude::*;"
        }
    ]
}
```

`dinoco/entities/account.dinoco` declares exactly what it depends on:

```dinoco
import { AccountType } from "../shared/enums.dinoco"

model Account {
    id           String      @id @default(uuid())
    email        String      @unique
    account_type AccountType @default(owner)
}
```

`dinoco/shared/enums.dinoco` owns the enum itself:

```dinoco
enum AccountType {
    owner
    member
}
```

Run `dinoco models generate` (or the regular migration workflow) after changing any file reachable from the root — Dinoco always recompiles the whole tree, not just the file you touched.
