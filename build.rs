use std::fmt::Write as _;
use std::path::Path;

/// Directory of the animations that `tools/link-extract` writes.
const DIR: &str = "assets/link";

/// Embeds `assets/link/animations.json` and every strip PNG of that directory:
/// writes `$OUT_DIR/link_assets.rs` with one `include_bytes!` per file.
fn main() {
    println!("cargo:rerun-if-changed={DIR}");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(DIR);
    let index = root.join("animations.json");
    if !index.is_file() {
        panic!(
            "\n\n{DIR}/animations.json is missing.\n\
             Extract the animations of Link from your copy of the game with \
             tools/link-extract. See README.md, section \"Animations\".\n\n"
        );
    }

    let mut strips: Vec<String> = std::fs::read_dir(&root)
        .expect("assets/link is readable")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".png") && n != "preview.png")
        .collect();
    strips.sort();

    let mut code = String::new();
    println!("cargo:rerun-if-changed={}", index.display());
    writeln!(
        code,
        "pub const ANIMATIONS_JSON: &[u8] = include_bytes!({:?});",
        index.display().to_string()
    )
    .unwrap();
    code.push_str("pub const STRIPS: &[(&str, &[u8])] = &[\n");
    for name in &strips {
        let path = root.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        writeln!(
            code,
            "    ({name:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");

    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("link_assets.rs");
    std::fs::write(out, code).expect("OUT_DIR is writable");
}
