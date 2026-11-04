//! pixelpal-chat: the chat panel, the menu of the pal and the settings
//! window of pixelpal, as web pages in Tauri windows.
//!
//! The pal process starts this program. The messages of the pal process come
//! on stdin, and the messages to the pal process go to stdout, one JSON line
//! each (see `pixelpal_core::ipc`). The end of stdin ends the program.

mod markdown;
mod screen_color;
mod session;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use base64::Engine as _;
use pixelpal_core::config::{Config, Permission, READ_ONLY_TOOLS};
use pixelpal_core::history::{Message, Role, SavedChat};
use pixelpal_core::ipc::{self, Bounds, ToChat, ToPal, Window};
use pixelpal_core::picker::{self, Pick};
use pixelpal_core::progress::Progress;
use pixelpal_core::theme::ChatTheme;
use pixelpal_core::{images, theme};
use serde::Serialize;
use tauri::http::{Response, StatusCode};
use tauri::image::Image;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::session::Session;

/// Labels of the Tauri windows.
const CHAT: &str = "chat";
const MENU: &str = "menu";
const SETTINGS: &str = "settings";
/// Smallest size of the chat panel, in pixels.
const PANEL_MIN: (f64, f64) = (280.0, 240.0);
/// Size of the menu of the pal, in pixels.
const MENU_SIZE: (f64, f64) = (170.0, 156.0);
/// Shortest time between two updates of a streamed reply in the page.
const STREAM_INTERVAL: Duration = Duration::from_millis(50);

/// A running resize drag of the chat panel: the size at its start.
#[derive(Clone, Copy)]
struct Resize {
    width: f32,
    height: f32,
}

/// The state of the chat process.
struct Shared {
    cfg: Config,
    /// The error of the configuration file at the start.
    cfg_error: Option<String>,
    session: Session,
    /// The window of the pal and the usable area of its monitor.
    anchor: Option<(Bounds, Bounds)>,
    panel_open: bool,
    menu_open: bool,
    menu_had_focus: bool,
    chat_key_error: Option<String>,
    pal_key_error: Option<String>,
    /// The path of the pal program, for the autostart entry.
    pal_exe: PathBuf,
    icon: Option<Image<'static>>,
    resize: Option<Resize>,
    /// Messages that changed since the page got them.
    dirty: BTreeSet<usize>,
    /// True if the state of the chat panel changed since the page got it.
    state_dirty: bool,
    /// A counter that changes the URL of the font file after each change of
    /// the settings, so that the page loads the font again.
    font_version: u32,
}

struct App {
    shared: Mutex<Shared>,
    /// Wakes the thread that sends the changes to the page.
    changed: Condvar,
}

type AppState = Arc<App>;

impl App {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Sends `msg` to the pal process.
fn to_pal(msg: ToPal) {
    let _ = ipc::send(&mut std::io::stdout().lock(), &msg);
}

/// A message as the page shows it.
#[derive(Clone, Serialize)]
struct MessageView {
    role: Role,
    /// The text as HTML: Markdown for a reply, escaped text for the others.
    html: String,
    /// URLs of the images of a prompt.
    images: Vec<String>,
}

fn image_url(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    format!("pximg://localhost/{}", name.unwrap_or_default())
}

fn message_view(m: &Message) -> MessageView {
    let html = match m.role {
        Role::Assistant => markdown::to_html(&m.text),
        _ => markdown::escape(&m.text),
    };
    MessageView {
        role: m.role,
        html,
        images: m.images.iter().map(|p| image_url(p)).collect(),
    }
}

/// The colors and the font of the chat panel as CSS values.
#[derive(Clone, Serialize)]
struct ThemeView {
    dark: bool,
    background: String,
    text: String,
    input_background: String,
    user_message: String,
    accent: String,
    font_size: f32,
    /// The URL of the font file, or `None` for the default font.
    font_url: Option<String>,
}

fn theme_view(t: &ChatTheme, font_version: u32) -> ThemeView {
    let alpha = t.opacity.clamp(0.0, 1.0);
    let rgba = |c: [u8; 3]| format!("rgba({}, {}, {}, {alpha})", c[0], c[1], c[2]);
    let rgb = |c: [u8; 3]| format!("rgb({}, {}, {})", c[0], c[1], c[2]);
    ThemeView {
        dark: t.dark,
        background: rgba(t.background),
        text: rgb(t.text),
        input_background: rgba(t.input_background),
        user_message: rgba(t.user_message),
        accent: rgb(t.accent),
        font_size: t.font_size,
        font_url: (font_error(&t.font_file).is_none() && !t.font_file.trim().is_empty())
            .then(|| format!("pxfont://localhost/font?v={font_version}")),
    }
}

/// The error of the font file `path`, or `None` for a valid font or an
/// empty path.
fn font_error(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return Some(format!("Cannot read the font file: {e}")),
    };
    let magic: [u8; 4] = data
        .get(..4)
        .and_then(|m| m.try_into().ok())
        .unwrap_or([0; 4]);
    let valid = matches!(&magic, b"\x00\x01\x00\x00" | b"OTTO" | b"true" | b"ttcf");
    (!valid).then(|| "The font file is not a valid TTF or OTF font.".to_owned())
}

