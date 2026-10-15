//! The stored last chat.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    /// A tool call of Claude, or a note of pixelpal.
    Note,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
    /// Unix time in seconds.
    pub time: i64,
}

impl Message {
    pub fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            time: chrono::Local::now().timestamp(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LastChat {
    pub session_id: String,
    pub working_dir: Option<PathBuf>,
    pub model: String,
    /// Unix time in seconds of the last change.
    pub updated: i64,
    pub messages: Vec<Message>,
}

impl LastChat {
    fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("pixelpal").join("last_chat.json"))
    }

    pub fn load() -> Option<Self> {
        let s = std::fs::read_to_string(Self::path()?).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = crate::files::write_atomic(&path, s.as_bytes());
        }
    }

    pub fn delete() {
        if let Some(path) = Self::path() {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn updated_text(&self) -> String {
        chrono::DateTime::from_timestamp(self.updated, 0)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "an unknown date".into())
    }
}
