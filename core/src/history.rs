//! The stored chats. Each chat is one JSON file in the `chats` directory of
//! the pixelpal data directory.

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
    /// PNG files of the images of a prompt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<PathBuf>,
    /// Unix time in seconds.
    pub time: i64,
}

impl Message {
    pub fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            images: Vec::new(),
            time: chrono::Local::now().timestamp(),
        }
    }
}

/// Longest title of a chat, in characters.
const TITLE_CHARS: usize = 70;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedChat {
    /// Name of the file of the chat. It stays the same when `claude` gives the
    /// conversation a new session id.
    #[serde(default)]
    pub id: String,
    pub session_id: String,
    pub working_dir: Option<PathBuf>,
    pub model: String,
    /// Unix time in seconds of the last change.
    pub updated: i64,
    pub messages: Vec<Message>,
}

impl SavedChat {
    fn dir() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("pixelpal").join("chats"))
    }

    /// The file of the single stored chat of older pixelpal versions.
    fn legacy_path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("pixelpal").join("last_chat.json"))
    }

    /// A new, unique chat id.
    pub fn new_id() -> String {
        format!(
            "{}-{:08x}",
            chrono::Local::now().format("%Y%m%d-%H%M%S"),
            fastrand::u32(..)
        )
    }

    fn path(id: &str) -> Option<PathBuf> {
        // An id is a file name, never a path.
        (!id.is_empty() && !id.contains(['/', '\\']) && !id.starts_with('.'))
            .then(|| Self::dir().map(|d| d.join(format!("{id}.json"))))
            .flatten()
    }

    /// Moves the single stored chat of older versions into the chat directory.
    fn migrate() {
        let Some(old) = Self::legacy_path() else {
            return;
        };
        let Some(mut chat) = std::fs::read_to_string(&old)
            .ok()
            .and_then(|s| serde_json::from_str::<SavedChat>(&s).ok())
        else {
            return;
        };
        if chat.id.is_empty() {
            chat.id = Self::new_id();
        }
        if chat.save().is_ok() {
            let _ = std::fs::remove_file(old);
        }
    }

    /// All stored chats, the most recent first.
    pub fn list() -> Vec<SavedChat> {
        Self::migrate();
        let Some(entries) = Self::dir().and_then(|d| std::fs::read_dir(d).ok()) else {
            return Vec::new();
        };
        let mut chats: Vec<SavedChat> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| {
                let mut chat: SavedChat =
                    serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
                chat.id = e.path().file_stem()?.to_string_lossy().into_owned();
                Some(chat)
            })
            .collect();
        chats.sort_by(|a, b| b.updated.cmp(&a.updated));
        chats
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path(&self.id).ok_or(std::io::ErrorKind::InvalidInput)?;
        let s = serde_json::to_string_pretty(self)?;
        crate::files::write_atomic(&path, s.as_bytes())
    }

    pub fn delete(id: &str) {
        if let Some(path) = Self::path(id) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// The first line of the first prompt, shortened. A first prompt with
    /// only images gives "(image)".
    pub fn title(&self) -> String {
        let prompt = self.messages.iter().find(|m| m.role == Role::User);
        let first = prompt
            .and_then(|m| m.text.lines().map(str::trim).find(|l| !l.is_empty()))
            .unwrap_or(match prompt {
                Some(m) if !m.images.is_empty() => "(image)",
                _ => "(no prompt)",
            });
        if first.chars().count() > TITLE_CHARS {
            let short: String = first.chars().take(TITLE_CHARS - 1).collect();
            format!("{short}…")
        } else {
            first.to_owned()
        }
    }

    /// True if `query` occurs in a message of the chat, in any case.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        q.is_empty()
            || self
                .messages
                .iter()
                .any(|m| m.text.to_lowercase().contains(&q))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(texts: &[&str]) -> SavedChat {
        SavedChat {
            id: "x".into(),
            session_id: "s".into(),
            working_dir: None,
            model: "default".into(),
            updated: 0,
            messages: texts.iter().map(|t| Message::new(Role::User, *t)).collect(),
        }
    }

    #[test]
    fn title_is_the_first_prompt_line() {
        assert_eq!(
            chat(&["\n  Hello there \nmore", "b"]).title(),
            "Hello there"
        );
        assert_eq!(chat(&[]).title(), "(no prompt)");
        let long = "a".repeat(100);
        assert_eq!(chat(&[&long]).title().chars().count(), TITLE_CHARS);
    }

    #[test]
    fn search_ignores_case() {
        let c = chat(&["Fix the Parser"]);
        assert!(c.matches("parser") && c.matches("") && !c.matches("lexer"));
    }

    #[test]
    fn ids_are_file_names() {
        assert!(SavedChat::path("../x").is_none());
        assert!(SavedChat::path("").is_none());
        assert!(SavedChat::path(&SavedChat::new_id()).is_some());
    }
}