/// The state of the chat panel, without the messages.
#[derive(Clone, Serialize)]
struct StateView {
    running: bool,
    /// URLs of the images for the next prompt.
    attachments: Vec<String>,
    /// The working directory of the next prompt.
    dir: String,
    model: String,
    models: Vec<String>,
    chat_id: Option<String>,
}

fn state_view(s: &Shared) -> StateView {
    StateView {
        running: s.session.is_running(),
        attachments: s.session.attachments.iter().map(|p| image_url(p)).collect(),
        dir: s
            .session
            .prompt_dir(&s.cfg)
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
        model: s.cfg.model.clone(),
        models: s.cfg.models.clone(),
        chat_id: s.session.chat_id.clone(),
    }
}

/// The whole chat panel.
#[derive(Clone, Serialize)]
struct ChatView {
    messages: Vec<MessageView>,
    state: StateView,
    theme: ThemeView,
}

#[derive(Clone, Serialize)]
struct MessageUpdate {
    index: usize,
    message: MessageView,
}

/// All messages and the state of the chat panel. The page then has all
/// changes up to now.
fn chat_view_of(s: &mut Shared) -> ChatView {
    s.dirty.clear();
    s.state_dirty = false;
    ChatView {
        messages: s.session.messages.iter().map(message_view).collect(),
        state: state_view(s),
        theme: theme_view(&s.cfg.theme, s.font_version),
    }
}

/// Sends all messages and the state to the chat panel.
fn emit_reset(app: &AppHandle, s: &mut Shared) {
    let view = chat_view_of(s);
    let _ = app.emit_to(CHAT, "reset", view);
}

/// Handles the events of `claude`. Runs on the reader thread of `claude`.
fn on_claude_event(state: &App) {
    let mut s = state.lock();
    let cfg = s.cfg.clone();
    let changes = s.session.poll(&cfg);
    if changes.messages.is_empty() && changes.tools.is_empty() && changes.turn_end.is_none() {
        return;
    }
    s.dirty.extend(changes.messages);
    for activity in changes.tools {
        to_pal(ToPal::Tool { activity });
    }
    if let Some(success) = changes.turn_end {
        to_pal(ToPal::TurnEnd { success });
        s.state_dirty = true;
    }
    drop(s);
    state.changed.notify_one();
}

/// Sends the changed messages and the changed state to the chat panel, at
/// most once per `STREAM_INTERVAL`.
fn stream_changes(app: AppHandle, state: AppState) {
    loop {
        let mut s = state.lock();
        while s.dirty.is_empty() && !s.state_dirty {
            s = state.changed.wait(s).unwrap_or_else(|e| e.into_inner());
        }
        let updates: Vec<MessageUpdate> = std::mem::take(&mut s.dirty)
            .into_iter()
            .filter_map(|i| {
                Some(MessageUpdate {
                    index: i,
                    message: message_view(s.session.messages.get(i)?),
                })
            })
            .collect();
        let view = std::mem::take(&mut s.state_dirty).then(|| state_view(&s));
        drop(s);
        for u in updates {
            let _ = app.emit_to(CHAT, "message", u);
        }
        if let Some(v) = view {
            let _ = app.emit_to(CHAT, "state", v);
        }
        std::thread::sleep(STREAM_INTERVAL);
    }
}

