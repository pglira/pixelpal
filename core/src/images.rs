//! Images of the chat, stored as PNG files in the `images` directory of the
//! pixelpal data directory.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use image::RgbaImage;
use image::imageops::FilterType;

use crate::history::SavedChat;

/// Longest edge of a stored image, in pixels. A larger image is scaled down.
const MAX_EDGE: u32 = 2000;

fn dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("pixelpal").join("images"))
}

/// Stores `rgba` as a PNG file in the image directory and returns its path.
pub fn store_rgba(rgba: &RgbaImage) -> Result<PathBuf, String> {
    let dir = dir().ok_or("No data directory.")?;
    store(&dir, rgba)
}

/// Stores the encoded image `data` (PNG, JPEG, GIF or WebP) as a PNG file in
/// the image directory and returns its path.
pub fn store_encoded(data: &[u8]) -> Result<PathBuf, String> {
    let img = image::load_from_memory(data).map_err(|e| format!("Cannot read the image: {e}"))?;
    store_rgba(&img.into_rgba8())
}

/// The stored image with the file name `name`. Only the names of stored
/// images are valid: the SHA-1 in hex and the extension `.png`.
pub fn stored(name: &str) -> Option<PathBuf> {
    let stem = name.strip_suffix(".png")?;
    (stem.len() == 40 && stem.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| dir().map(|d| d.join(name)))
        .flatten()
}

/// Writes `rgba` as a PNG file into `dir`, scaled down to `MAX_EDGE`. The file
/// name is the SHA-1 of the PNG data, so the same image gives the same file.
fn store(dir: &Path, rgba: &RgbaImage) -> Result<PathBuf, String> {
    let (w, h) = rgba.dimensions();
    let scaled;
    let img = if w.max(h) > MAX_EDGE {
        let f = f64::from(MAX_EDGE) / f64::from(w.max(h));
        let size = |n: u32| ((f64::from(n) * f).round() as u32).max(1);
        scaled = image::imageops::resize(rgba, size(w), size(h), FilterType::Triangle);
        &scaled
    } else {
        rgba
    };
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("Cannot encode the image: {e}"))?;
    let path = dir.join(format!("{}.png", sha1_smol::Sha1::from(&png).digest()));
    crate::files::write_atomic(&path, &png)
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(path)
}

/// The content of the PNG file `path`, encoded as base64.
pub fn base64_png(path: &Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(data))
}

/// Removes the stored images that no stored chat uses.
pub fn prune(chats: &[SavedChat]) {
    let used: HashSet<&PathBuf> = chats
        .iter()
        .flat_map(|c| &c.messages)
        .flat_map(|m| &m.images)
        .collect();
    let Some(entries) = dir().and_then(|d| std::fs::read_dir(d).ok()) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "png") && !used.contains(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_stored_names_are_valid() {
        let name = format!("{}.png", "ab".repeat(20));
        assert!(stored(&name).is_some_and(|p| p.ends_with(&name)));
        assert!(stored("../config.png").is_none());
        assert!(stored(&format!("{}.png", "x".repeat(40))).is_none());
        assert!(stored(&"ab".repeat(20)).is_none());
    }

    #[test]
    fn large_images_are_scaled_down() {
        let dir = std::env::temp_dir().join(format!("pixelpal-img-{}", std::process::id()));
        let path = store(&dir, &RgbaImage::new(4000, 1000)).unwrap();
        let img = image::open(&path).unwrap();
        assert_eq!((img.width(), img.height()), (MAX_EDGE, 500));
        assert!(base64_png(&path).unwrap().starts_with("iVBORw0KGgo"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
