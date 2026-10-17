//! A global keyboard shortcut: a passive key grab on the X11 root window.
//!
//! A shortcut is a text such as `Super+Shift+C`: modifiers and one key,
//! separated by `+`, in any case.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, Keycode, ModMask};
use x11rb::rust_connection::RustConnection;

/// Time between two checks for new events.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A parsed shortcut: the modifier mask and the keysym of the key.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shortcut {
    mods: u16,
    keysym: u32,
}

/// Parses a shortcut such as `Ctrl+Alt+P`. A shortcut needs at least one
/// modifier, so that it does not take a key from normal typing.
fn parse(spec: &str) -> Result<Shortcut, String> {
    let parts: Vec<&str> = spec.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last().ok_or("empty shortcut")?;
    let mut mask = 0u16;
    for m in mods {
        mask |= match m.to_ascii_lowercase().as_str() {
            "shift" => u16::from(ModMask::SHIFT),
            "ctrl" | "control" => u16::from(ModMask::CONTROL),
            "alt" => u16::from(ModMask::M1),
            "super" | "win" | "meta" => u16::from(ModMask::M4),
            _ => return Err(format!("unknown modifier \"{m}\"")),
        };
    }
    if mask == 0 {
        return Err("the shortcut needs a modifier: Ctrl, Alt, Shift or Super".into());
    }
    let keysym = keysym(key).ok_or_else(|| format!("unknown key \"{key}\""))?;
    Ok(Shortcut { mods: mask, keysym })
}

/// The X11 keysym of a key name: a letter, a digit, F1 to F24, or a named key.
fn keysym(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && (c.is_ascii_alphanumeric() || c.is_ascii_punctuation())
    {
        return Some(c as u32);
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u32>().ok())
        && (1..=24).contains(&n)
    {
        return Some(0xffbe + n - 1);
    }
    Some(match lower.as_str() {
        "space" => 0x20,
        "enter" | "return" => 0xff0d,
        "escape" | "esc" => 0xff1b,
        "tab" => 0xff09,
        "backspace" => 0xff08,
        "insert" => 0xff63,
        "delete" => 0xffff,
        "home" => 0xff50,
        "end" => 0xff57,
        "pageup" => 0xff55,
        "pagedown" => 0xff56,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "pause" => 0xff13,
        _ => return None,
    })
}

/// The keycodes that produce `keysym` on the current keyboard layout.
fn keycodes(conn: &RustConnection, keysym: u32) -> Result<Vec<Keycode>, String> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = conn
        .get_keyboard_mapping(min, max - min + 1)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?;
    let per = map.keysyms_per_keycode.max(1) as usize;
    Ok(map
        .keysyms
        .chunks(per)
        .enumerate()
        .filter(|(_, syms)| syms.contains(&keysym))
        .map(|(i, _)| min + i as u8)
        .collect())
}

/// A grabbed global shortcut. The grab ends when this value is dropped.
pub struct Hotkey {
    stop: Arc<AtomicBool>,
    rx: Receiver<()>,
}

impl Hotkey {
    /// Grabs the shortcut `spec`. Each press of it sends one event and
    /// repaints `ctx`. Fails if the text is not a shortcut or another program
    /// has the shortcut.
    pub fn grab(spec: &str, ctx: egui::Context) -> Result<Self, String> {
        let shortcut = parse(spec)?;
        let (conn, screen) = x11rb::connect(None).map_err(|e| e.to_string())?;
        let root = conn.setup().roots[screen].root;
        let codes = keycodes(&conn, shortcut.keysym)?;
        if codes.is_empty() {
            return Err("the keyboard layout has no such key".into());
        }
        // Caps Lock and Num Lock must not block the shortcut.
        let lock = u16::from(ModMask::LOCK);
        let num = u16::from(ModMask::M2);
        for code in codes {
            for extra in [0, lock, num, lock | num] {
                conn.grab_key(
                    true,
                    root,
                    ModMask::from(shortcut.mods | extra),
                    code,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )
                .map_err(|e| e.to_string())?
                .check()
                .map_err(|_| format!("another program uses {spec}"))?;
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let thread_stop = stop.clone();
        std::thread::spawn(move || {
            // A held key repeats as a release and a press with the same time.
            // Only the first press counts.
            let mut release_time = None;
            while !thread_stop.load(Ordering::Relaxed) {
                loop {
                    match conn.poll_for_event() {
                        Ok(Some(Event::KeyPress(e))) => {
                            if release_time != Some(e.time) && tx.send(()).is_ok() {
                                ctx.request_repaint();
                            }
                        }
                        Ok(Some(Event::KeyRelease(e))) => release_time = Some(e.time),
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(_) => return,
                    }
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        });
        Ok(Self { stop, rx })
    }

    /// True if the shortcut was pressed since the last call.
    pub fn pressed(&self) -> bool {
        self.rx.try_iter().count() > 0
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        let s = parse("Super+Shift+C").unwrap();
        assert_eq!(s.mods, u16::from(ModMask::M4 | ModMask::SHIFT));
        assert_eq!(s.keysym, 'c' as u32);
        assert_eq!(parse("ctrl + alt + F12").unwrap().keysym, 0xffc9);
        assert_eq!(parse("Alt+Space").unwrap().keysym, 0x20);
        assert!(parse("C").is_err());
        assert!(parse("Hyper+C").is_err());
        assert!(parse("Ctrl+Nokey").is_err());
    }
}
