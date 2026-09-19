//! Structured view of the Rust code Dinoco generates, plus the
//! [`DinocoTransformer`] hook used to customize it from `dinoco/transform.rs`.
//!
//! Codegen first builds a [`Schema`] (enums and structs, with their derives,
//! attributes, fields and impl blocks), lets the transformer mutate it, and only
//! then renders it to Rust source.

use std::collections::BTreeSet;
use std::fmt::{self, Display};

/// A Rust type written as source text, e.g. `String` or `::dinoco::Uuid`.
///
/// It never includes the `Option<..>`/`Vec<..>` wrapper: see
/// [`Field::nullable`] and [`Relation::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustType(pub String);

impl RustType {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RustType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for RustType {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for RustType {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Public,
    Crate,
    Private,
}

impl Visibility {
    fn prefix(self) -> &'static str {
        match self {
            Visibility::Public => "pub ",
            Visibility::Crate => "pub(crate) ",
            Visibility::Private => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Receiver {
    /// `&self`
    #[default]
    Ref,
    /// `&mut self`
    RefMut,
    /// `self`
    Owned,
    /// An associated function without a receiver.
    None,
}

/// A method inside an [`ImplBlock`].
#[derive(Debug, Clone, Default)]
pub struct ImplMethod {
    pub name: String,
    pub visibility: Visibility,
    pub is_async: bool,
    pub receiver: Receiver,
    /// `(name, type)` pairs, without the receiver.
    pub params: Vec<(String, String)>,
    /// Empty means `()`.
    pub return_type: String,
    /// The method body, without the surrounding braces. Anything that
    /// implements `ToString` works, including a `quote!` token stream.
    pub body: String,
    /// Full attribute lines such as `#[inline]`.
    pub attributes: Vec<String>,
    pub docs: Vec<String>,
}

impl ImplMethod {
    pub fn new(name: impl Into<String>, return_type: impl Into<String>, body: impl ToString) -> Self {
        Self { name: name.into(), return_type: return_type.into(), body: body.to_string(), ..Self::default() }
    }

    fn render(&self, out: &mut String) {
        for doc in &self.docs {
            out.push_str(&format!("    /// {doc}\n"));
        }
        for attribute in &self.attributes {
            out.push_str(&format!("    {attribute}\n"));
        }

        let mut params = Vec::new();
        match self.receiver {
            Receiver::Ref => params.push("&self".to_string()),
            Receiver::RefMut => params.push("&mut self".to_string()),
            Receiver::Owned => params.push("self".to_string()),
            Receiver::None => {}
        }
        params.extend(self.params.iter().map(|(name, ty)| format!("{name}: {ty}")));

        let asyncness = if self.is_async { "async " } else { "" };
        let return_type =
            if self.return_type.trim().is_empty() { String::new() } else { format!(" -> {}", self.return_type.trim()) };
        out.push_str(&format!(
            "    {}{asyncness}fn {}({}){return_type} {{\n",
            self.visibility.prefix(),
            self.name,
            params.join(", ")
        ));
        for line in self.body.trim().lines() {
            if line.trim().is_empty() {
                out.push('\n');
            } else {
                out.push_str(&format!("        {}\n", line.trim_end()));
            }
        }
        out.push_str("    }\n");
    }
}

/// An `impl` block attached to a generated type.
#[derive(Debug, Clone, Default)]
pub struct ImplBlock {
    /// `None` for an inherent `impl Type`, otherwise the trait path of
    /// `impl Trait for Type`.
    pub trait_path: Option<String>,
    pub attributes: Vec<String>,
    /// Raw associated items (`type Error = ..;`, `const X: u8 = 1;`, or whole
    /// methods) rendered before [`ImplBlock::methods`].
    pub items: Vec<String>,
    pub methods: Vec<ImplMethod>,
}

impl ImplBlock {
    pub fn inherent() -> Self {
        Self::default()
    }

    pub fn for_trait(trait_path: impl Into<String>) -> Self {
        Self { trait_path: Some(trait_path.into()), ..Self::default() }
    }

    pub fn item(mut self, item: impl ToString) -> Self {
        self.items.push(item.to_string());
        self
    }

    pub fn method(mut self, method: ImplMethod) -> Self {
        self.methods.push(method);
        self
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.methods.is_empty() && self.trait_path.is_none()
    }

    fn render(&self, type_name: &str, out: &mut String) {
        if self.is_empty() {
            return;
        }

        out.push('\n');
        for attribute in &self.attributes {
            out.push_str(attribute);
            out.push('\n');
        }
        match &self.trait_path {
            Some(path) => out.push_str(&format!("impl {path} for {type_name} {{\n")),
            None => out.push_str(&format!("impl {type_name} {{\n")),
        }
        let mut wrote = false;
        for item in &self.items {
            if wrote {
                out.push('\n');
            }
            for line in item.trim().lines() {
                if line.trim().is_empty() {
                    out.push('\n');
                } else {
                    out.push_str(&format!("    {}\n", line.trim_end()));
                }
            }
            wrote = true;
        }
        for method in &self.methods {
            if wrote {
                out.push('\n');
            }
            method.render(out);
            wrote = true;
        }
        out.push_str("}\n");
    }
}

/// A scalar, enum or many-to-many key field of a generated struct.
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: RustType,
    /// Rendered as `Option<ty>`.
    pub nullable: bool,
    /// Full attribute lines such as `#[serde(skip)]`.
    pub attributes: Vec<String>,
}

impl Field {
    pub fn add_attribute(&mut self, attribute: impl Into<String>) {
        push_unique(&mut self.attributes, attribute.into());
    }

