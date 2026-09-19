# Transform API reference

Everything below is exported by `dinoco_codegen::prelude::*`. A transformer receives a structured description of the code Dinoco is about to write, mutates it, and Dinoco renders the result.

## DinocoTransformer

```rust
pub trait DinocoTransformer {
    fn transform_schema(&self, _schema: &mut Schema) {}
    fn transform_enum(&self, _item: &mut Enum) {}
    fn transform_variant(&self, _item: &Enum, _variant: &mut Variant) {}
    fn transform_model(&self, _model: &mut Model) {}
    fn transform_field(&self, _model: &Model, _field: &mut Field) {}
    fn transform_relation(&self, _model: &Model, _relation: &mut Relation) {}
}
```

Every method has an empty default; implement only what you need. Hooks run in this order:

1. `transform_schema`, once, with every enum and model.
2. For each enum: `transform_enum`, then `transform_variant` for each variant.
3. For each model: `transform_model`, then `transform_field` for each scalar/enum field, then `transform_relation` for each relation.

`transform_variant`, `transform_field`, and `transform_relation` receive their parent as a read-only **snapshot** taken after the parent's own hook ran. Changes you make to a field are not visible in the snapshot handed to the next field. A field you removed inside `transform_model` is never visited.

Two helpers wrap a closure into a transformer: `transform_models(|model| ...)` and `transform_enums(|item| ...)`.

## Schema

| Member | Type | Description |
| --- | --- | --- |
| `enums` | `Vec<Enum>` | Every generated enum |
| `models` | `Vec<Model>` | Every generated struct |
| `imports` | `Vec<String>` | `use` statements added to **every** generated file |
| `add_import(path)` | method | Adds an import |
| `model(name)` / `model_mut(name)` | method | Looks a model up by schema name |
| `enum_(name)` / `enum_mut(name)` | method | Looks an enum up by schema name |

## Model (struct)

| Member | Type | Description |
| --- | --- | --- |
| `name` | `String` | The Rust struct name |
| `table_name` | `String` | The database table |
| `derives` | `Vec<String>` | Derive paths, e.g. `Debug`, `::dinoco::serde::Serialize` |
| `attributes` | `Vec<String>` | Type attributes, one full attribute per entry |
| `imports` | `Vec<String>` | `use` statements added to this model's file |
| `fields` | `Vec<Field>` | Scalar, enum, and many-to-many key fields |
| `relations` | `Vec<Relation>` | Fields that point to another model |
| `impls` | `Vec<ImplBlock>` | `impl` blocks rendered after the struct |

| Method | Description |
| --- | --- |
| `add_derive(path)` | Adds a derive unless one with the same last path segment exists |
| `remove_derive(name)` / `has_derive(name)` | Matches on the last path segment |
| `add_attribute(attr)` | Adds a full attribute such as `#[serde(rename_all = "camelCase")]` (no duplicates) |
| `add_import(path)` | Adds a `use`; `std::fmt`, `use std::fmt`, and `use std::fmt;` are the same import |
| `add_impl_method(method)` | Adds a method to the model's inherent `impl` block (created on first use) |
| `add_trait_impl(trait_path, items)` | Adds `impl <trait_path> for Model { <items> }`; `items` is the raw text of the body |
| `add_impl(block)` | Adds a complete `ImplBlock` |
| `field(name)` / `field_mut(name)` | Finds a field |
| `relation(name)` / `relation_mut(name)` | Finds a relation |

## Enum

`Enum` has `name`, `derives`, `attributes`, `imports`, `impls`, and `variants: Vec<Variant>`, plus the same `add_derive`, `remove_derive`, `has_derive`, `add_attribute`, `add_import`, `add_impl_method`, `add_trait_impl`, and `add_impl` methods as `Model`.

## Variant

| Member | Description |
| --- | --- |
| `name` | The Rust variant name (PascalCase) |
| `value` | The value stored in the database |
| `attributes` | Attributes rendered above the variant |
| `add_attribute(attr)` | Adds an attribute |

## Field

| Member | Description |
| --- | --- |
| `name` | The field name |
| `ty` | `RustType`, the type **without** the `Option<...>` wrapper |
| `nullable` | When `true`, rendered as `Option<ty>` |
| `attributes` | Attributes rendered above the field |
| `add_attribute(attr)` | Adds an attribute (no duplicates) |
| `has_attribute(prefix)` | `true` if an attribute starts with `prefix` |
| `remove_attributes(prefix)` | Removes attributes starting with `prefix` |

Scalar list fields keep their `Vec<...>` inside `ty`. `RustType::new("::std::sync::Arc<str>")` builds a type; `&str` and `String` convert with `.into()`.

## Relation

| Member | Description |
| --- | --- |
| `name` | The field name |
| `target` | The schema name of the model it points to |
| `list` | Rendered as `Vec<ty>` |
| `nullable` | Rendered as `Option<ty>` (ignored when `list` is set) |
| `ty` | The related type (`Box<Self>` for an optional self-relation) |
| `attributes` | Attributes rendered above the field |

## ImplBlock

```rust
pub struct ImplBlock {
    pub trait_path: Option<String>,   // None: `impl Type`, Some: `impl Trait for Type`
    pub attributes: Vec<String>,      // e.g. #[allow(clippy::...)]
    pub items: Vec<String>,           // raw associated items: `type Error = ..;`, `const`, whole methods
    pub methods: Vec<ImplMethod>,
}
```

Build one with `ImplBlock::inherent()` or `ImplBlock::for_trait("std::fmt::Display")`, and chain `.item(text)` / `.method(method)`. Raw `items` render before `methods`.

## ImplMethod

```rust
pub struct ImplMethod {
    pub name: String,
    pub visibility: Visibility,      // Public (default) | Crate | Private
    pub is_async: bool,
    pub receiver: Receiver,          // Ref (default) | RefMut | Owned | None
    pub params: Vec<(String, String)>, // (name, type), without the receiver
    pub return_type: String,         // empty means ()
    pub body: String,                // without the outer braces
    pub attributes: Vec<String>,
    pub docs: Vec<String>,
}
```

`ImplMethod::new(name, return_type, body)` sets the three essentials and leaves the rest at their defaults; use struct-update syntax (`..ImplMethod::default()`) for the others. `body` accepts anything that implements `ToString`, so both a string and a `quote! { ... }` token stream work. A token stream is rendered on one token-spaced line (`self . deleted_at . is_none ()`); it is valid Rust, and `rustfmt` will tidy it.

Use `Visibility::Private` for methods inside a trait impl, where visibility qualifiers are not allowed.

## Imports

Dinoco does not resolve or check imports; it writes what you give it, once per file, sorted. `add_import` on a model or enum goes into that file only; `Schema::add_import` goes into every generated file. Enum imports land in `dinoco/models/mod.rs`, model imports in the model's own file. The crate that provides an imported item is still yours to add to `Cargo.toml`.

## Functions

| Function | Description |
| --- | --- |
| `apply_transformer(&mut schema, &transformer)` | Runs a transformer over a `Schema` |
| `build_schema(&compiler_schema)` | Builds the untransformed `Schema` from a compiled `schema.dinoco` |
| `generate_models_with(&schema, workspace, &transformer)` | Writes `dinoco/` with a transformer applied |
| `render_model_source(&model, &imports)` / `render_models_mod_from(&schema)` | Render the files without writing them — useful in tests |