/// The place of the chat panel: centered above the pal, inside the area.
fn panel_rect(s: &Shared) -> Bounds {
    let (w, h) = (
        s.cfg.panel_width.round() as i32,
        s.cfg.panel_height.round() as i32,
    );
    let (pal, area) = s.anchor.unwrap_or((
        Bounds {
            x: 640,
            y: 700,
            w: 0,
            h: 0,
        },
        Bounds {
            x: 0,
            y: 0,
            w: 1280,
            h: 720,
        },
    ));
    let x = (pal.x + pal.w / 2 - w / 2).clamp(area.x, (area.x + area.w - w).max(area.x));
    let y = (pal.y - h - 4).max(area.y);
    Bounds { x, y, w, h }
}

fn place_panel(win: &WebviewWindow, s: &Shared) {
    let r = panel_rect(s);
    let _ = win.set_size(PhysicalSize::new(r.w.max(1) as u32, r.h.max(1) as u32));
    let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
}

fn show_panel(app: &AppHandle, state: &App) {
    let Some(win) = app.get_webview_window(CHAT) else {
        return;
    };
    let mut s = state.lock();
    place_panel(&win, &s);
    let _ = win.show();
    let _ = win.set_focus();
    if !s.panel_open {
        s.panel_open = true;
        to_pal(ToPal::Window {
            window: Window::Chat,
            open: true,
        });
    }
    drop(s);
    let _ = app.emit_to(CHAT, "focus", ());
}

fn hide_panel(app: &AppHandle, state: &App) {
    if let Some(win) = app.get_webview_window(CHAT) {
        let _ = win.hide();
    }
    let mut s = state.lock();
    s.resize = None;
    if s.panel_open {
        s.panel_open = false;
        to_pal(ToPal::Window {
            window: Window::Chat,
            open: false,
        });
    }
}

fn close_window(app: &AppHandle, label: &str) {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.destroy();
    }
}

/// Builds a window of this program.
fn window<'a>(
    app: &'a AppHandle,
    label: &str,
    page: &str,
    title: &str,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into())).title(title)
}

/// Gives the window `win` the icon of the pal.
fn set_icon(win: &WebviewWindow, icon: Option<Image<'static>>) {
    if let Some(icon) = icon {
        let _ = win.set_icon(icon);
    }
}

/// Builds the hidden menu window. The menu window stays for the run of the
/// program, so that a right-click only moves and shows it.
fn build_menu<R: tauri::Runtime, M: Manager<R>>(app: &M) -> tauri::Result<WebviewWindow<R>> {
    WebviewWindowBuilder::new(app, MENU, WebviewUrl::App("menu.html".into()))
        .title(ipc::MENU_TITLE)
        .inner_size(MENU_SIZE.0, MENU_SIZE.1)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()
}

fn open_menu(app: &AppHandle, state: &App, x: i32, y: i32) {
    let win = match app.get_webview_window(MENU) {
        Some(win) => win,
        None => match build_menu(app) {
            Ok(win) => {
                set_icon(&win, state.lock().icon.clone());
                win
            }
            Err(e) => {
                eprintln!("pixelpal-chat: cannot open the menu: {e}");
                return;
            }
        },
    };
    let mut s = state.lock();
    s.menu_had_focus = false;
    let panel_open = s.panel_open;
    let was_open = std::mem::replace(&mut s.menu_open, true);
    drop(s);
    let _ = app.emit_to(MENU, "menu", panel_open);
    let _ = win.set_position(PhysicalPosition::new(x, y - MENU_SIZE.1 as i32));
    let _ = win.show();
    let _ = win.set_focus();
    if !was_open {
        to_pal(ToPal::Window {
            window: Window::Menu,
            open: true,
        });
    }
}

