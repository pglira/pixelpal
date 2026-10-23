//! Images of the chat: pasted from the clipboard, stored as PNG files in the
//! `images` directory of the pixelpal data directory, and shown as thumbnails.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use egui::{ColorImage, TextureHandle, TextureOptions};
use image::RgbaImage;
use image::imageops::FilterType;

use crate::history::SavedChat;

/// Longest edge of a stored image, in pixels. A larger image is scaled down.
const MAX_EDGE: u32 = 2000;

fn dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("pixelpal").join("images"))
}

/// Stores the image of the clipboard as a PNG file and returns its path.
/// Returns `Ok(None)` if the clipboard has no image.
pub fn paste() -> Result<Option<PathBuf>, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let data = match clipboard.get_image() {
        Ok(data) => data,
        Err(arboard::Error::ContentNotAvailable) => return Ok(None),
        Err(e) => return Err(format!("Cannot read the image of the clipboard: {e}")),
    };
    let rgba = RgbaImage::from_raw(
        data.width as u32,
        data.height as u32,
        data.bytes.into_owned(),
    )
    .ok_or("The image of the clipboard has an invalid size.")?;
    let dir = dir().ok_or("No data directory.")?;
    store(&dir, &rgba).map(Some)
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

/// The textures of the images, loaded on first use. A file that cannot be
/// loaded has no texture.
#[derive(Default)]
pub struct Thumbnails {
    textures: HashMap<PathBuf, Option<TextureHandle>>,
}

impl Thumbnails {
    pub fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<&TextureHandle> {
        self.textures
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let img = image::open(path).ok()?.into_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                Some(ctx.load_texture(path.display().to_string(), color, TextureOptions::LINEAR))
            })
            .as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
