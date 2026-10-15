use std::path::Path;

const SHEET: &str = "assets/link_sheet.png";

fn main() {
    println!("cargo:rerun-if-changed={SHEET}");
    if !Path::new(SHEET).is_file() {
        panic!(
            "\n\n{SHEET} is missing.\n\
             Download \"SNES - The Legend of Zelda: A Link to the Past - Playable Characters - Link\" \
             from The Spriters Resource and copy it to {SHEET}. See README.md.\n\n"
        );
    }
}