/// Hides the menu window.
fn close_menu(app: &AppHandle, state: &App) {
    if let Some(win) = app.get_webview_window(MENU) {
        let _ = win.hide();
    }
    if std::mem::replace(&mut state.lock().menu_open, false) {
        to_pal(ToPal::Window {
            window: Window::Menu,
            open: false,
        });
    }
}

fn open_settings_window(app: &AppHandle, state: &App) {
    if let Some(win) = app.get_webview_window(SETTINGS) {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let icon = state.lock().icon.clone();
    let built = window(app, SETTINGS, "settings.html", ipc::SETTINGS_TITLE)
        .inner_size(580.0, 720.0)
        .min_inner_size(420.0, 300.0)
        .always_on_top(true)
        .build();
    match built {
        Ok(win) => {
            set_icon(&win, icon);
            let _ = win.set_focus();
            to_pal(ToPal::Window {
                window: Window::Settings,
                open: true,
            });
        }
        Err(e) => eprintln!("pixelpal-chat: cannot open the settings: {e}"),
    }
}

/// Stops the running prompt and ends the program.
fn quit(app: &AppHandle, state: &App) {
    state.lock().session.stop();
    app.exit(0);
}

/// Reads the messages of the pal process until the end of stdin.
fn read_pal(app: AppHandle, state: AppState) {
    let mut input = std::io::stdin().lock();
    while let Some(msg) = ipc::receive::<ToChat>(&mut input) {
        let app = app.clone();
        let state = state.clone();
        // Window functions run on the main thread.
        let _ = app
            .clone()
            .run_on_main_thread(move || handle_pal(&app, &state, msg));
    }
    let _ = app.clone().run_on_main_thread(move || quit(&app, &state));
}

fn handle_pal(app: &AppHandle, state: &App, msg: ToChat) {
    match msg {
        ToChat::TogglePanel => {
            close_menu(app, state);
            if state.lock().panel_open {
                hide_panel(app, state);
            } else {
                show_panel(app, state);
            }
        }
        ToChat::HidePanel => {
            close_menu(app, state);
            hide_panel(app, state);
        }
        ToChat::Anchor { pal, area } => {
            let mut s = state.lock();
            s.anchor = Some((pal, area));
            if s.panel_open
                && s.resize.is_none()
                && let Some(win) = app.get_webview_window(CHAT)
            {
                place_panel(&win, &s);
            }
        }
        ToChat::OpenSettings => open_settings_window(app, state),
        ToChat::Menu { x, y } => open_menu(app, state, x, y),
        ToChat::Shortcuts {
            chat_error,
            pal_error,
        } => {
            let mut s = state.lock();
            s.chat_key_error = chat_error;
            s.pal_key_error = pal_error;
            let view = settings_view(&s);
            drop(s);
            let _ = app.emit_to(SETTINGS, "settings", view);
        }
        ToChat::Icon {
            width,
            height,
            rgba,
        } => {
            let Ok(data) = base64::engine::general_purpose::STANDARD.decode(rgba) else {
                return;
            };
            if data.len() != (width * height * 4) as usize {
                return;
            }
            let icon = Image::new_owned(data, width, height);
            for win in app.webview_windows().values() {
                let _ = win.set_icon(icon.clone());
            }
            state.lock().icon = Some(icon);
        }
        ToChat::Quit => quit(app, state),
    }
}

// ---- Commands of the chat panel ----

#[tauri::command]
fn chat_view(state: State<'_, AppState>) -> ChatView {
    chat_view_of(&mut state.lock())
}

#[tauri::command]
fn send(text: String, state: State<'_, AppState>) -> bool {
    let mut s = state.lock();
    let cfg = s.cfg.clone();
    let state2 = state.inner().clone();
    let sent = s
        .session
        .send(&text, &cfg, move || on_claude_event(&state2));
    if sent {
        if s.session.is_running() {
            to_pal(ToPal::Running { running: true });
        }
        let i = s.session.messages.len().saturating_sub(1);
        s.dirty.insert(i);
        if i > 0 {
            // A failed start adds the prompt and an error.
            s.dirty.insert(i - 1);
        }
        s.state_dirty = true;
        drop(s);
        state.changed.notify_one();
    }
    sent
}

#[tauri::command]
fn stop(state: State<'_, AppState>) {
    state.lock().session.stop();
}

/// Ends a running prompt without its result, for a change of the chat.
fn drop_run(s: &mut Shared) {
    if s.session.is_running() {
        to_pal(ToPal::Running { running: false });
    }
}

#[tauri::command]
fn new_chat(app: AppHandle, state: State<'_, AppState>) {
    let mut s = state.lock();
    drop_run(&mut s);
    s.session.new_chat();
    emit_reset(&app, &mut s);
}

#[tauri::command]
fn set_model(model: String, state: State<'_, AppState>) {
    let mut s = state.lock();
    if s.cfg.models.contains(&model) && s.cfg.model != model {
        s.cfg.model = model;
        s.cfg.save();
        to_pal(ToPal::ConfigChanged);
    }
}

/// A stored chat in the history list.
#[derive(Clone, Serialize)]
struct HistoryItem {
    id: String,
    title: String,
    /// The date, the number of messages and the working directory.
    detail: String,
}

#[tauri::command]
fn history(query: String) -> Vec<HistoryItem> {
    SavedChat::list()
        .into_iter()
        .filter(|c| c.matches(&query))
        .map(|c| {
            let dir = c
                .working_dir
                .as_ref()
                .map(|d| d.display().to_string())
                .unwrap_or_default();
            HistoryItem {
                title: c.title(),
                detail: format!(
                    "{} · {} messages · {dir}",
                    c.updated_text(),
                    c.messages.len()
                ),
                id: c.id,
            }
        })
        .collect()
}

#[tauri::command]
fn open_chat(id: String, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut s = state.lock();
    if s.session.is_running() {
        return Err("Stop the running prompt first.".into());
    }
    let chat = SavedChat::list()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or("The chat is not stored any more.")?;
    s.session.open(chat);
    emit_reset(&app, &mut s);
    Ok(())
}

#[tauri::command]
fn delete_chat(id: String, state: State<'_, AppState>) {
    let mut s = state.lock();
    s.session.delete(&id);
    s.state_dirty = true;
    drop(s);
    state.changed.notify_one();
}

/// Attaches an image that the page read from the clipboard: an encoded image
/// file in base64.
#[tauri::command]
fn attach_image(data: String, state: State<'_, AppState>) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("Cannot read the image: {e}"))?;
    let path = images::store_encoded(&bytes)?;
    let mut s = state.lock();
    s.session.attach(path);
    s.state_dirty = true;
    drop(s);
    state.changed.notify_one();
    Ok(())
}

/// Attaches the image of the clipboard. Returns false if the clipboard has
/// no image.
#[tauri::command]
fn paste_image(state: State<'_, AppState>) -> Result<bool, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let data = match clipboard.get_image() {
        Ok(data) => data,
        Err(arboard::Error::ContentNotAvailable) => return Ok(false),
        Err(e) => return Err(format!("Cannot read the image of the clipboard: {e}")),
    };
    let rgba = image::RgbaImage::from_raw(
        data.width as u32,
        data.height as u32,
        data.bytes.into_owned(),
    )
    .ok_or("The image of the clipboard has an invalid size.")?;
    let path = images::store_rgba(&rgba)?;
    let mut s = state.lock();
    s.session.attach(path);
    s.state_dirty = true;
    drop(s);
    state.changed.notify_one();
    Ok(true)
}

