//! The ROM of *The Legend of Zelda: A Link to the Past* (USA), the source of
//! all graphics of pixelpal. pixelpal keeps a checked copy of the ROM in its
//! data directory and asks for the ROM file when that copy is missing.

use std::path::{Path, PathBuf};

use crate::picker;

/// SHA-1 of the ROM without a copier header. The sprite recipe needs this ROM.
pub const SHA1: &str = "6d4f10a8b10e10dbe624cb23cf03b88bb8252973";

/// Size of the ROM without a copier header, in bytes.
const SIZE: usize = 0x100000;

/// The name of the game and the version that pixelpal needs.
pub const GAME: &str = "The Legend of Zelda: A Link to the Past (USA)";

/// The checked copy of the ROM: `zelda3.sfc` in the data directory of pixelpal.
pub fn stored_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("pixelpal").join("zelda3.sfc"))
}

/// Reads the ROM at `path`, removes a copier header of 512 bytes, and checks
/// the size and the SHA-1.
pub fn read(path: &Path) -> Result<Vec<u8>, String> {
    let mut data =
        std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if data.len() % 1024 == 512 {
        data.drain(..512);
    }
    if data.len() != SIZE {
        return Err(format!(
            "{} is not the ROM of {GAME}: its size is {} bytes, the ROM has {SIZE} bytes.",
            path.display(),
            data.len()
        ));
    }
    let sha1 = sha1_smol::Sha1::from(&data).digest().to_string();
    if sha1 != SHA1 {
        return Err(format!(
            "{} is not the ROM of {GAME}.\nIts SHA-1 is {sha1}, the ROM has the SHA-1 {SHA1}.",
            path.display()
        ));
    }
    Ok(data)
}

/// Checks the ROM at `path` and stores a copy of it. Returns the ROM data.
pub fn import(path: &Path) -> Result<Vec<u8>, String> {
    let data = read(path)?;
    let target = stored_path().ok_or("No data directory found.")?;
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    }
    std::fs::write(&target, &data)
        .map_err(|e| format!("Cannot write {}: {e}", target.display()))?;
    Ok(data)
}

/// The stored ROM, or else a ROM file that the user selects in a dialog. A
/// selected file is checked and stored. The dialog opens again after a file
/// that fails the check. Returns an error if the user cancels the dialog or
/// if no dialog program is available.
pub fn obtain() -> Result<Vec<u8>, String> {
    let stored = stored_path();
    if let Some(path) = stored.as_deref().filter(|p| p.is_file()) {
        match read(path) {
            Ok(data) => return Ok(data),
            Err(e) => eprintln!("pixelpal: {e}"),
        }
    }
    let help = format!(
        "pixelpal draws Link with the graphics of the ROM of {GAME}.\n\
         Import the ROM with: pixelpal --import-rom <file>"
    );
    let start = dirs::home_dir().unwrap_or_default();
    let title = format!("pixelpal: select the ROM of {GAME}");
    loop {
        let path = match picker::pick_rom(&title, &start) {
            Ok(Some(path)) => path,
            Ok(None) => return Err(format!("No ROM selected.\n{help}")),
            Err(e) => return Err(format!("{e}\n{help}")),
        };
        match import(Path::new(&path)) {
            Ok(data) => return Ok(data),
            Err(e) => picker::show_error(&e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, len: usize) -> PathBuf {
        let path = std::env::temp_dir().join(format!("pixelpal-{}-{name}", std::process::id()));
        std::fs::write(&path, vec![0u8; len]).unwrap();
        path
    }

    #[test]
    fn a_file_of_the_wrong_size_is_rejected() {
        let path = temp_file("small.sfc", 1000);
        let e = read(&path).unwrap_err();
        std::fs::remove_file(&path).unwrap();
        assert!(e.contains("size is 1000 bytes"), "{e}");
    }

    #[test]
    fn a_copier_header_is_removed_before_the_checksum() {
        let path = temp_file("header.smc", SIZE + 512);
        let e = read(&path).unwrap_err();
        std::fs::remove_file(&path).unwrap();
        assert!(e.contains("SHA-1"), "{e}");
    }
}
