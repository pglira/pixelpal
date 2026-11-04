//! The messages between the pal process and the chat process.
//!
//! The pal process starts the chat process with piped stdin and stdout. Each
//! message is one line of JSON: `ToChat` on the stdin of the chat process,
//! `ToPal` on its stdout.

use std::io::{BufRead, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Title of the chat panel window.
pub const CHAT_TITLE: &str = "pixelpal chat";
/// Title of the menu window of the pal.
pub const MENU_TITLE: &str = "pixelpal menu";
/// Title of the settings window.
pub const SETTINGS_TITLE: &str = "pixelpal settings";

/// A rectangle on the screen, in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// A window of the chat process that the pal process activates when it opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    Chat,
    Menu,
    Settings,
}

impl Window {
    pub const ALL: [Window; 3] = [Window::Chat, Window::Menu, Window::Settings];

    /// The title of the window.
    pub fn title(self) -> &'static str {
        match self {
            Window::Chat => CHAT_TITLE,
            Window::Menu => MENU_TITLE,
            Window::Settings => SETTINGS_TITLE,
        }
    }
}

/// A tool event of the running turn, for the animation of the pal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolActivity {
    Use(String),
    Error,
}

/// A message from the pal process to the chat process.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToChat {
    /// Opens the chat panel if it is closed, and closes it if it is open.
    TogglePanel,
    /// Closes the chat panel and the menu.
    HidePanel,
    /// The window of the pal and the usable area of its monitor. The chat
    /// panel stays centered above the pal, inside the area.
    Anchor {
        pal: Bounds,
        area: Bounds,
    },
    OpenSettings,
    /// Opens the menu of the pal with its bottom-left corner at `(x, y)`.
    Menu {
        x: i32,
        y: i32,
    },
    /// The errors of the global shortcuts, for the settings window.
    Shortcuts {
        chat_error: Option<String>,
        pal_error: Option<String>,
    },
    /// The window icon, as straight RGBA pixels in base64.
    Icon {
        width: u32,
        height: u32,
        rgba: String,
    },
    /// Stops the running prompt and ends the chat process.
    Quit,
}

/// A message from the chat process to the pal process.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToPal {
    /// A window opened or closed.
    Window {
        window: Window,
        open: bool,
    },
    /// A prompt started or ended.
    Running {
        running: bool,
    },
    /// A turn ended with a result or with an error.
    TurnEnd {
        success: bool,
    },
    Tool {
        activity: ToolActivity,
    },
    /// The configuration file changed.
    ConfigChanged,
    /// The user selected "Hide" in the menu of the pal.
    HidePal,
    /// The user selected "Quit" in the menu of the pal.
    Quit,
}

/// Writes `msg` as one line of JSON and flushes the output.
pub fn send<T: Serialize>(out: &mut impl Write, msg: &T) -> std::io::Result<()> {
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    out.write_all(line.as_bytes())?;
    out.flush()
}

/// Reads the next message from `input`. Lines that are not messages are
/// skipped. Returns `None` at the end of the input.
pub fn receive<T: DeserializeOwned>(input: &mut impl BufRead) -> Option<T> {
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if let Ok(msg) = serde_json::from_str(&line) {
            return Some(msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_go_through_one_line_each() {
        let mut buf = Vec::new();
        let msgs = [
            ToPal::Tool {
                activity: ToolActivity::Use("Bash".into()),
            },
            ToPal::Window {
                window: Window::Chat,
                open: true,
            },
            ToPal::ConfigChanged,
        ];
        for m in &msgs {
            send(&mut buf, m).unwrap();
        }
        buf.extend_from_slice(b"not a message\n");
        assert_eq!(buf.iter().filter(|&&b| b == b'\n').count(), 4);
        let mut input = std::io::Cursor::new(buf);
        for m in &msgs {
            assert_eq!(receive::<ToPal>(&mut input).as_ref(), Some(m));
        }
        assert_eq!(receive::<ToPal>(&mut input), None);
    }

    #[test]
    fn a_message_has_a_type_field() {
        let line = serde_json::to_string(&ToChat::Menu { x: 1, y: 2 }).unwrap();
        assert_eq!(line, r#"{"type":"menu","x":1,"y":2}"#);
    }
}
