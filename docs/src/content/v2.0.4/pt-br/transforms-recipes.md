# Receitas de transforms

Cada receita é um `dinoco/transform.rs` completo que você pode copiar. Mantenha só os hooks que usar.

## Adicionar derives e imports

```rust
use dinoco_codegen::prelude::*;

pub fn transformer() -> impl DinocoTransformer {
    struct Derives;

    impl DinocoTransformer for Derives {
        fn transform_model(&self, model: &mut Model) {
            model.add_derive("validator::Validate");
            model.add_import("use validator::Validate");
        }

        fn transform_enum(&self, item: &mut Enum) {
            item.add_derive("strum::Display");
            item.add_import("use strum::Display");
        }
    }

    Derives
}
```

A crate que fornece o derive (`validator`, `strum`) é dependência da *sua* aplicação, então adicione-a ao seu `Cargo.toml`. Derives que compartilham o último segmento com um que o Dinoco já adiciona (`Clone`, `Debug`, `Serialize`, `Deserialize`) são ignorados, porque derivar a mesma trait duas vezes não compila.

## Adicionar atributos a campos

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

Os atributos são acrescentados depois dos que o Dinoco escreve. Num campo anulável, o `serde(with = ...)` que você escolher precisa combinar com o tipo `Option<...>`; verifique `field.nullable` quando o módulo citado só trata um formato.

## Adicionar métodos

```rust
fn transform_model(&self, model: &mut Model) {
    if model.has_derive("Entity") && model.field("deleted_at").is_some() {
        model.add_impl_method(ImplMethod::new(
            "is_active",
            "bool",
            quote! { self.deleted_at.is_none() },
        ));
    }

    model.add_impl_method(ImplMethod {
        name: "table".into(),
        receiver: Receiver::None,
        return_type: "&'static str".into(),
        body: format!("\"{}\"", model.table_name),
        ..ImplMethod::default()
    });
}
```

Todos os métodos adicionados com `add_impl_method` compartilham um único bloco `impl` inerente, na ordem em que foram adicionados.

## Implementar uma trait

```rust
fn transform_model(&self, model: &mut Model) {
    model.add_import("std::fmt");
    model.add_trait_impl(
        "fmt::Display",
        r#"fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", self.id)
}"#,
    );
}
```

Para uma trait com itens associados, monte o bloco explicitamente:

```rust
model.add_impl(
    ImplBlock::for_trait("TryFrom<&str>")
        .item("type Error = String;")
        .method(ImplMethod {
            name: "try_from".into(),
            visibility: Visibility::Private,
            receiver: Receiver::None,
            params: vec![("value".into(), "&str".into())],
            return_type: "Result<Self, Self::Error>".into(),
            body: "Err(format!(\"cannot parse {value}\"))".into(),
            ..ImplMethod::default()
        }),
);
```

## Enums

```rust
fn transform_enum(&self, item: &mut Enum) {
    item.add_derive("Hash");

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

## Todos os arquivos de uma vez

```rust
fn transform_schema(&self, schema: &mut Schema) {
    schema.add_import("use std::fmt::Debug");

    for model in &mut schema.models {
        model.add_attribute("#[serde(rename_all = \"camelCase\")]");
    }
}
```

## Mudar o tipo de um campo

```rust
fn transform_field(&self, model: &Model, field: &mut Field) {
    if field.name == "metadata" {
        field.ty = RustType::new("crate::Metadata");
    }
}
```

O Dinoco não verifica se o novo tipo funciona com a coluna do banco; para ele é só texto no arquivo gerado.

## Migrar de custom_derives

Troque cada entrada de `custom_derives` por um hook. Uma entrada com `into = "enum"` vira `transform_enum`, e `into = "struct"` vira `transform_model`:

```dinoco
config {
    custom_derives = [
        { into = "enum",   derive = "ZodSchema", import = "use zod_rs::prelude::*;" },
        { into = "struct", derive = "Validate",  import = "use validator::Validate;" }
    ]
}
```

vira, em `dinoco/transform.rs`:

```rust
use dinoco_codegen::prelude::*;

struct Legacy;

impl DinocoTransformer for Legacy {
    fn transform_enum(&self, item: &mut Enum) {
        item.add_derive("ZodSchema");
        item.add_import("use zod_rs::prelude::*;");
    }

    fn transform_model(&self, model: &mut Model) {
        model.add_derive("Validate");
        model.add_import("use validator::Validate;");
    }
}

pub fn transformer() -> impl DinocoTransformer {
    Legacy
}
```

e a chave `custom_derives` é apagada do `schema.dinoco`. Diferente do `custom_derives`, um transform pode olhar o model e aplicar um derive só a alguns deles.

## Testar um transform

Os renderizadores são funções comuns, então um transform pode ser testado sem tocar no sistema de arquivos, a partir de qualquer crate que dependa de `dinoco_codegen` e `dinoco_compiler`:

```rust
let schema = dinoco_compiler::compile("model User { id String @id }")?;
let mut generated = dinoco_codegen::build_schema(&schema);
dinoco_codegen::apply_transformer(&mut generated, &MyTransformer);

let user = generated.model("User").unwrap();
let source = dinoco_codegen::render_model_source(user, &generated.imports_for(&user.imports));
assert!(source.contains("pub fn is_active"));
```

## Limites

- O transformer roda como um programa próprio: ele enxerga o schema e a estrutura gerada pelo Dinoco, não os tipos da sua aplicação.
- Não renomeie models, enums nem seus campos: as relações, a lista de módulos gerada e o derive `Entity` continuam se referindo aos nomes originais. Customize por derives, atributos, imports e impls.
- O Dinoco não compila o texto que você gera. Um erro de digitação num corpo ou atributo aparece na próxima vez que você compilar a aplicação, no arquivo gerado.
