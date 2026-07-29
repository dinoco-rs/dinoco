use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn fulltext_method_only_exists_on_marked_string_fields() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root");
    let fixture = std::env::temp_dir().join(format!(
        "dinoco-fulltext-compile-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos()
    ));
    fs::create_dir_all(fixture.join("src")).expect("fixture source");
    fs::write(
        fixture.join("Cargo.toml"),
        format!(
            "[package]\nname = \"dinoco_fulltext_compile\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
             [dependencies]\ndinoco = {{ path = {:?} }}\n",
            root.join("crates/dinoco")
        ),
    )
    .expect("fixture manifest");

    fs::write(
        fixture.join("src/main.rs"),
        r#"
use dinoco::{Entity, find_first};

#[derive(Entity)]
#[dinoco(table_name = "article")]
struct Article {
    id: String,
    #[dinoco(fulltext)]
    title: String,
}

fn main() {
    let _ = find_first::<Article>().where_(|article| article.title.fulltext("rust"));
}
"#,
    )
    .expect("marked fixture");
    let marked = cargo_check(root, &fixture);
    assert!(
        marked.status.success(),
        "a marked String field must expose fulltext:\n{}",
        String::from_utf8_lossy(&marked.stderr)
    );

    fs::write(
        fixture.join("src/main.rs"),
        r#"
use dinoco::{Entity, find_first};

#[derive(Entity)]
#[dinoco(table_name = "article")]
struct Article {
    id: String,
    title: String,
}

fn main() {
    let _ = find_first::<Article>().where_(|article| article.title.fulltext("rust"));
}
"#,
    )
    .expect("unmarked fixture");
    let unmarked = cargo_check(root, &fixture);
    assert!(!unmarked.status.success(), "an unmarked String field must not expose fulltext");
    assert!(
        String::from_utf8_lossy(&unmarked.stderr).contains("no method named `fulltext`"),
        "{}",
        String::from_utf8_lossy(&unmarked.stderr)
    );

    fs::remove_dir_all(fixture).expect("remove fixture");
}

fn cargo_check(root: &std::path::Path, fixture: &std::path::Path) -> std::process::Output {
    Command::new("cargo")
        .args(["check", "--quiet", "--offline"])
        .env("CARGO_TARGET_DIR", root.join("target"))
        .current_dir(fixture)
        .output()
        .expect("cargo check fixture")
}