    pub fn has_attribute(&self, prefix: &str) -> bool {
        self.attributes.iter().any(|attribute| attribute.starts_with(prefix))
    }

    pub fn remove_attributes(&mut self, prefix: &str) {
        self.attributes.retain(|attribute| !attribute.starts_with(prefix));
    }

    fn rendered_type(&self) -> String {
        if self.nullable { format!("Option<{}>", self.ty) } else { self.ty.to_string() }
    }
}

/// A field that points to another model.
#[derive(Debug, Clone)]
pub struct Relation {
    pub name: String,
    /// The schema name of the model this relation points to.
    pub target: String,
    /// Rendered as `Vec<ty>`.
    pub list: bool,
    /// Rendered as `Option<ty>` (ignored when `list` is set).
    pub nullable: bool,
    pub ty: RustType,
    pub attributes: Vec<String>,
}

impl Relation {
    pub fn add_attribute(&mut self, attribute: impl Into<String>) {
        push_unique(&mut self.attributes, attribute.into());
    }

    pub fn has_attribute(&self, prefix: &str) -> bool {
        self.attributes.iter().any(|attribute| attribute.starts_with(prefix))
    }

    fn rendered_type(&self) -> String {
        if self.list {
            format!("Vec<{}>", self.ty)
        } else if self.nullable {
            format!("Option<{}>", self.ty)
        } else {
            self.ty.to_string()
        }
    }
}

/// A generated `struct`.
#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub table_name: String,
    /// Derive paths, e.g. `Debug` or `::dinoco::serde::Serialize`.
    pub derives: Vec<String>,
    /// Full attribute lines placed after the derive, e.g. `#[serde(rename_all = "camelCase")]`.
    pub attributes: Vec<String>,
    /// `use` statements added to this model's file.
    pub imports: Vec<String>,
    pub fields: Vec<Field>,
    pub relations: Vec<Relation>,
    pub impls: Vec<ImplBlock>,
}

impl Model {
    /// Adds a derive unless one with the same final path segment already exists.
    pub fn add_derive(&mut self, path: impl Into<String>) {
        push_derive(&mut self.derives, path.into());
    }

    pub fn remove_derive(&mut self, name: &str) {
        self.derives.retain(|derive| short_name(derive) != short_name(name));
    }

    pub fn has_derive(&self, name: &str) -> bool {
        self.derives.iter().any(|derive| short_name(derive) == short_name(name))
    }

    pub fn add_attribute(&mut self, attribute: impl Into<String>) {
        push_unique(&mut self.attributes, attribute.into());
    }

    /// Adds `use ...;`. Accepts the statement with or without `use`/`;`.
    pub fn add_import(&mut self, import: impl AsRef<str>) {
        push_unique(&mut self.imports, normalize_import(import.as_ref()));
    }

    pub fn add_impl(&mut self, block: ImplBlock) {
        self.impls.push(block);
    }

    /// Adds a method to the model's inherent `impl` block.
    pub fn add_impl_method(&mut self, method: ImplMethod) {
        inherent_block(&mut self.impls).methods.push(method);
    }

