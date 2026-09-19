use dinoco_codegen::prelude::*;
use dinoco_codegen::{build_schema, render_model_file, render_model_source, render_models_mod, render_models_mod_from};

const SCHEMA: &str = r#"
enum Status { active inactive }

model User {
    id         String   @id
    name       String
    status     Status
    deleted_at DateTime?
    created_at DateTime @default(now())
    posts      Post[]
}

model Post {
    id     String @id
    title  String
    author User?  @relation(fields: [author_id], references: [id])
    author_id String?
}
"#;

fn schema() -> dinoco_compiler::Schema {
    dinoco_compiler::compile(SCHEMA).expect("schema")
}

fn render_model(transformer: &dyn DinocoTransformer, name: &str) -> String {
    let mut generated = build_schema(&schema());
    apply_transformer(&mut generated, transformer);
    let model = generated.model(name).expect("model");
    render_model_source(model, &generated.imports_for(&model.imports))
}

fn render_mod(transformer: &dyn DinocoTransformer) -> String {
    let mut generated = build_schema(&schema());
    apply_transformer(&mut generated, transformer);
    render_models_mod_from(&generated)
}

#[test]
fn no_transform_renders_exactly_what_the_plain_renderers_do() {
    let schema = schema();
    let mut generated = build_schema(&schema);
    apply_transformer(&mut generated, &NoTransform);

    assert_eq!(render_models_mod_from(&generated), render_models_mod(&schema));
    let user = schema.models().find(|model| model.name == "User").expect("user");
    let model = generated.model("User").expect("user");
    assert_eq!(render_model_source(model, &Default::default()), render_model_file(user, &schema));
}

