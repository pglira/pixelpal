//! Colors and font of the chat panel.

use serde::{Deserialize, Serialize};

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
}