    /// Adds `impl <trait_path> for Model { <items> }` where `items` is the raw
    /// text between the braces.
    pub fn add_trait_impl(&mut self, trait_path: impl Into<String>, items: impl ToString) {
        self.impls.push(ImplBlock::for_trait(trait_path).item(items));
    }

    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.name == name)
    }

    pub fn field_mut(&mut self, name: &str) -> Option<&mut Field> {
        self.fields.iter_mut().find(|field| field.name == name)
    }

    pub fn relation(&self, name: &str) -> Option<&Relation> {
        self.relations.iter().find(|relation| relation.name == name)
    }

    pub fn relation_mut(&mut self, name: &str) -> Option<&mut Relation> {
        self.relations.iter_mut().find(|relation| relation.name == name)
    }

    pub(crate) fn render(&self, out: &mut String) {
        out.push_str(&format!("#[derive({})]\n", self.derives.join(", ")));
        for attribute in &self.attributes {
            out.push_str(attribute);
            out.push('\n');
        }
        out.push_str(&format!("pub struct {} {{\n", self.name));
        for field in &self.fields {
            for attribute in &field.attributes {
                out.push_str(&format!("    {attribute}\n"));
            }
            out.push_str(&format!("    pub {}: {},\n\n", field.name, field.rendered_type()));
        }
        for relation in &self.relations {
            for attribute in &relation.attributes {
                out.push_str(&format!("    {attribute}\n"));
            }
            out.push_str(&format!("    pub {}: {},\n\n", relation.name, relation.rendered_type()));
        }
        out.push_str("}\n");
        for block in &self.impls {
            block.render(&self.name, out);
        }
    }
}

/// One variant of a generated `enum`.
#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    /// The value stored in the database.
    pub value: String,
    pub attributes: Vec<String>,
}

impl Variant {
    pub fn add_attribute(&mut self, attribute: impl Into<String>) {
        push_unique(&mut self.attributes, attribute.into());
    }
}

/// A generated `enum`.
#[derive(Debug, Clone)]
pub struct Enum {
    pub name: String,
    pub derives: Vec<String>,
    pub attributes: Vec<String>,
    pub imports: Vec<String>,
    pub variants: Vec<Variant>,
    pub impls: Vec<ImplBlock>,
}

impl Enum {
    pub fn add_derive(&mut self, path: impl Into<String>) {
        push_derive(&mut self.derives, path.into());
    }

    pub fn remove_derive(&mut self, name: &str) {
        self.derives.retain(|derive| short_name(derive) != short_name(name));
    }

    pub fn has_derive(&self, name: &str) -> bool {
        self.derives.iter().any(|derive| short_name(derive) == short_name(name))
    }

    pub fn add_attribute(&mut self, attribute: impl Into<String>) {
        push_unique(&mut self.attributes, attribute.into());
    }

    pub fn add_import(&mut self, import: impl AsRef<str>) {
        push_unique(&mut self.imports, normalize_import(import.as_ref()));
    }

    pub fn add_impl(&mut self, block: ImplBlock) {
        self.impls.push(block);
    }

    pub fn add_impl_method(&mut self, method: ImplMethod) {
        inherent_block(&mut self.impls).methods.push(method);
    }

    pub fn add_trait_impl(&mut self, trait_path: impl Into<String>, items: impl ToString) {
        self.impls.push(ImplBlock::for_trait(trait_path).item(items));
    }

    pub(crate) fn render(&self, out: &mut String) {
        out.push_str(&format!("#[derive({})]\n", self.derives.join(", ")));
        for attribute in &self.attributes {
            out.push_str(attribute);
            out.push('\n');
        }
        out.push_str(&format!("pub enum {} {{\n", self.name));
        for variant in &self.variants {
            for attribute in &variant.attributes {
                out.push_str(&format!("    {attribute}\n"));
            }
            out.push_str(&format!("    {},\n", variant.name));
        }
        out.push_str("}\n");
        for block in &self.impls {
            block.render(&self.name, out);
        }
    }
}

/// Everything codegen is about to write.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub enums: Vec<Enum>,
    pub models: Vec<Model>,
    /// `use` statements added to every generated file.
    pub imports: Vec<String>,
}

impl Schema {
    pub fn add_import(&mut self, import: impl AsRef<str>) {
        push_unique(&mut self.imports, normalize_import(import.as_ref()));
    }

