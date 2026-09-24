# Referência da API de transforms

Tudo abaixo é exportado por `dinoco_codegen::prelude::*`. Um transformer recebe uma descrição estruturada do código que o Dinoco vai escrever, altera essa descrição e o Dinoco renderiza o resultado.

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

Todo método tem um padrão vazio; implemente só o que precisar. Os hooks rodam nesta ordem:

1. `transform_schema`, uma vez, com todos os enums e models.
2. Para cada enum: `transform_enum` e depois `transform_variant` para cada variante.
3. Para cada model: `transform_model`, depois `transform_field` para cada campo escalar/enum e `transform_relation` para cada relação.

`transform_variant`, `transform_field` e `transform_relation` recebem o pai como um **snapshot** somente leitura, tirado depois do hook do próprio pai. Mudanças feitas em um campo não aparecem no snapshot entregue ao campo seguinte. Um campo removido dentro de `transform_model` nunca é visitado.

Dois helpers transformam uma closure em transformer: `transform_models(|model| ...)` e `transform_enums(|item| ...)`.

## Schema

| Membro | Tipo | Descrição |
| --- | --- | --- |
| `enums` | `Vec<Enum>` | Todos os enums gerados |
| `models` | `Vec<Model>` | Todas as structs geradas |
| `imports` | `Vec<String>` | `use` adicionados a **todo** arquivo gerado |
| `add_import(path)` | método | Adiciona um import |
| `model(nome)` / `model_mut(nome)` | método | Busca um model pelo nome no schema |
| `enum_(nome)` / `enum_mut(nome)` | método | Busca um enum pelo nome no schema |

## Model (struct)

| Membro | Tipo | Descrição |
| --- | --- | --- |
| `name` | `String` | O nome da struct Rust |
| `table_name` | `String` | A tabela no banco |
| `derives` | `Vec<String>` | Caminhos de derive, como `Debug`, `::dinoco::serde::Serialize` |
| `attributes` | `Vec<String>` | Atributos do tipo, um atributo completo por item |
| `imports` | `Vec<String>` | `use` adicionados ao arquivo deste model |
| `fields` | `Vec<Field>` | Campos escalares, enum e chaves many-to-many |
| `relations` | `Vec<Relation>` | Campos que apontam para outro model |
| `impls` | `Vec<ImplBlock>` | Blocos `impl` renderizados depois da struct |

| Método | Descrição |
| --- | --- |
| `add_derive(path)` | Adiciona um derive, a menos que exista um com o mesmo último segmento |
| `remove_derive(nome)` / `has_derive(nome)` | Comparam pelo último segmento do caminho |
| `add_attribute(attr)` | Adiciona um atributo completo como `#[serde(rename_all = "camelCase")]` (sem duplicar) |
| `add_import(path)` | Adiciona um `use`; `std::fmt`, `use std::fmt` e `use std::fmt;` são o mesmo import |
| `add_impl_method(method)` | Adiciona um método ao bloco `impl` inerente do model (criado no primeiro uso) |
| `add_trait_impl(trait_path, items)` | Adiciona `impl <trait_path> for Model { <items> }`; `items` é o texto bruto do corpo |
| `add_impl(block)` | Adiciona um `ImplBlock` completo |
| `field(nome)` / `field_mut(nome)` | Busca um campo |
| `relation(nome)` / `relation_mut(nome)` | Busca uma relação |

## Enum

`Enum` tem `name`, `derives`, `attributes`, `imports`, `impls` e `variants: Vec<Variant>`, além dos mesmos métodos `add_derive`, `remove_derive`, `has_derive`, `add_attribute`, `add_import`, `add_impl_method`, `add_trait_impl` e `add_impl` do `Model`.

## Variant

| Membro | Descrição |
| --- | --- |
| `name` | O nome da variante em Rust (PascalCase) |
| `value` | O valor guardado no banco |
| `attributes` | Atributos renderizados acima da variante |
| `add_attribute(attr)` | Adiciona um atributo |

