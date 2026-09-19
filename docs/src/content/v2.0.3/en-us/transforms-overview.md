# Code transforms overview

Dinoco generates your models, but the generated Rust is rarely the last word: you want `serde` attributes on timestamp fields, a `Validate` derive, an `is_active()` helper, a `Display` impl. **Code transforms** let you change what gets generated — derives, attributes, imports, methods, trait impls — from a Rust file in your project, without editing generated code that would be overwritten on the next run.

> [!WARNING]
> Transforms replace `config.custom_derives`, which was **removed**. A schema that still declares `custom_derives` now fails to compile with a message pointing here. See [Migrate from custom_derives](/en-us/docs/orm/guide/transforms-recipes#migrate-from-custom-derives).

## How it works

1. `dinoco models generate` (or `dinoco migrate generate`) compiles `schema.dinoco`.
2. Dinoco builds an in-memory description of the Rust it is about to write: every enum and struct, with its derives, attributes, imports, fields, relations, and `impl` blocks.
3. If `dinoco/transform.rs` exists, the CLI builds and runs it. Your `transformer()` receives that description **mutably**, one piece at a time, and changes it.
4. Dinoco renders the result to `dinoco/models/`.

There is no template language and nothing to register: a transform is ordinary Rust that edits plain data (`model.derives`, `field.attributes`, `model.impls`, ...). What you add is written into the generated file as text, so anything you can write in Rust you can generate.

## The transform file

Create `dinoco/transform.rs`. It must expose one function, `transformer()`, returning something that implements `DinocoTransformer`:

```rust
use dinoco_codegen::prelude::*;

struct MyTransformer;

impl DinocoTransformer for MyTransformer {
    fn transform_model(&self, model: &mut Model) {
        model.add_derive("Hash");
    }
}

pub fn transformer() -> impl DinocoTransformer {
    MyTransformer
}
```

Then generate as usual:

```bash
dinoco models generate      # or: dinoco migrate generate
```

Whenever `dinoco/transform.rs` exists, both commands apply it. Delete the file and the next generation is back to the plain output. `dinoco_codegen::prelude::*` brings in everything used on this page: `DinocoTransformer`, `Schema`, `Model`, `Enum`, `Field`, `Variant`, `Relation`, `ImplBlock`, `ImplMethod`, `Visibility`, `Receiver`, `RustType`, and the `quote!` macro.

## The hooks

You implement only the hooks you need; the rest do nothing.

| Hook | Runs for | Receives |
| --- | --- | --- |
| `transform_schema` | Once | The whole `Schema` (all models and enums) |
| `transform_model` | Every struct | `&mut Model` |
| `transform_field` | Every scalar/enum field | The parent `&Model` (read-only) and `&mut Field` |
| `transform_relation` | Every field pointing to another model | The parent `&Model` and `&mut Relation` |
| `transform_enum` | Every enum | `&mut Enum` |
| `transform_variant` | Every enum variant | The parent `&Enum` and `&mut Variant` |

## Imports

An **import** is a Rust `use` line written at the top of a generated file, so the derives, traits, and types you reference resolve. Dinoco does not guess what your transform needs: whatever you use in a derive, attribute, method, or impl has to be imported — with `add_import`, on the item that uses it.

```rust
fn transform_model(&self, model: &mut Model) {
    model.add_import("use lib::Trait;");     // written exactly as a `use` statement
    model.add_import("lib::Trait");          // same import: `use` and `;` are optional
    model.add_import("lib::{Trait, Other}"); // groups work too
    model.add_import("std::fmt as std_fmt"); // so do renames
}
```

The generated `dinoco/models/user.rs` then starts with:

```rust
#[allow(unused_imports)]
use super::*;
use dinoco::Entity;
use lib::Trait;
use lib::{Trait, Other};
use std::fmt as std_fmt;
```

Rules of thumb:

- The three spellings above are the **same** import: `use lib::Trait`, `use lib::Trait;`, and `lib::Trait` are normalized and written once.
- `add_import` on a `Model` goes into **that model's file**. On an `Enum` it goes into `dinoco/models/mod.rs`, where the enums live.
- `schema.add_import(...)` (in `transform_schema`) goes into **every** generated file.
- Imports are sorted and de-duplicated per file.
- The crate that provides the import (`lib` above) is a dependency of *your application* — add it to your `Cargo.toml`. Dinoco writes the `use` line and does not check that it resolves.
- Paths are resolved from the generated file, which sits in `dinoco/models/`. Use `crate::...` for your own code, or an absolute crate path.

## Derives

```rust
fn transform_model(&self, model: &mut Model) {
    model.add_import("validator::Validate");
    model.add_derive("Hash");
    model.add_derive("validator::Validate");
}
```

```rust
#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize, Hash, validator::Validate)]
pub struct User { /* ... */ }
```

A derive whose last path segment already exists is skipped — deriving `Clone` or `Serialize` twice would not compile — so `model.add_derive("serde::Serialize")` is a harmless no-op. `model.remove_derive("Clone")` and `model.has_derive("Clone")` exist too. The same methods work on `Enum`.

## Methods

Add a method with `add_impl_method`. All methods you add go into one inherent `impl` block, in order:

```rust
fn transform_model(&self, model: &mut Model) {
    // Shortest form: name, return type, body.
    model.add_impl_method(ImplMethod::new(
        "is_active",
        "bool",
        quote! { self.deleted_at.is_none() },
    ));

    // Full control: visibility, receiver, parameters, async, attributes, docs.
    model.add_impl_method(ImplMethod {
        name: "with_name".into(),
        visibility: Visibility::Public,
        receiver: Receiver::Owned,                     // self
        params: vec![("name".into(), "String".into())],
        return_type: "Self".into(),
        body: "Self { name, ..self }".into(),
        attributes: vec!["#[must_use]".into()],
        docs: vec!["Returns a copy with a new name.".into()],
        ..ImplMethod::default()
    });

    // An associated function (no `self`).
    model.add_impl_method(ImplMethod {
        name: "table".into(),
        receiver: Receiver::None,
        return_type: "&'static str".into(),
        body: format!("\"{}\"", model.table_name),
        ..ImplMethod::default()
    });
}
```

```rust
impl User {
    pub fn is_active(&self) -> bool {
        self . deleted_at . is_none ()
    }

    /// Returns a copy with a new name.
    #[must_use]
    pub fn with_name(self, name: String) -> Self {
        Self { name, ..self }
    }

    pub fn table() -> &'static str {
        "user"
    }
}
```

`body` is the text between the braces. It takes anything that implements `ToString`: a string, a `format!`, or a `quote! { ... }` token stream. A `quote!` body is written on one token-spaced line (`self . deleted_at . is_none ()`), which is valid Rust that `rustfmt` tidies — use a plain string when you want the layout preserved.

## Traits

`add_trait_impl(trait_path, items)` writes `impl <trait_path> for Model { <items> }`, where `items` is the raw text of the body. **Import the trait you implement**, or write its full path:

```rust
fn transform_model(&self, model: &mut Model) {
    // 1. A trait from std, imported.
    model.add_import("std::fmt");
    model.add_trait_impl(
        "fmt::Display",
        r#"fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "User({})", self.id)
}"#,
    );

    // 2. A trait from your own crate or a dependency.
    model.add_import("lib::Trait");
    model.add_trait_impl("Trait", "fn label(&self) -> String {\n    self.id.to_string()\n}");

    // 3. A marker trait with nothing to implement.
    model.add_import("lib::Marker");
    model.add_trait_impl("Marker", "");
}
```

```rust
impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "User({})", self.id)
    }
}

impl Trait for User {
    fn label(&self) -> String {
        self.id.to_string()
    }
}
```

When the trait has associated types or constants, or when you want to build the methods with `ImplMethod`, use an `ImplBlock`. In a trait impl, methods take `Visibility::Private` (a `pub` qualifier is not allowed there):

```rust
fn transform_model(&self, model: &mut Model) {
    model.add_import("lib::Repository");

    model.add_impl(
        ImplBlock::for_trait("Repository")
            .item("type Id = String;")                 // raw associated item
            .item("const KIND: &'static str = \"user\";")
            .method(ImplMethod {
                name: "id".into(),
                visibility: Visibility::Private,       // required inside a trait impl
                return_type: "Self::Id".into(),
                body: "self.id.clone()".into(),
                ..ImplMethod::default()
            }),
    );
}
```

```rust
impl Repository for User {
    type Id = String;

    const KIND: &'static str = "user";

    fn id(&self) -> Self::Id {
        self.id.clone()
    }
}
```

Generic traits take their arguments in the path: `ImplBlock::for_trait("From<UserId>")`, `add_trait_impl("TryFrom<&str>", "...")`.

## Field attributes

`transform_field` sees every scalar and enum field. Attributes are full attribute lines, appended after the ones Dinoco writes:

```rust
fn transform_field(&self, model: &Model, field: &mut Field) {
    if field.name.ends_with("_at") {
        field.add_attribute(r#"#[serde(with = "chrono::serde::ts_seconds")]"#);
    }

    if model.name == "User" && field.name == "password_hash" {
        field.add_attribute("#[serde(skip_serializing)]");
    }
}
```

```rust
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,
```

Type-level attributes go through `model.add_attribute(...)` (`#[serde(rename_all = "camelCase")]`). You can also change a field's type (`field.ty = RustType::new("crate::Metadata")`) or nullability (`field.nullable = true`), and give relations attributes in `transform_relation`.

## Enums

Everything above works for enums too, and `transform_variant` reaches each variant:

```rust
fn transform_enum(&self, item: &mut Enum) {
    item.add_import("strum::Display");
    item.add_derive("Hash");
    item.add_derive("strum::Display");

    let arms = item
        .variants
        .iter()
        .map(|variant| format!("Self::{} => \"{}\",", variant.name, variant.value))
        .collect::<String>();
    item.add_impl_method(ImplMethod::new("label", "&'static str", format!("match self {{ {arms} }}")));
}

fn transform_variant(&self, item: &Enum, variant: &mut Variant) {
    if item.name == "Status" && variant.value == "legacy" {
        variant.add_attribute("#[deprecated]");
    }
}
```

## Everything at once

A single transformer can combine all of it, and can branch on the model's name or its fields:

```rust
use dinoco_codegen::prelude::*;

struct Project;

impl DinocoTransformer for Project {
    fn transform_schema(&self, schema: &mut Schema) {
        schema.add_import("crate::traits::Auditable"); // every generated file
    }

    fn transform_model(&self, model: &mut Model) {
        model.add_derive("Hash");

        if model.field("deleted_at").is_some() {
            model.add_impl_method(ImplMethod::new("is_active", "bool", "self.deleted_at.is_none()"));
        }

        model.add_impl(
            ImplBlock::for_trait("Auditable").method(ImplMethod {
                name: "audit_id".into(),
                visibility: Visibility::Private,
                return_type: "String".into(),
                body: "self.id.to_string()".into(),
                ..ImplMethod::default()
            }),
        );
    }

    fn transform_field(&self, _model: &Model, field: &mut Field) {
        if field.name.ends_with("_at") {
            field.add_attribute(r#"#[serde(with = "chrono::serde::ts_seconds_option")]"#);
        }
    }
}

pub fn transformer() -> impl DinocoTransformer {
    Project
}
```

Running it on `model User { id String @id  deleted_at DateTime? }` produces:

```rust
#[allow(unused_imports)]
use super::*;
use dinoco::Entity;
use crate::traits::Auditable;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize, Hash)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "user")]
pub struct User {
    #[dinoco(primary_key)]
    pub id: String,

    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub deleted_at: Option<::dinoco::chrono::DateTime<::dinoco::chrono::Utc>>,

}

impl User {
    pub fn is_active(&self) -> bool {
        self.deleted_at.is_none()
    }
}

impl Auditable for User {
    fn audit_id(&self) -> String {
        self.id.to_string()
    }
}
```

More copyable examples are in the [recipes](/en-us/docs/orm/guide/transforms-recipes), and every type and method is in the [API reference](/en-us/docs/orm/guide/transforms-api).

## Only a closure

If you only need to touch every model or every enum, skip the trait:

```rust
use dinoco_codegen::prelude::*;

pub fn transformer() -> impl DinocoTransformer {
    transform_models(|model| {
        model.add_derive("Hash");
    })
}
```

`transform_enums(|item| { ... })` does the same for enums.

## Where the transform runs

The transform is compiled and run by the CLI, not by your application. It is a separate program, so `transform.rs` can use `dinoco_codegen` (through `dinoco_codegen::prelude::*`), `quote!`, and `std` — but **not** your application's own modules or crates. That is why the types and traits you *mention* in generated code are only text: they need to exist in your application, not in `transform.rs`. It never becomes part of your build: `dinoco/transform.rs` is not declared as a module anywhere, and your editor will treat it as a standalone file.

Like [manual migrations](/en-us/docs/orm/guide/migration-workflow#how-migrations-are-executed), it needs a Rust toolchain wherever `dinoco models generate` runs, builds through a generated project in `dinoco/.runner/` (self-ignored by Git), and prints Cargo's errors if `transform.rs` does not compile. A failing transform stops the generation; Dinoco never falls back to untransformed output.

## Generated output is still generated

Generated files are fully rewritten on every run, so put customizations in `transform.rs` — never in `dinoco/models/`. Because the transformer runs on every generation, its result is reproducible, reviewable in Git, and identical on every machine.

Dinoco's own attributes and derives are part of the model you receive (`#[derive(Debug, Clone, Entity, ...)]`, `#[dinoco(...)]`, `#[serde(crate = ...)]`). You can inspect them, and removing one is allowed but can stop the generated code from compiling — that is your call. Dinoco does not compile what you generate: a typo in a body, an attribute, or an import shows up the next time you build your application, in the generated file.