    pub fn model(&self, name: &str) -> Option<&Model> {
        self.models.iter().find(|model| model.name == name)
    }

    pub fn model_mut(&mut self, name: &str) -> Option<&mut Model> {
        self.models.iter_mut().find(|model| model.name == name)
    }

    pub fn enum_(&self, name: &str) -> Option<&Enum> {
        self.enums.iter().find(|item| item.name == name)
    }

    pub fn enum_mut(&mut self, name: &str) -> Option<&mut Enum> {
        self.enums.iter_mut().find(|item| item.name == name)
    }

    /// The imports a file needs: the schema-wide imports plus `own`, sorted and
    /// deduplicated.
    pub fn imports_for(&self, own: &[String]) -> BTreeSet<String> {
        self.imports.iter().chain(own).cloned().collect()
    }
}

/// Hooks that customize the generated code. Every method has an empty default,
/// so implement only what you need.
///
/// Hooks run in this order: `transform_schema`, then for every enum
/// `transform_enum` followed by `transform_variant` for each variant, then for
/// every model `transform_model` followed by `transform_field` and
/// `transform_relation` for each field.
pub trait DinocoTransformer {
    fn transform_schema(&self, _schema: &mut Schema) {}

    fn transform_enum(&self, _item: &mut Enum) {}

    fn transform_variant(&self, _item: &Enum, _variant: &mut Variant) {}

    fn transform_model(&self, _model: &mut Model) {}

    /// `model` is a snapshot taken before any field of it was transformed.
    fn transform_field(&self, _model: &Model, _field: &mut Field) {}

    /// `model` is a snapshot taken before any relation of it was transformed.
    fn transform_relation(&self, _model: &Model, _relation: &mut Relation) {}
}

/// The transformer that changes nothing.
pub struct NoTransform;

impl DinocoTransformer for NoTransform {}

struct ModelFn<F>(F);

impl<F> DinocoTransformer for ModelFn<F>
where
    F: Fn(&mut Model),
{
    fn transform_model(&self, model: &mut Model) {
        (self.0)(model);
    }
}

/// Builds a transformer from a single closure that runs on every model.
pub fn transform_models<F>(callback: F) -> impl DinocoTransformer
where
    F: Fn(&mut Model),
{
    ModelFn(callback)
}

struct EnumFn<F>(F);

impl<F> DinocoTransformer for EnumFn<F>
where
    F: Fn(&mut Enum),
{
    fn transform_enum(&self, item: &mut Enum) {
        (self.0)(item);
    }
}

/// Builds a transformer from a single closure that runs on every enum.
pub fn transform_enums<F>(callback: F) -> impl DinocoTransformer
where
    F: Fn(&mut Enum),
{
    EnumFn(callback)
}

/// Runs `transformer` over `schema` in the documented order.
pub fn apply_transformer(schema: &mut Schema, transformer: &dyn DinocoTransformer) {
    transformer.transform_schema(schema);

    for item in &mut schema.enums {
        transformer.transform_enum(item);
        let snapshot = item.clone();
        for variant in &mut item.variants {
            transformer.transform_variant(&snapshot, variant);
        }
    }

    for model in &mut schema.models {
        transformer.transform_model(model);
        let snapshot = model.clone();
        for field in &mut model.fields {
            transformer.transform_field(&snapshot, field);
        }
        for relation in &mut model.relations {
            transformer.transform_relation(&snapshot, relation);
        }
    }
}

fn inherent_block(impls: &mut Vec<ImplBlock>) -> &mut ImplBlock {
    let index = match impls.iter().position(|block| block.trait_path.is_none()) {
        Some(index) => index,
        None => {
            impls.push(ImplBlock::inherent());
            impls.len() - 1
        }
    };
    &mut impls[index]
}

fn short_name(path: &str) -> &str {
    path.trim().rsplit("::").next().unwrap_or(path).trim()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn push_derive(derives: &mut Vec<String>, path: String) {
    if !derives.iter().any(|derive| short_name(derive) == short_name(&path)) {
        derives.push(path);
    }
}

/// Normalizes `use a::b`, `use a::b;` and `a::b` into `use a::b;`.
pub fn normalize_import(import: &str) -> String {
    let import = import.trim().trim_end_matches(';').trim();
    let import = import.strip_prefix("use ").unwrap_or(import).trim();
    format!("use {import};")
}
