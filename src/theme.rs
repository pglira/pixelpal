//! Colors and font of the chat panel.

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Style, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

/// Name of the font family of the chat panel.
const CHAT_FAMILY: &str = "chat";
const CHAT_FONT: &str = "chat_font";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatTheme {
    /// Dark or light widget style.
    pub dark: bool,
    pub background: [u8; 3],
    pub text: [u8; 3],
    pub input_background: [u8; 3],
    pub user_message: [u8; 3],
    /// Selection, links and focus.
    pub accent: [u8; 3],
    /// A TTF or OTF file. Empty means the built-in font.
    pub font_file: String,
    /// Size of the body text in points.
    pub font_size: f32,
    /// Opacity of the backgrounds of the chat panel, from 0 to 1. The text
    /// stays opaque.
    pub opacity: f32,
}

impl Default for ChatTheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl ChatTheme {
    pub fn dark() -> Self {
        Self {
            dark: true,
            background: [27, 27, 27],
            text: [215, 215, 215],
            input_background: [12, 12, 12],
            user_message: [45, 45, 52],
            accent: [70, 140, 210],
            font_file: String::new(),
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            background: [248, 248, 246],
            text: [30, 30, 30],
            input_background: [255, 255, 255],
            user_message: [228, 234, 244],
            accent: [40, 110, 200],
            font_file: String::new(),
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// The style of the chat panel.
    pub fn style(&self, base: &Style) -> Style {
        let rgb = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
        let alpha = (self.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let rgba = |c: [u8; 3]| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], alpha);
        let mut style = base.clone();
        let mut v = if self.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        let bg = rgba(self.background);
        let accent = rgb(self.accent);
        v.panel_fill = bg;
        v.window_fill = bg;
        v.override_text_color = Some(rgb(self.text));
        v.extreme_bg_color = rgba(self.input_background);
        v.text_edit_bg_color = Some(rgba(self.input_background));
        v.code_bg_color = rgba(self.input_background);
        v.faint_bg_color = rgba(self.user_message);
        v.hyperlink_color = accent;
        v.selection.bg_fill = accent.gamma_multiply(0.6);
        v.selection.stroke.color = accent;
        v.widgets.hovered.bg_stroke.color = accent;
        v.widgets.active.bg_stroke.color = accent;
        style.visuals = v;

        let family = FontFamily::Name(CHAT_FAMILY.into());
        let size = self.font_size;
        style.text_styles = [
            (TextStyle::Small, FontId::new(size * 0.75, family.clone())),
            (TextStyle::Body, FontId::new(size, family.clone())),
            (TextStyle::Button, FontId::new(size, family.clone())),
            (TextStyle::Heading, FontId::new(size * 1.4, family)),
            (
                TextStyle::Monospace,
                FontId::new(size * 0.95, FontFamily::Monospace),
            ),
        ]
        .into();
        style
    }
}

/// Registers the font family of the chat panel with the font of `font_file`.
/// The family falls back to the built-in fonts if the file cannot be read.
/// Returns an error message if the file cannot be read.
pub fn set_fonts(ctx: &egui::Context, font_file: &str) -> Result<(), String> {
    let mut defs = FontDefinitions::default();
    let mut chain = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut result = Ok(());
    let path = font_file.trim();
    if !path.is_empty() {
        match std::fs::read(path) {
            Ok(bytes) => {
                defs.font_data
                    .insert(CHAT_FONT.to_owned(), Arc::new(FontData::from_owned(bytes)));
                chain.insert(0, CHAT_FONT.to_owned());
            }
            Err(e) => result = Err(format!("Cannot read the font file: {e}")),
        }
    }
    defs.families
        .insert(FontFamily::Name(CHAT_FAMILY.into()), chain);
    ctx.set_fonts(defs);
    result
}