/// Puts `text` on the clipboard. The clipboard handle stays alive, because
/// on X11 the program that owns the clipboard must serve its content.
#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);
    let mut slot = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    slot.as_mut()
        .map_or(Ok(()), |c| c.set_text(text))
        .map_err(|e| format!("Cannot copy the text: {e}"))
}

#[tauri::command]
fn remove_attachment(index: usize, state: State<'_, AppState>) {
    let mut s = state.lock();
    if index < s.session.attachments.len() {
        s.session.attachments.remove(index);
    }
    s.state_dirty = true;
    drop(s);
    state.changed.notify_one();
}

#[tauri::command]
fn close_panel(app: AppHandle, state: State<'_, AppState>) {
    hide_panel(&app, &state);
}

#[tauri::command]
fn open_settings(app: AppHandle, state: State<'_, AppState>) {
    open_settings_window(&app, &state);
}

/// Opens an `http`, `https` or `mailto` link in the default program.
#[tauri::command]
fn open_url(url: String) {
    let lower = url.to_ascii_lowercase();
    if ["http://", "https://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[tauri::command]
fn resize_begin(state: State<'_, AppState>) {
    let mut s = state.lock();
    s.resize = Some(Resize {
        width: s.cfg.panel_width,
        height: s.cfg.panel_height,
    });
}

/// Changes the size of the chat panel during a resize drag. `dx` and `dy` are
/// the movement of the pointer since the start of the drag. The panel stays
/// centered above the pal, so a side edge changes the width on both sides.
#[tauri::command]
fn resize_by(
    dx: f32,
    dy: f32,
    left: bool,
    right: bool,
    top: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) {
    let mut s = state.lock();
    let Some(r) = s.resize else { return };
    let (mut w, mut h) = (r.width, r.height);
    if right {
        w += 2.0 * dx;
    }
    if left {
        w -= 2.0 * dx;
    }
    if top {
        h -= dy;
    }
    let area = s.anchor.map(|(_, a)| a);
    let max_w = area
        .map_or(f32::MAX, |a| a.w as f32)
        .max(PANEL_MIN.0 as f32);
    let max_h = area
        .map_or(f32::MAX, |a| a.h as f32)
        .max(PANEL_MIN.1 as f32);
    s.cfg.panel_width = w.clamp(PANEL_MIN.0 as f32, max_w).round();
    s.cfg.panel_height = h.clamp(PANEL_MIN.1 as f32, max_h).round();
    if let Some(win) = app.get_webview_window(CHAT) {
        place_panel(&win, &s);
    }
}

/// Ends a resize drag and stores the new size.
#[tauri::command]
fn resize_end(app: AppHandle, state: State<'_, AppState>) {
    let mut s = state.lock();
    if s.resize.take().is_some() {
        s.cfg.save();
        to_pal(ToPal::ConfigChanged);
        let size = (s.cfg.panel_width, s.cfg.panel_height);
        drop(s);
        let _ = app.emit_to(SETTINGS, "panel_size", size);
    }
}

// ---- Commands of the menu ----

#[tauri::command]
fn menu_view(state: State<'_, AppState>) -> bool {
    state.lock().panel_open
}

#[tauri::command]
fn menu_action(action: String, app: AppHandle, state: State<'_, AppState>) {
    close_menu(&app, &state);
    match action.as_str() {
        "chat" => {
            if state.lock().panel_open {
                hide_panel(&app, &state);
            } else {
                show_panel(&app, &state);
            }
        }
        "settings" => open_settings_window(&app, &state),
        "hide" => to_pal(ToPal::HidePal),
        "quit" => to_pal(ToPal::Quit),
        _ => {}
    }
}

// ---- Commands of the settings window ----

/// The settings window: the configuration and the information around it.
#[derive(Clone, Serialize)]
struct SettingsView {
    config: Config,
    dark_theme: ChatTheme,
    light_theme: ChatTheme,
    /// The labels of the permission presets, by their names in the file.
    permissions: Vec<(Permission, &'static str)>,
    read_only_tools: Vec<&'static str>,
    progress: String,
    config_path: String,
    /// The error of the configuration file at the start.
    status: Option<String>,
    font_error: Option<String>,
    chat_key_error: Option<String>,
    pal_key_error: Option<String>,
}

fn settings_view(s: &Shared) -> SettingsView {
    SettingsView {
        config: s.cfg.clone(),
        dark_theme: theme::ChatTheme::dark(),
        light_theme: theme::ChatTheme::light(),
        permissions: Permission::ALL.iter().map(|p| (*p, p.label())).collect(),
        read_only_tools: READ_ONLY_TOOLS.to_vec(),
        progress: Progress::load().summary(),
        config_path: Config::path()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        status: s.cfg_error.clone(),
        font_error: font_error(&s.cfg.theme.font_file),
        chat_key_error: s.chat_key_error.clone(),
        pal_key_error: s.pal_key_error.clone(),
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> SettingsView {
    settings_view(&state.lock())
}

/// Applies and stores the configuration `config`. Returns the new state of
/// the settings window, or the error that stopped the change.
#[tauri::command]
fn apply_settings(
    config: Config,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SettingsView, String> {
    let mut draft = config;
    draft.sanitize();
    let mut s = state.lock();
    if draft.autostart != s.cfg.autostart {
        draft
            .apply_autostart(&s.pal_exe)
            .map_err(|e| format!("Autostart: {e}"))?;
    }
    // The model selector of the chat panel can change the model while the
    // settings window is open.
    if draft.models.contains(&s.cfg.model) {
        draft.model = s.cfg.model.clone();
    } else if !draft.models.contains(&draft.model) {
        draft.model = draft
            .models
            .first()
            .cloned()
            .unwrap_or_else(|| "default".into());
    }
    s.cfg = draft;
    s.cfg.save();
    s.cfg_error = None;
    s.font_version += 1;
    to_pal(ToPal::ConfigChanged);
    if s.panel_open
        && let Some(win) = app.get_webview_window(CHAT)
    {
        place_panel(&win, &s);
    }
    let _ = app.emit_to(CHAT, "theme", theme_view(&s.cfg.theme, s.font_version));
    s.state_dirty = true;
    let view = settings_view(&s);
    drop(s);
    state.changed.notify_one();
    Ok(view)
}

/// Opens a file or folder dialog and returns the chosen path, or `None` for
/// a cancel.
#[tauri::command]
async fn pick(kind: String, start: String) -> Result<Option<String>, String> {
    let pick = if kind == "font" {
        Pick::Font
    } else {
        Pick::Folder
    };
    let start = if start.trim().is_empty() {
        match pick {
            Pick::Font => PathBuf::from("/usr/share/fonts"),
            Pick::Folder => dirs::home_dir().unwrap_or_default(),
        }
    } else {
        let p = PathBuf::from(start.trim());
        match pick {
            Pick::Font => p.parent().map(Path::to_path_buf).unwrap_or(p),
            Pick::Folder => p,
        }
    };
    tauri::async_runtime::spawn_blocking(move || picker::pick(pick, &start))
        .await
        .map_err(|e| e.to_string())?
}

/// Lets the user click a pixel on the screen and returns its color, or
/// `None` for a cancel.
#[tauri::command]
async fn pick_screen_color() -> Result<Option<[u8; 3]>, String> {
    tauri::async_runtime::spawn_blocking(screen_color::pick)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn close_settings(app: AppHandle) {
    close_window(&app, SETTINGS);
}

/// A response with the file `path`, or 404 if it cannot be read.
fn file_response(path: Option<PathBuf>, mime: &str) -> Response<Vec<u8>> {
    match path.and_then(|p| std::fs::read(p).ok()) {
        Some(data) => Response::builder()
            .header("Content-Type", mime)
            .body(data)
            .unwrap_or_default(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap_or_default(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pal_exe = args
        .iter()
        .position(|a| a == "--pal-exe")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("pixelpal"));
    let (cfg, cfg_error) = Config::load();
    images::prune(&SavedChat::list());
    let state: AppState = Arc::new(App {
        shared: Mutex::new(Shared {
            cfg,
            cfg_error,
            session: Session::default(),
            anchor: None,
            panel_open: false,
            menu_open: false,
            menu_had_focus: false,
            chat_key_error: None,
            pal_key_error: None,
            pal_exe,
            icon: None,
            resize: None,
            dirty: BTreeSet::new(),
            state_dirty: false,
            font_version: 0,
        }),
        changed: Condvar::new(),
    });

    let app = tauri::Builder::default()
        .manage(state.clone())
        // The images of the chats, by file name.
        .register_uri_scheme_protocol("pximg", |_ctx, req| {
            let name = req.uri().path().trim_start_matches('/');
            file_response(images::stored(name), "image/png")
        })
        // The font file of the theme.
        .register_uri_scheme_protocol("pxfont", |ctx, _req| {
            let state = ctx.app_handle().state::<AppState>();
            let file = state.lock().cfg.theme.font_file.trim().to_owned();
            let path =
                (!file.is_empty() && font_error(&file).is_none()).then(|| PathBuf::from(&file));
            let mime = if file.to_ascii_lowercase().ends_with(".otf") {
                "font/otf"
            } else {
                "font/ttf"
            };
            file_response(path, mime)
        })
        .invoke_handler(tauri::generate_handler![
            chat_view,
            send,
            stop,
            new_chat,
            set_model,
            history,
            open_chat,
            delete_chat,
            attach_image,
            paste_image,
            copy_text,
            remove_attachment,
            close_panel,
            open_settings,
            open_url,
            resize_begin,
            resize_by,
            resize_end,
            menu_view,
            menu_action,
            get_settings,
            apply_settings,
            pick,
            pick_screen_color,
            close_settings,
        ])
        .on_window_event(|win, event| {
            let app = win.app_handle();
            let state = app.state::<AppState>();
            match (win.label(), event) {
                // The window manager can close the panel: it then hides.
                (CHAT, WindowEvent::CloseRequested { api, .. }) => {
                    api.prevent_close();
                    hide_panel(app, &state);
                }
                // Other programs can maximize or resize the panel, for example
                // a window rule that matches its title. The panel then takes
                // its own size and place again.
                (CHAT, WindowEvent::Resized(size)) => {
                    let s = state.lock();
                    let r = panel_rect(&s);
                    let own = (size.width as i32, size.height as i32) == (r.w, r.h);
                    if s.panel_open
                        && s.resize.is_none()
                        && !own
                        && let Some(w) = app.get_webview_window(CHAT)
                    {
                        if w.is_maximized().unwrap_or(false) {
                            let _ = w.unmaximize();
                        }
                        place_panel(&w, &s);
                    }
                }
                // The menu closes when it loses the focus after it had it.
                (MENU, WindowEvent::Focused(true)) => state.lock().menu_had_focus = true,
                (MENU, WindowEvent::Focused(false)) if state.lock().menu_had_focus => {
                    close_menu(app, &state);
                }
                (MENU, WindowEvent::CloseRequested { api, .. }) => {
                    api.prevent_close();
                    close_menu(app, &state);
                }
                (SETTINGS, WindowEvent::Destroyed) => to_pal(ToPal::Window {
                    window: Window::Settings,
                    open: false,
                }),
                _ => {}
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let s = state.lock();
            let r = panel_rect(&s);
            drop(s);
            WebviewWindowBuilder::new(app, CHAT, WebviewUrl::App("chat.html".into()))
                .title(ipc::CHAT_TITLE)
                .inner_size(r.w as f64, r.h as f64)
                .decorations(false)
                .transparent(true)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .build()?;
            build_menu(app)?;
            let (h2, s2) = (handle.clone(), state.clone());
            std::thread::spawn(move || stream_changes(h2, s2));
            let s3 = state.clone();
            std::thread::spawn(move || read_pal(handle, s3));
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(e) => {
            eprintln!("pixelpal-chat: {e}");
            std::process::exit(1);
        }
    };
    app.run(|_app, event| {
        // The program runs without visible windows. Only `quit` ends it.
        if let RunEvent::ExitRequested {
            api, code: None, ..
        } = event
        {
            api.prevent_exit();
        }
    });
}
