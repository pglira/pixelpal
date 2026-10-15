use std::fmt::Write as _;
use std::path::Path;

/// Directory of the animations that `tools/link-extract` writes: one
/// subdirectory per gear tier, `tier0`, `tier1`, ...
const DIR: &str = "assets/link";

/// Embeds `animations.json` and every PNG of each tier directory: writes
/// `$OUT_DIR/link_assets.rs` with one `include_bytes!` per file.
fn main() {
    println!("cargo:rerun-if-changed={DIR}");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(DIR);

    let mut tiers = Vec::new();
    while root
        .join(format!("tier{}", tiers.len()))
        .join("animations.json")
        .is_file()
    {
        tiers.push(root.join(format!("tier{}", tiers.len())));
    }
    if tiers.is_empty() {
        panic!(
            "\n\n{DIR}/tier0/animations.json is missing.\n\
             Extract the animations of Link from your copy of the game with \
             tools/link-extract/extract_all.sh. See README.md, section \"Animations\".\n\n"
        );
    }

    let mut code = String::from("pub const TIERS: &[Tier] = &[\n");
    for dir in &tiers {
        println!("cargo:rerun-if-changed={}", dir.display());
        let index = dir.join("animations.json");
        println!("cargo:rerun-if-changed={}", index.display());
        let mut strips: Vec<String> = std::fs::read_dir(dir)
            .expect("the tier directory is readable")
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.ends_with(".png") && n != "preview.png")
            .collect();
        strips.sort();
        writeln!(
            code,
            "    Tier {{\n        json: include_bytes!({:?}),\n        strips: &[",
            index.display().to_string()
        )
        .unwrap();
        for name in &strips {
            let path = dir.join(name);
            println!("cargo:rerun-if-changed={}", path.display());
            writeln!(
                code,
                "            ({name:?}, include_bytes!({:?})),",
                path.display().to_string()
            )
            .unwrap();
        }
        code.push_str("        ],\n    },\n");
    }
    code.push_str("];\n");

    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("link_assets.rs");
    std::fs::write(out, code).expect("OUT_DIR is writable");
}
