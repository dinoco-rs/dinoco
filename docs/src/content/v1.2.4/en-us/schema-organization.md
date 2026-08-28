# Schema organization

Dinoco schemas can be split across multiple `.dinoco` files. The main `dinoco/schema.dinoco` remains the project entrypoint: it owns `config`, loads complete schema files through `config.imports`, and can configure extra Rust derives. Child files keep their dependencies explicit with named `import { ... } from "..."` statements.

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

Only `schema.dinoco` may declare `config`. Models and enums may be declared in the main file or any reachable child file.

`entities/` and `shared/` are source directories chosen by the application. Do not store `.dinoco` source files in `dinoco/models/` or `dinoco/migrations/`: `models/` is replaced by code generation, while `migrations/` is reserved for the managed SQL migration history.

## Root file imports

Use `config.imports` in the main file when it should load every model and enum declared directly by another file:

```dinoco
config {
    imports = [
        "entities/account.dinoco",
        "entities/business.dinoco",
        "shared/enums.dinoco"
    ]

    database = "postgresql"
    database_url = env("DATABASE_URL")
}
```

No symbol list is required. This keeps the entrypoint small even when the schema contains many declarations. `imports` is a global project property; when `workspace` is used, declare it directly inside `config`, not inside an individual workspace:

The value of `imports` must be an array. The array may be empty, but every item it contains must be a quoted, non-empty string path. Identifiers, numbers, booleans, objects, nested arrays, and `env(...)` values are rejected.

```dinoco
config {
    imports = ["entities/account.dinoco"]

    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database = "postgresql"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

`config.imports` is available only in the main `schema.dinoco`. A child file cannot declare its own `config` block.

## Named imports

Child files use named imports when they refer to models or enums declared elsewhere:

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

Each named symbol must be declared directly in the target file. Multiple symbols are separated by commas, and a trailing comma is accepted. Named imports also work in the main file, although `config.imports` is usually more concise there.

## File scope

Every file has an independent type scope:

| File                       | Visible declarations                                                                          |
| -------------------------- | --------------------------------------------------------------------------------------------- |
| Main `schema.dinoco`       | Its own declarations, named imports, and every direct declaration from `config.imports` files |
| Child `.dinoco` file       | Its own declarations and only the symbols in its named imports                                |
| A file imported by a child | Not automatically re-exported to the child's parent or to the main file                       |

For example, if `entities/business.dinoco` imports `BusinessStatus`, that enum is visible inside `business.dinoco`. A model declared in `schema.dinoco` can use the enum only when `shared/enums.dinoco` is also listed in the main `config.imports`, or the enum is imported by name in the main file.

The compiler still consolidates the complete reachable import tree for validation, migrations, and code generation. Isolated scopes prevent one file from compiling only because an unrelated file happened to import the missing type.

## Import validation

Both import forms follow the same path rules:

- paths are relative to the file that declares the import;
- paths must be relative and end in `.dinoco`;
- `.` and `..` segments are normalized before duplicate detection;
- missing files and circular imports are compilation errors;
- importing the same resolved file twice from one file is rejected;
- duplicate symbols, unknown named symbols, and conflicts with local declarations are rejected;
- diagnostics point to the originating file and line whenever possible.

The CLI starts compilation from `dinoco/schema.dinoco`. Passing import syntax to the string-only compiler API is rejected because it has no filesystem base from which to resolve paths.

## Custom derives

Use `config.custom_derives` to add derive macros to every generated enum or every generated model struct:

```dinoco
config {
    database = "sqlite"
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

Like `imports`, `custom_derives` is global. It belongs directly inside the main `config` block and remains outside individual workspace blocks.

## Custom derive fields

Every item in `custom_derives` is an object with three required string properties:

| Property | Accepted value                                               | Effect                                                     |
| -------- | ------------------------------------------------------------ | ---------------------------------------------------------- |
| `into`   | `"enum"` or `"struct"`                                       | Selects all generated enums or all generated model structs |
| `derive` | A Rust derive path such as `ZodSchema` or `crate::ZodSchema` | Appends the macro to the generated `#[derive(...)]`        |
| `import` | One single-line Rust `use ...` statement                     | Adds the macro import to the generated Rust module         |

All three keys are mandatory in every object. An empty `{}`, an object with only one or two keys, or an object with an empty/non-string value is rejected; Dinoco never applies a partially specified custom derive. Unknown or repeated properties, invalid Rust paths, and non-`use` import statements are also rejected while compiling the schema.

The crate that provides a custom derive must be added to the application dependencies. Dinoco does not install it or verify that every generated field implements the traits required by the macro. Because each target applies globally, use a derive only when it is valid for every generated enum or every generated model.

## Generated Rust output

Enum imports and derives are emitted in `dinoco/models/mod.rs`. Struct imports and derives are emitted in every generated model file. Repeated import statements are emitted once per target, and derives with the same final Rust path segment are deduplicated, including derives already provided by Dinoco such as `Clone` or `Debug`.

For example, an enum configuration with `derive = "ZodSchema"` produces output equivalent to:

```rust
use zod_rs::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, ZodSchema)]
pub enum BusinessStatus {
    Active,
    Inactive,
}
```

Generated files are replaced when models are regenerated, so configure derives in the schema instead of editing generated Rust. The generated `dinoco/mod.rs` also starts with `#![allow(dead_code)]`, preventing unused generated helpers from producing warnings.

## Complete example

The main `dinoco/schema.dinoco` stays focused on project-wide configuration:

```dinoco
config {
    imports = ["entities/account.dinoco", "shared/enums.dinoco"]

    database = "sqlite"
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

`dinoco/entities/account.dinoco` declares its dependency explicitly:

```dinoco
import { AccountType } from "../shared/enums.dinoco"

model Account {
    id           String      @id @default(uuid())
    email        String      @unique
    account_type AccountType @default(OWNER)
}
```

`dinoco/shared/enums.dinoco` owns the enum:

```dinoco
enum AccountType {
    OWNER
    MEMBER
}
```

Run `dinoco models generate` or the normal migration workflow after changing any file in the import tree.