## Field

| Membro | Descrição |
| --- | --- |
| `name` | O nome do campo |
| `ty` | `RustType`, o tipo **sem** o wrapper `Option<...>` |
| `nullable` | Quando `true`, renderizado como `Option<ty>` |
| `attributes` | Atributos renderizados acima do campo |
| `add_attribute(attr)` | Adiciona um atributo (sem duplicar) |
| `has_attribute(prefixo)` | `true` se algum atributo começa com `prefixo` |
| `remove_attributes(prefixo)` | Remove atributos que começam com `prefixo` |

Campos escalares de lista mantêm o `Vec<...>` dentro de `ty`. `RustType::new("::std::sync::Arc<str>")` cria um tipo; `&str` e `String` convertem com `.into()`.

## Relation

| Membro | Descrição |
| --- | --- |
| `name` | O nome do campo |
| `target` | O nome, no schema, do model para o qual aponta |
| `list` | Renderizado como `Vec<ty>` |
| `nullable` | Renderizado como `Option<ty>` (ignorado quando `list` está ativo) |
| `ty` | O tipo relacionado (`Box<Self>` numa auto-relação opcional) |
| `attributes` | Atributos renderizados acima do campo |

## ImplBlock

```rust
pub struct ImplBlock {
    pub trait_path: Option<String>,   // None: `impl Tipo`, Some: `impl Trait for Tipo`
    pub attributes: Vec<String>,      // ex.: #[allow(clippy::...)]
    pub items: Vec<String>,           // itens associados brutos: `type Error = ..;`, `const`, métodos inteiros
    pub methods: Vec<ImplMethod>,
}
```

Crie com `ImplBlock::inherent()` ou `ImplBlock::for_trait("std::fmt::Display")` e encadeie `.item(texto)` / `.method(method)`. Os `items` brutos são renderizados antes dos `methods`.

## ImplMethod

```rust
pub struct ImplMethod {
    pub name: String,
    pub visibility: Visibility,      // Public (padrão) | Crate | Private
    pub is_async: bool,
    pub receiver: Receiver,          // Ref (padrão) | RefMut | Owned | None
    pub params: Vec<(String, String)>, // (nome, tipo), sem o receiver
    pub return_type: String,         // vazio significa ()
    pub body: String,                // sem as chaves externas
    pub attributes: Vec<String>,
    pub docs: Vec<String>,
}
```

`ImplMethod::new(nome, tipo_de_retorno, corpo)` define os três essenciais e deixa o resto no padrão; use a sintaxe de atualização de struct (`..ImplMethod::default()`) para os demais. `body` aceita qualquer coisa que implemente `ToString`, então tanto uma string quanto um token stream de `quote! { ... }` funcionam. Um token stream é renderizado numa linha com tokens espaçados (`self . deleted_at . is_none ()`); é Rust válido e o `rustfmt` arruma.

Use `Visibility::Private` em métodos dentro de um impl de trait, onde qualificadores de visibilidade não são permitidos.

## Imports

O Dinoco não resolve nem verifica imports; ele escreve o que você passar, uma vez por arquivo, ordenado. `add_import` num model ou enum vai só para aquele arquivo; `Schema::add_import` vai para todo arquivo gerado. Imports de enum caem em `dinoco/models/mod.rs`, imports de model no arquivo do próprio model. A crate que fornece um item importado continua sendo sua para adicionar ao `Cargo.toml`.

## Funções

| Função | Descrição |
| --- | --- |
| `apply_transformer(&mut schema, &transformer)` | Roda um transformer sobre um `Schema` |
| `build_schema(&compiler_schema)` | Monta o `Schema` sem transformação a partir de um `schema.dinoco` compilado |
| `generate_models_with(&schema, workspace, &transformer)` | Escreve `dinoco/` com um transformer aplicado |
| `render_model_source(&model, &imports)` / `render_models_mod_from(&schema)` | Renderizam os arquivos sem escrevê-los — úteis em testes |
