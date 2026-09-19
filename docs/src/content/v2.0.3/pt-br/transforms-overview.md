# Visão geral dos code transforms

O Dinoco gera seus models, mas o Rust gerado raramente é a palavra final: você quer atributos `serde` nos campos de timestamp, um derive `Validate`, um helper `is_active()`, um impl de `Display`. Os **code transforms** permitem mudar o que é gerado — derives, atributos, imports, métodos, impls de trait — a partir de um arquivo Rust do seu projeto, sem editar código gerado que seria sobrescrito na próxima execução.

> [!WARNING]
> Os transforms substituem `config.custom_derives`, que foi **removido**. Um schema que ainda declara `custom_derives` agora falha ao compilar com uma mensagem apontando para cá. Veja [Migrar de custom_derives](/pt-br/docs/orm/guide/transforms-recipes#migrar-de-custom-derives).

## Como funciona

1. `dinoco models generate` (ou `dinoco migrate generate`) compila o `schema.dinoco`.
2. O Dinoco monta, na memória, uma descrição do Rust que vai escrever: cada enum e struct, com seus derives, atributos, imports, campos, relações e blocos `impl`.
3. Se `dinoco/transform.rs` existe, a CLI o compila e executa. O seu `transformer()` recebe essa descrição **mutável**, uma peça por vez, e a altera.
4. O Dinoco renderiza o resultado em `dinoco/models/`.

Não há linguagem de template nem nada para registrar: um transform é Rust comum editando dados simples (`model.derives`, `field.attributes`, `model.impls`, ...). O que você adiciona é escrito como texto no arquivo gerado, então tudo que você consegue escrever em Rust, consegue gerar.

## O arquivo de transform

Crie `dinoco/transform.rs`. Ele precisa expor uma função, `transformer()`, que retorna algo que implementa `DinocoTransformer`:

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

Depois gere como sempre:

```bash
dinoco models generate      # ou: dinoco migrate generate
```

Sempre que `dinoco/transform.rs` existe, os dois comandos o aplicam. Apague o arquivo e a próxima geração volta à saída padrão. `dinoco_codegen::prelude::*` traz tudo que é usado nesta página: `DinocoTransformer`, `Schema`, `Model`, `Enum`, `Field`, `Variant`, `Relation`, `ImplBlock`, `ImplMethod`, `Visibility`, `Receiver`, `RustType` e a macro `quote!`.

## Os hooks

Implemente só os hooks de que precisa; os demais não fazem nada.

| Hook | Roda para | Recebe |
| --- | --- | --- |
| `transform_schema` | Uma vez | O `Schema` inteiro (todos os models e enums) |
| `transform_model` | Cada struct | `&mut Model` |
| `transform_field` | Cada campo escalar/enum | O `&Model` pai (somente leitura) e `&mut Field` |
| `transform_relation` | Cada campo que aponta para outro model | O `&Model` pai e `&mut Relation` |
| `transform_enum` | Cada enum | `&mut Enum` |
| `transform_variant` | Cada variante de enum | O `&Enum` pai e `&mut Variant` |

## Imports

Um **import** é uma linha `use` do Rust escrita no topo de um arquivo gerado, para que os derives, traits e tipos que você referencia sejam resolvidos. O Dinoco não adivinha o que o seu transform precisa: tudo que você usa num derive, atributo, método ou impl precisa ser importado — com `add_import`, no item que o usa.

```rust
fn transform_model(&self, model: &mut Model) {
    model.add_import("use lib::Trait;");     // escrito exatamente como um `use`
    model.add_import("lib::Trait");          // mesmo import: `use` e `;` são opcionais
    model.add_import("lib::{Trait, Other}"); // grupos também funcionam
    model.add_import("std::fmt as std_fmt"); // renomeações também
}
```

O `dinoco/models/user.rs` gerado então começa com:

```rust
#[allow(unused_imports)]
use super::*;
use dinoco::Entity;
use lib::Trait;
use lib::{Trait, Other};
use std::fmt as std_fmt;
```

Regras práticas:

- As três grafias acima são o **mesmo** import: `use lib::Trait`, `use lib::Trait;` e `lib::Trait` são normalizadas e escritas uma vez.
- `add_import` num `Model` vai para o **arquivo daquele model**. Num `Enum`, vai para `dinoco/models/mod.rs`, onde ficam os enums.
- `schema.add_import(...)` (em `transform_schema`) vai para **todo** arquivo gerado.
- Os imports são ordenados e deduplicados por arquivo.
- A crate que fornece o import (`lib` acima) é dependência da *sua aplicação* — adicione-a ao `Cargo.toml`. O Dinoco escreve a linha `use` e não verifica se ela resolve.
- Os caminhos são resolvidos a partir do arquivo gerado, que fica em `dinoco/models/`. Use `crate::...` para o seu código, ou um caminho absoluto de crate.

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

Um derive cujo último segmento já existe é ignorado — derivar `Clone` ou `Serialize` duas vezes não compilaria — então `model.add_derive("serde::Serialize")` é um no-op inofensivo. `model.remove_derive("Clone")` e `model.has_derive("Clone")` também existem. Os mesmos métodos funcionam em `Enum`.

## Métodos

Adicione um método com `add_impl_method`. Todos os métodos que você adiciona vão para um único bloco `impl` inerente, em ordem:

```rust
fn transform_model(&self, model: &mut Model) {
    // Forma mais curta: nome, tipo de retorno, corpo.
    model.add_impl_method(ImplMethod::new(
        "is_active",
        "bool",
        quote! { self.deleted_at.is_none() },
    ));

    // Controle total: visibilidade, receiver, parâmetros, async, atributos, docs.
    model.add_impl_method(ImplMethod {
        name: "with_name".into(),
        visibility: Visibility::Public,
        receiver: Receiver::Owned,                     // self
        params: vec![("name".into(), "String".into())],
        return_type: "Self".into(),
        body: "Self { name, ..self }".into(),
        attributes: vec!["#[must_use]".into()],
        docs: vec!["Retorna uma cópia com um novo nome.".into()],
        ..ImplMethod::default()
    });

    // Uma função associada (sem `self`).
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

    /// Retorna uma cópia com um novo nome.
    #[must_use]
    pub fn with_name(self, name: String) -> Self {
        Self { name, ..self }
    }

    pub fn table() -> &'static str {
        "user"
    }
}
```

`body` é o texto entre as chaves. Aceita qualquer coisa que implemente `ToString`: uma string, um `format!` ou um token stream de `quote! { ... }`. Um corpo `quote!` é escrito numa linha com tokens espaçados (`self . deleted_at . is_none ()`), que é Rust válido e o `rustfmt` arruma — use uma string simples quando quiser preservar o layout.

## Traits

`add_trait_impl(trait_path, items)` escreve `impl <trait_path> for Model { <items> }`, onde `items` é o texto bruto do corpo. **Importe a trait que você implementa**, ou escreva o caminho completo:

```rust
fn transform_model(&self, model: &mut Model) {
    // 1. Uma trait da std, importada.
    model.add_import("std::fmt");
    model.add_trait_impl(
        "fmt::Display",
        r#"fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "User({})", self.id)
}"#,
    );

    // 2. Uma trait da sua própria crate ou de uma dependência.
    model.add_import("lib::Trait");
    model.add_trait_impl("Trait", "fn label(&self) -> String {\n    self.id.to_string()\n}");

    // 3. Uma trait marcadora sem nada a implementar.
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

Quando a trait tem tipos ou constantes associados, ou quando você quer montar os métodos com `ImplMethod`, use um `ImplBlock`. Num impl de trait, os métodos usam `Visibility::Private` (um qualificador `pub` não é permitido ali):

```rust
fn transform_model(&self, model: &mut Model) {
    model.add_import("lib::Repository");

    model.add_impl(
        ImplBlock::for_trait("Repository")
            .item("type Id = String;")                 // item associado bruto
            .item("const KIND: &'static str = \"user\";")
            .method(ImplMethod {
                name: "id".into(),
                visibility: Visibility::Private,       // obrigatório dentro de um impl de trait
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

Traits genéricas recebem os argumentos no caminho: `ImplBlock::for_trait("From<UserId>")`, `add_trait_impl("TryFrom<&str>", "...")`.

## Atributos de campos

`transform_field` enxerga todo campo escalar e enum. Atributos são linhas de atributo completas, acrescentadas depois das que o Dinoco escreve:

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

Atributos no nível do tipo passam por `model.add_attribute(...)` (`#[serde(rename_all = "camelCase")]`). Você também pode mudar o tipo de um campo (`field.ty = RustType::new("crate::Metadata")`) ou a nulabilidade (`field.nullable = true`), e dar atributos a relações em `transform_relation`.

## Enums

Tudo acima funciona também para enums, e `transform_variant` alcança cada variante:

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

## Tudo junto

Um único transformer pode combinar tudo isso e ramificar pelo nome do model ou pelos seus campos:

```rust
use dinoco_codegen::prelude::*;

struct Project;

impl DinocoTransformer for Project {
    fn transform_schema(&self, schema: &mut Schema) {
        schema.add_import("crate::traits::Auditable"); // todo arquivo gerado
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

Rodando sobre `model User { id String @id  deleted_at DateTime? }` produz:

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

Mais exemplos copiáveis estão nas [receitas](/pt-br/docs/orm/guide/transforms-recipes), e todo tipo e método está na [referência da API](/pt-br/docs/orm/guide/transforms-api).

## Só uma closure

Se você só precisa mexer em todo model ou todo enum, dispense a trait:

```rust
use dinoco_codegen::prelude::*;

pub fn transformer() -> impl DinocoTransformer {
    transform_models(|model| {
        model.add_derive("Hash");
    })
}
```

`transform_enums(|item| { ... })` faz o mesmo para enums.

## Onde o transform roda

O transform é compilado e executado pela CLI, não pela sua aplicação. É um programa separado, então o `transform.rs` pode usar `dinoco_codegen` (por `dinoco_codegen::prelude::*`), `quote!` e `std` — mas **não** os módulos ou crates da sua aplicação. Por isso os tipos e traits que você *menciona* no código gerado são só texto: eles precisam existir na sua aplicação, não no `transform.rs`. Ele nunca entra no seu build: `dinoco/transform.rs` não é declarado como módulo em lugar nenhum, e o editor o trata como um arquivo avulso.

Assim como as [migrations manuais](/pt-br/docs/orm/guide/migration-workflow#como-as-migrations-sao-executadas), ele precisa de uma toolchain Rust onde `dinoco models generate` roda, compila por um projeto gerado em `dinoco/.runner/` (ignorado pelo Git sozinho) e imprime os erros do Cargo se o `transform.rs` não compilar. Um transform que falha interrompe a geração; o Dinoco nunca volta para a saída sem transformação.

## O código gerado continua gerado

Os arquivos gerados são totalmente reescritos a cada execução, então coloque as customizações no `transform.rs` — nunca em `dinoco/models/`. Como o transformer roda a cada geração, o resultado é reproduzível, revisável no Git e idêntico em toda máquina.

Os atributos e derives do próprio Dinoco fazem parte do model que você recebe (`#[derive(Debug, Clone, Entity, ...)]`, `#[dinoco(...)]`, `#[serde(crate = ...)]`). Você pode inspecioná-los, e remover um é permitido, mas pode impedir o código gerado de compilar — a decisão é sua. O Dinoco não compila o que você gera: um erro de digitação num corpo, atributo ou import aparece na próxima vez que você compilar a aplicação, no arquivo gerado.