#[test]
fn transformer_adds_derives_imports_and_field_attributes() {
    struct Serde;
    impl DinocoTransformer for Serde {
        fn transform_model(&self, model: &mut Model) {
            model.add_derive("validator::Validate");
            model.add_derive("serde::Serialize"); // already generated as ::dinoco::serde::Serialize
            model.add_import("use validator::Validate");
            model.add_import("validator::Validate;");
            model.add_attribute("#[serde(rename_all = \"camelCase\")]");
        }

        fn transform_field(&self, _model: &Model, field: &mut Field) {
            if field.name.ends_with("_at") {
                field.add_attribute(r#"#[serde(with = "chrono::serde::ts_seconds")]"#);
            }
        }
    }

    let user = render_model(&Serde, "User");

    assert!(user.contains("::dinoco::serde::Deserialize, validator::Validate)]"), "{user}");
    assert_eq!(user.matches("Serialize").count(), 1, "same-named derives must not be duplicated");
    assert_eq!(user.matches("use validator::Validate;").count(), 1, "imports are normalized and deduplicated");
    assert!(user.contains("#[serde(rename_all = \"camelCase\")]\npub struct User"), "{user}");
    assert!(user.contains("    #[serde(with = \"chrono::serde::ts_seconds\")]\n    pub created_at:"), "{user}");
    assert!(
        user.contains(
            "    #[dinoco(default = ::dinoco::chrono::Utc::now())]\n    #[serde(with = \"chrono::serde::ts_seconds\")]"
        ),
        "{user}"
    );
    assert!(!render_model(&Serde, "Post").contains("ts_seconds"));
}

#[test]
fn transformer_adds_inherent_methods_and_trait_impls() {
    let transformer = transform_models(|model| {
        if model.name != "User" {
            return;
        }

        model.add_impl_method(ImplMethod::new("is_active", "bool", quote! { self.deleted_at.is_none() }));
        model.add_impl_method(ImplMethod {
            name: "with_name".into(),
            visibility: Visibility::Crate,
            receiver: Receiver::Owned,
            params: vec![("name".into(), "String".into())],
            return_type: "Self".into(),
            body: "Self { name, ..self }".into(),
            ..ImplMethod::default()
        });
        model.add_import("std::fmt");
        model.add_trait_impl(
            "fmt::Display",
            "fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {\n    write!(formatter, \"{}\", self.name)\n}",
        );
    });

    let user = render_model(&transformer, "User");

    assert!(user.contains("use std::fmt;"), "{user}");
    assert!(user.contains("impl User {\n    pub fn is_active(&self) -> bool {"), "{user}");
    assert!(user.contains("self . deleted_at . is_none ()") || user.contains("self.deleted_at.is_none()"), "{user}");
    assert!(user.contains("    pub(crate) fn with_name(self, name: String) -> Self {"), "{user}");
    assert_eq!(user.matches("impl User {").count(), 1, "methods share one inherent impl block");
    assert!(user.contains("impl fmt::Display for User {\n    fn fmt(&self,"), "{user}");
    assert!(!render_model(&transformer, "Post").contains("impl "));
}

#[test]
fn transformer_customizes_enums_variants_and_schema_wide_imports() {
    struct Enums;
    impl DinocoTransformer for Enums {
        fn transform_schema(&self, schema: &mut Schema) {
            schema.add_import("use strum::Display");
        }

        fn transform_enum(&self, item: &mut Enum) {
            item.add_derive("strum::Display");
            item.add_derive("Hash");
            item.add_impl_method(ImplMethod::new(
                "label",
                "&'static str",
                format!(
                    "match self {{ {} }}",
                    item.variants.iter().map(|v| format!("Self::{} => \"{}\",", v.name, v.value)).collect::<String>()
                ),
            ));
        }

        fn transform_variant(&self, item: &Enum, variant: &mut Variant) {
            if item.name == "Status" && variant.value == "inactive" {
                variant.add_attribute("#[deprecated]");
            }
        }
    }

    let models_mod = render_mod(&Enums);

    assert!(models_mod.starts_with("use strum::Display;\n"), "{models_mod}");
    assert!(models_mod.contains("::dinoco::DinocoEnum, strum::Display, Hash)]"), "{models_mod}");
    assert_eq!(models_mod.matches("Display").count(), 1 + 1, "import + skipped duplicate derive: {models_mod}");
    assert!(models_mod.contains("    #[deprecated]\n    Inactive,"), "{models_mod}");
    assert!(models_mod.contains("impl Status {\n    pub fn label(&self) -> &'static str {"), "{models_mod}");
    // Schema-wide imports reach every model file as well.
    assert!(render_model(&Enums, "Post").contains("use strum::Display;"));
}

#[test]
fn hooks_run_in_order_with_pre_transform_snapshots() {
    use std::cell::RefCell;

    #[derive(Default)]
    struct Recorder(RefCell<Vec<String>>);

    impl DinocoTransformer for Recorder {
        fn transform_schema(&self, _: &mut Schema) {
            self.0.borrow_mut().push("schema".into());
        }

        fn transform_enum(&self, item: &mut Enum) {
            self.0.borrow_mut().push(format!("enum:{}", item.name));
        }

        fn transform_variant(&self, _: &Enum, variant: &mut Variant) {
            self.0.borrow_mut().push(format!("variant:{}", variant.name));
        }

        fn transform_model(&self, model: &mut Model) {
            self.0.borrow_mut().push(format!("model:{}", model.name));
            model.fields.retain(|field| field.name != "name");
        }

        fn transform_field(&self, model: &Model, field: &mut Field) {
            self.0.borrow_mut().push(format!("field:{}.{}", model.name, field.name));
        }

        fn transform_relation(&self, model: &Model, relation: &mut Relation) {
            self.0.borrow_mut().push(format!("relation:{}.{}->{}", model.name, relation.name, relation.target));
        }
    }

    let recorder = Recorder::default();
    let mut generated = build_schema(&schema());
    apply_transformer(&mut generated, &recorder);
    let calls = recorder.0.into_inner();

    assert_eq!(&calls[..5], ["schema", "enum:Status", "variant:Active", "variant:Inactive", "model:User"]);
    assert!(calls.contains(&"relation:User.posts->Post".to_string()));
    assert!(calls.contains(&"relation:Post.author->User".to_string()));
    assert!(!calls.iter().any(|call| call == "field:User.name"), "a field removed by transform_model is not visited");
    assert!(calls.contains(&"field:User.id".to_string()));
}

#[test]
fn transformer_can_edit_relations_and_field_types() {
    struct Edit;
    impl DinocoTransformer for Edit {
        fn transform_model(&self, model: &mut Model) {
            if let Some(field) = model.field_mut("name") {
                field.ty = RustType::new("::std::sync::Arc<str>");
                field.nullable = true;
            }
        }

        fn transform_relation(&self, _model: &Model, relation: &mut Relation) {
            relation.add_attribute("#[serde(skip)]");
        }
    }

    let user = render_model(&Edit, "User");
    let post = render_model(&Edit, "Post");

    assert!(user.contains("pub name: Option<::std::sync::Arc<str>>,"), "{user}");
    assert!(user.contains("#[serde(skip)]\n    pub posts: Vec<Post>,"), "{user}");
    assert!(post.contains("#[serde(skip)]\n    pub author: Option<User>,"), "{post}");
}

#[test]
fn transformed_output_is_written_by_generate_models_with() {
    let previous = std::env::current_dir().expect("cwd");
    let project = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(project.path().join("dinoco")).expect("dinoco dir");
    std::env::set_current_dir(project.path()).expect("chdir");

    let result =
        dinoco_codegen::generate_models_with(&schema(), None, &transform_models(|model| model.add_derive("Hash")));
    let user = std::fs::read_to_string(project.path().join("dinoco/models/user.rs"));
    std::env::set_current_dir(previous).expect("restore cwd");

    result.expect("generate");
    assert!(user.expect("user.rs").contains(", Hash)]"));
}
