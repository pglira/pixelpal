//! The pal process: the event loop, the pal window, the enemy window, the
//! tray icon, the global shortcuts, and the link to the chat process.
//!
//! The event loop waits on the X11 connection and on a waker for the other
//! threads (tray, shortcuts, chat process). Each pass handles the new events,
//! moves the pal, draws the windows, and computes the time of the next pass.

use std::os::fd::AsFd;
use std::time::Instant;

use base64::Engine as _;
use pixelpal_core::config::Config;
use pixelpal_core::ipc::{Bounds, ToChat, ToPal, Window};
use pixelpal_core::progress::Progress;
use rustix::event::{PollFd, PollFlags};
use x11rb::protocol::Event;

use crate::canvas::Canvas;
use crate::chat_process::{ChatEvent, ChatProcess};
use crate::enemy::{ENEMY_TITLE, Fight};
use crate::geom::{Pos2, Rect, Vec2, pos2};
use crate::hotkey::Binding;
use crate::link::Sprites;
use crate::pal::{Area, Pal, Senses};
use crate::tray::{Tray, TrayCommand};
use crate::wake::{self, WakeSource, Waker};
use crate::x11::{PxRect, WindowSpec, X11};

/// Title of the pal window.
pub const PAL_TITLE: &str = "pixelpal";
/// Seconds during which pixelpal tries to activate a new window.
const FOCUS_SECONDS: f64 = 3.0;
/// Seconds between two activation requests.
const FOCUS_INTERVAL: f64 = 0.2;
/// Seconds during which a new window must keep the keyboard focus. The window
/// manager can give the focus back to the last window shortly after it
/// activated the new window.
const FOCUS_STABLE: f64 = 0.6;
/// Seconds between two queries of the usable area.
const AREA_INTERVAL: f64 = 2.0;
/// Seconds between two updates of the window states.
const MANAGE_INTERVAL: f64 = 1.0;
/// Distance in pixels that the pointer moves with the left button down before
/// a press becomes a drag.
const DRAG_DISTANCE: f32 = 6.0;
/// Longest press that counts as a click, in seconds.
const CLICK_SECONDS: f64 = 0.8;
/// Longest wait of the event loop, in seconds.
const MAX_WAIT: f64 = 1.0;

/// A window of the pal process and its pixels.
struct SpriteWindow {
    id: u32,
    canvas: Canvas,
    mapped: bool,
    /// The last position sent to the X server.
    pos: Option<(i32, i32)>,
}

impl SpriteWindow {
    fn create(
        x11: &X11,
        title: &str,
        rect: PxRect,
        override_redirect: bool,
    ) -> Result<Self, String> {
        let id = x11.create_window(&WindowSpec {
            title,
            rect,
            override_redirect,
        })?;
        Ok(Self {
            id,
            canvas: Canvas::new(rect.w as u32, rect.h as u32),
            mapped: false,
            pos: Some((rect.x, rect.y)),
        })
    }

    fn set_mapped(&mut self, x11: &X11, mapped: bool) {
        if self.mapped != mapped {
            self.mapped = mapped;
            x11.set_mapped(self.id, mapped);
        }
    }

    fn move_to(&mut self, x11: &X11, pos: Pos2) {
        let p = (pos.x.round() as i32, pos.y.round() as i32);
        if self.pos != Some(p) {
            self.pos = Some(p);
            x11.move_window(self.id, p.0, p.1);
        }
    }

    fn resize(&mut self, x11: &X11, size: Vec2) {
        let (w, h) = (size.x.round() as u32, size.y.round() as u32);
        if (self.canvas.width(), self.canvas.height()) != (w.max(1), h.max(1)) {
            self.canvas = Canvas::new(w, h);
            x11.resize_window(self.id, w as i32, h as i32);
        }
    }

    /// Sends the pixels of the canvas to the window.
    fn present(&self, x11: &X11) {
        self.canvas
            .with_pixels(|p| x11.put_pixels(self.id, self.canvas.width(), p));
    }
}

/// A press of a mouse button on the pal.
struct Press {
    button: u8,
    at: Pos2,
    time: f64,
    /// Pointer position minus pal position, once the press is a drag.
    drag_offset: Option<Vec2>,
}

/// A window of the chat process that pixelpal activates until it keeps the
/// keyboard focus.
struct FocusRequest {
    window: Window,
    /// The time until which pixelpal tries to activate the window.
    until: f64,
    /// The time since which the window has the focus without a break.
    active_since: Option<f64>,
}

pub struct App {
    cfg: Config,
    sprites: Sprites,
    pal: Pal,
    x11: X11,
    area: Area,
    area_time: f64,
    pal_win: SpriteWindow,
    /// The enemy window, created when the first enemy comes.
    enemy_win: Option<SpriteWindow>,
    chat: ChatProcess,
    /// The open windows of the chat process, in the order of `Window::ALL`.
    open: [bool; 3],
    /// True while `claude` runs.
    running: bool,
    /// The last time at which the chat panel was open or `claude` ran.
    chat_active_at: f64,
    tray: Option<Tray>,
    /// The global shortcuts that toggle the chat panel and the pal.
    chat_key: Binding,
    pal_key: Binding,
    chat_key_error: Option<String>,
    pal_key_error: Option<String>,
    /// The shortcut errors that the chat process knows.
    sent_key_errors: Option<(Option<String>, Option<String>)>,
    pal_visible: bool,
    manage_time: f64,
    /// Windows that do not have a stable keyboard focus yet.
    focus_pending: Vec<FocusRequest>,
    focus_time: f64,
    press: Option<Press>,
    progress: Progress,
    /// The last senses of the pal, and the time of their last full query.
    senses: Senses,
    senses_time: f64,
    /// The last input shape of the pal window.
    input_shape: Option<PxRect>,
    fight: Fight,
    /// The last anchor sent to the chat process.
    sent_anchor: Option<(Bounds, Bounds)>,
    start: Instant,
    last_frame: f64,
    waker: Waker,
    wake_source: WakeSource,
    quit: bool,
}

/// The area of a rectangle of the X server.
fn area_of(r: PxRect) -> Area {
    Area {
        left: r.x as f32,
        top: r.y as f32,
        right: (r.x + r.w) as f32,
        bottom: (r.y + r.h) as f32,
    }
}

fn bounds_of(r: Rect) -> Bounds {
    Bounds {
        x: r.left().round() as i32,
        y: r.top().round() as i32,
        w: r.width().round() as i32,
        h: r.height().round() as i32,
    }
}

impl App {
    pub fn new(cfg: Config, x11: X11) -> Result<Self, String> {
        let (waker, wake_source) = wake::pair().map_err(|e| e.to_string())?;
        if !x11.has_compositor() {
            eprintln!(
                "pixelpal: no compositing manager runs: the pal window has a black background."
            );
        }

        // Start on the monitor under the pointer, at a random x position,
        // above the ground. The pal then falls down to the bottom edge.
        let (px, py) = x11.pointer().unwrap_or((0, 0));
        let area = x11.work_area_at(px, py).map(area_of).unwrap_or(Area {
            left: 0.0,
            top: 0.0,
            right: 1280.0,
            bottom: 720.0,
        });
        let size = Pal::window_size(cfg.scale);
        let span = (area.right - area.left - 3.0 * size.x).max(0.0);
        let start = pos2(
            area.left + size.x + fastrand::f32() * span,
            area.bottom - size.y - 120.0,
        )
        .round();
        let pal_win = SpriteWindow::create(
            &x11,
            PAL_TITLE,
            PxRect {
                x: start.x as i32,
                y: start.y as i32,
                w: size.x as i32,
                h: size.y as i32,
            },
            false,
        )?;

        let (w, h, rgba) = crate::link::icon(48);
        let tray = Tray::spawn(waker.clone(), w, h, &rgba)
            .map_err(|e| eprintln!("pixelpal: no tray icon: {e}"))
            .ok();
        let progress = Progress::default();
        progress.save();
        let tier = if cfg.gear_progress {
            progress.tier()
        } else {
            0
        };

        let mut chat = ChatProcess::new(waker.clone());
        let (w, h, rgba) = crate::link::icon(96);
        chat.set_greeting(vec![ToChat::Icon {
            width: w,
            height: h,
            rgba: base64::engine::general_purpose::STANDARD.encode(rgba),
        }]);
        chat.start();

        let mut app = Self {
            sprites: Sprites::load(tier),
            fight: Fight::default(),
            progress,
            pal: Pal::new(start),
            x11,
            area,
            area_time: f64::NEG_INFINITY,
            pal_win,
            enemy_win: None,
            chat,
            open: [false; 3],
            running: false,
            chat_active_at: f64::NEG_INFINITY,
            tray,
            chat_key: Binding::default(),
            pal_key: Binding::default(),
            chat_key_error: None,
            pal_key_error: None,
            sent_key_errors: None,
            pal_visible: true,
            manage_time: f64::NEG_INFINITY,
            focus_pending: Vec::new(),
            focus_time: f64::NEG_INFINITY,
            press: None,
            senses: Senses::default(),
            senses_time: f64::NEG_INFINITY,
            input_shape: None,
            sent_anchor: None,
            start: Instant::now(),
            last_frame: 0.0,
            waker,
            wake_source,
            quit: false,
            cfg,
        };
        app.pal_win.set_mapped(&app.x11, true);
        Ok(app)
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn is_open(&self, w: Window) -> bool {
        self.open[Window::ALL.iter().position(|x| *x == w).unwrap_or(0)]
    }

    /// Runs the event loop until the user quits.
    pub fn run(mut self) {
        loop {
            self.wake_source.drain();
            while let Some(ev) = self.x11.poll_event() {
                self.handle_x_event(ev);
            }
            self.handle_tray();
            self.handle_hotkeys();
            self.handle_chat();
            if let Some(e) = self.x11.lost() {
                eprintln!("pixelpal: the connection to the X server is lost: {e}");
                self.quit = true;
            }
            if self.quit {
                self.chat.send_if_running(&ToChat::Quit);
                return;
            }
            let wait = self.frame();
            // Replies of the frame can bring events into the queue of the
            // connection. They do not make the socket readable again.
            if let Some(ev) = self.x11.poll_event() {
                self.handle_x_event(ev);
                continue;
            }
            let (x_fd, wake_fd) = (self.x11.as_fd(), self.wake_source.as_fd());
            let mut fds = [
                PollFd::new(&x_fd, PollFlags::IN),
                PollFd::new(&wake_fd, PollFlags::IN),
            ];
            let timeout = rustix::event::Timespec {
                tv_sec: wait.trunc() as i64,
                tv_nsec: (wait.fract() * 1e9) as i64,
            };
            let _ = rustix::event::poll(&mut fds, Some(&timeout));
        }
    }

    fn handle_x_event(&mut self, ev: Event) {
        let now = self.now();
        let on_pal = |w: u32| w == self.pal_win.id;
        match ev {
            Event::ButtonPress(e) if on_pal(e.event) && self.press.is_none() => {
                self.press = Some(Press {
                    button: e.detail,
                    at: pos2(e.root_x as f32, e.root_y as f32),
                    time: now,
                    drag_offset: None,
                });
            }
            Event::MotionNotify(e) if on_pal(e.event) => {
                self.handle_motion(pos2(e.root_x as f32, e.root_y as f32));
            }
            Event::ButtonRelease(e)
                if on_pal(e.event) && self.press.as_ref().is_some_and(|p| p.button == e.detail) =>
            {
                let at = pos2(e.root_x as f32, e.root_y as f32);
                self.handle_release(at, now);
            }
            _ => {}
        }
    }

    /// A drag lifts the pal. While the chat panel is open, a drag moves the
    /// pal only along the ground, and the panel follows him.
    fn handle_motion(&mut self, at: Pos2) {
        let panel_open = self.is_open(Window::Chat);
        let Some(p) = &mut self.press else { return };
        if p.button != 1 {
            return;
        }
        if p.drag_offset.is_none() && (at - p.at).length() >= DRAG_DISTANCE {
            p.drag_offset = Some(p.at - self.pal.pos);
            if !panel_open {
                self.pal.hold();
            }
        }
        let Some(offset) = p.drag_offset else { return };
        let pos = at - offset;
        if self.pal.is_held() {
            self.pal.pos = pos;
        } else {
            let max_x = (self.area.right - Pal::window_size(self.cfg.scale).x).max(self.area.left);
            self.pal.pos.x = pos.x.clamp(self.area.left, max_x);
        }
    }

    fn handle_release(&mut self, at: Pos2, now: f64) {
        self.handle_motion(at);
        let Some(p) = self.press.take() else { return };
        if p.drag_offset.is_some() {
            if self.pal.is_held() {
                // The drop may be on another monitor: find its area before
                // the pal falls.
                self.area_time = f64::NEG_INFINITY;
                self.refresh_area(now);
                self.pal.release();
            }
            return;
        }
        if now - p.time > CLICK_SECONDS {
            return;
        }
        match p.button {
            1 => self.toggle_chat(),
            2 => self.pal.poke(now, &self.sprites),
            3 => {
                let x = at.x.round() as i32;
                let y = at.y.round() as i32;
                self.chat.send(&ToChat::Menu { x, y });
            }
            _ => {}
        }
    }

    /// Queries the usable area of the monitor under the pal.
    fn refresh_area(&mut self, now: f64) {
        if now - self.area_time < AREA_INTERVAL {
            return;
        }
        self.area_time = now;
        let size = Pal::window_size(self.cfg.scale);
        let mut center = self.pal.pos + size / 2.0;
        // Only a drag moves the pal to another monitor. A pal that drops from
        // above the screen stays on its monitor.
        if !self.pal.is_held() {
            center.y = center.y.clamp(self.area.top, self.area.bottom - 1.0);
        }
        if let Some(r) = self.x11.work_area_at(center.x as i32, center.y as i32) {
            self.area = area_of(r);
        }
        // The window manager can move the pal window, for example when a
        // monitor is added or removed. Then the window is not at the position
        // of the pal, and the position must be sent again.
        if self.pal_win.mapped
            && let (Some(at), Some(sent)) =
                (self.x11.window_origin(self.pal_win.id), self.pal_win.pos)
            && at != sent
        {
            self.pal_win.pos = None;
        }
    }

    /// What the pal perceives now. The idle time and the hour are queried at
    /// most once per second. The pointer is queried only while the pal uses
    /// it.
    fn senses(&mut self, now: f64, may_move: bool) -> Senses {
        if now - self.senses_time >= 1.0 {
            self.senses_time = now;
            self.senses.idle = self.x11.idle_time();
            self.senses.night = self
                .cfg
                .is_night_hour(chrono::Timelike::hour(&chrono::Local::now()));
        }
        self.senses.pointer = if self.pal.wants_pointer(now, &self.cfg, may_move) {
            self.x11.pointer().map(|(x, y)| pos2(x as f32, y as f32))
        } else {
            None
        };
        self.senses
    }

    /// Activates the windows of the chat process that opened until they keep
    /// the keyboard focus for `FOCUS_STABLE` seconds.
    ///
    /// The window manager does not always move the focus to a window that
    /// opens after a click on the pal. After a global shortcut, the window
    /// manager can give the focus back to the last window shortly after it
    /// activated the new window.
    fn update_focus(&mut self, now: f64) {
        let Some(pid) = self.chat.pid() else {
            self.focus_pending.clear();
            return;
        };
        if self.focus_pending.is_empty() || now - self.focus_time < FOCUS_INTERVAL {
            return;
        }
        self.focus_time = now;
        for r in &mut self.focus_pending {
            let Some(win) = self.x11.find_window(pid, r.window.title()) else {
                continue;
            };
            if self.x11.active_window() == Some(win) {
                let since = *r.active_since.get_or_insert(now);
                if now - since >= FOCUS_STABLE {
                    r.until = now;
                }
            } else {
                r.active_since = None;
                self.x11.activate(win);
            }
        }
        self.focus_pending.retain(|r| r.until > now);
    }

    /// Shows or hides the pal. A hidden pal has no open chat panel and no menu.
    fn set_pal_visible(&mut self, visible: bool) {
        if visible == self.pal_visible {
            return;
        }
        self.pal_visible = visible;
        if !visible {
            self.chat.send_if_running(&ToChat::HidePanel);
            self.press = None;
        }
        self.pal_win.set_mapped(&self.x11, visible);
        if visible {
            // The window manager can place a mapped window anew.
            self.pal_win.pos = None;
        }
        if let Some(t) = &self.tray {
            t.set_pal_visible(visible);
        }
    }

    fn handle_tray(&mut self) {
        while let Some(cmd) = self.tray.as_ref().and_then(Tray::try_recv) {
            match cmd {
                TrayCommand::TogglePal => self.set_pal_visible(!self.pal_visible),
                TrayCommand::ToggleChat => self.toggle_chat(),
                TrayCommand::OpenSettings => self.chat.send(&ToChat::OpenSettings),
                TrayCommand::Quit => self.quit = true,
            }
        }
    }

    /// The window of the pal and the area of its monitor, for the place of
    /// the chat panel.
    fn anchor(&self) -> (Bounds, Bounds) {
        let pal = Rect::from_min_size(self.pal.pos.round(), Pal::window_size(self.cfg.scale));
        let a = self.area;
        let area = Rect::from_min_max(pos2(a.left, a.top), pos2(a.right, a.bottom));
        (bounds_of(pal), bounds_of(area))
    }

    /// Sends the anchor of the chat panel when it changed.
    fn send_anchor(&mut self) {
        let anchor = self.anchor();
        if self.sent_anchor != Some(anchor) {
            self.sent_anchor = Some(anchor);
            self.chat.send(&ToChat::Anchor {
                pal: anchor.0,
                area: anchor.1,
            });
        }
    }

    /// Opens or closes the chat panel, and shows a hidden pal.
    fn toggle_chat(&mut self) {
        self.set_pal_visible(true);
        self.chat.start();
        self.send_anchor();
        self.chat.send(&ToChat::TogglePanel);
    }

    /// Grabs the shortcuts of the configuration, and toggles the chat panel
    /// or the pal when its shortcut was pressed. Sends changed errors of the
    /// shortcuts to the chat process.
    fn handle_hotkeys(&mut self) {
        if self.chat_key.poll(
            &self.cfg.chat_shortcut,
            &self.waker,
            &mut self.chat_key_error,
        ) {
            self.toggle_chat();
        }
        if self
            .pal_key
            .poll(&self.cfg.pal_shortcut, &self.waker, &mut self.pal_key_error)
        {
            self.set_pal_visible(!self.pal_visible);
        }
        let errors = (self.chat_key_error.clone(), self.pal_key_error.clone());
        if self.sent_key_errors.as_ref() != Some(&errors) && self.chat.pid().is_some() {
            self.chat.send(&ToChat::Shortcuts {
                chat_error: errors.0.clone(),
                pal_error: errors.1.clone(),
            });
            self.sent_key_errors = Some(errors);
        }
    }

    fn handle_chat(&mut self) {
        let now = self.now();
        while let Some(ev) = self.chat.try_recv() {
            match ev {
                ChatEvent::Message(msg) => self.handle_chat_message(msg, now),
                ChatEvent::Ended(_) => {
                    if self.chat.pid().is_none() {
                        self.open = [false; 3];
                        self.running = false;
                        self.pal.set_thinking(false);
                        self.focus_pending.clear();
                        self.sent_anchor = None;
                        self.sent_key_errors = None;
                    }
                }
            }
        }
    }

    fn handle_chat_message(&mut self, msg: ToPal, now: f64) {
        match msg {
            ToPal::Window { window, open } => {
                let i = Window::ALL.iter().position(|w| *w == window).unwrap_or(0);
                if open && !self.open[i] {
                    self.focus_pending.retain(|r| r.window != window);
                    self.focus_pending.push(FocusRequest {
                        window,
                        until: now + FOCUS_SECONDS,
                        active_since: None,
                    });
                    self.focus_time = f64::NEG_INFINITY;
                }
                if !open {
                    self.focus_pending.retain(|r| r.window != window);
                }
                self.open[i] = open;
            }
            ToPal::Running { running } => {
                self.running = running;
                self.pal.set_thinking(running);
            }
            ToPal::TurnEnd { success } => {
                self.running = false;
                if success {
                    let before = self.progress.tier();
                    self.progress.completed += 1;
                    let unlocked = self.progress.tier() > before;
                    self.progress.save();
                    self.pal.celebrate(now);
                    self.update_tier(now, unlocked);
                } else {
                    self.pal.set_thinking(false);
                }
            }
            ToPal::Tool { activity } => {
                if self.cfg.tool_reactions {
                    self.pal.react_to_tool(&activity, now, &self.sprites);
                }
            }
            ToPal::ConfigChanged => {
                let (cfg, error) = Config::load();
                if let Some(e) = error {
                    eprintln!("pixelpal: {e}");
                }
                self.cfg = cfg;
                self.update_tier(now, false);
                self.sent_anchor = None;
            }
            ToPal::HidePal => self.set_pal_visible(false),
            ToPal::Quit => self.quit = true,
        }
    }

    /// Loads the sprites of the gear tier of the progress. `unlocked` is true
    /// when a turn unlocked a new tier: Link then shows a level up.
    fn update_tier(&mut self, now: f64, unlocked: bool) {
        let tier = if self.cfg.gear_progress {
            self.progress.tier().min(crate::link::tier_count() - 1)
        } else {
            0
        };
        if tier != self.sprites.tier {
            self.sprites = Sprites::load(tier);
            if unlocked && self.cfg.gear_progress {
                self.pal.level_up(now, &self.sprites);
            }
        }
    }

    /// True if the pal may walk now: the pal is visible, the menu is closed,
    /// and the chat was not active during the rest time of the
    /// configuration. Enemies come only while the pal may walk.
    fn may_move(&mut self, now: f64) -> bool {
        if self.is_open(Window::Chat) || self.running {
            self.chat_active_at = now;
        }
        self.pal_visible
            && !self.is_open(Window::Menu)
            && now - self.chat_active_at >= f64::from(self.cfg.chat_rest)
    }

    /// Keeps the window states and the input shape of the pal window, and
    /// the enemy window above all other windows.
    fn manage_windows(&mut self, now: f64) {
        if now - self.manage_time < MANAGE_INTERVAL {
            return;
        }
        self.manage_time = now;
        if self.pal_win.mapped {
            self.x11.keep_states(self.pal_win.id);
        }
        if let Some(e) = &self.enemy_win
            && e.mapped
        {
            self.x11.raise(e.id);
        }
    }

    /// Only Link's body takes clicks; the rest of the pal window lets them
    /// through to the windows below.
    fn update_input_shape(&mut self) {
        let full = Rect::from_min_size(Pos2::ZERO, Pal::window_size(self.cfg.scale));
        let click = Pal::click_rect(full, &self.sprites, self.cfg.scale as f32);
        let shape = PxRect {
            x: click.min.x.floor() as i32,
            y: click.min.y.floor() as i32,
            w: click.width().ceil() as i32,
            h: click.height().ceil() as i32,
        };
        if self.input_shape != Some(shape) {
            self.x11.set_input_rect(self.pal_win.id, shape);
            self.input_shape = Some(shape);
        }
    }

    /// Shows the enemy window while an enemy or a drop is on the screen. The
    /// window lets all clicks through.
    ///
    /// The window manager does not manage the enemy window (override
    /// redirect), so a new enemy never takes the keyboard focus from the
    /// active window.
    fn show_enemy(&mut self, now: f64, night: bool) {
        let pos = self
            .fight
            .window_pos(&self.cfg, self.area)
            .filter(|_| self.pal_visible);
        let Some(pos) = pos else {
            if let Some(e) = &mut self.enemy_win {
                e.set_mapped(&self.x11, false);
            }
            return;
        };
        let size = crate::enemy::WINDOW * self.cfg.scale as f32;
        if self.enemy_win.is_none() {
            let rect = PxRect {
                x: pos.x as i32,
                y: pos.y as i32,
                w: size.x as i32,
                h: size.y as i32,
            };
            match SpriteWindow::create(&self.x11, ENEMY_TITLE, rect, true) {
                Ok(w) => {
                    self.x11.clear_input(w.id);
                    self.enemy_win = Some(w);
                }
                Err(e) => {
                    eprintln!("pixelpal: no enemy window: {e}");
                    return;
                }
            }
        }
        let Some(win) = &mut self.enemy_win else {
            return;
        };
        win.resize(&self.x11, size);
        win.move_to(&self.x11, pos);
        win.canvas.clear();
        let rect = win.canvas.rect();
        self.fight.paint(
            &win.canvas,
            rect,
            pos,
            &self.sprites,
            &self.cfg,
            self.area,
            night,
            now,
            false,
        );
        win.present(&self.x11);
        if !win.mapped {
            win.set_mapped(&self.x11, true);
            self.x11.raise(win.id);
        }
    }

    /// One pass: moves the pal and the enemies, and draws the windows.
    /// Returns the time in seconds until the next pass.
    fn frame(&mut self) -> f64 {
        let now = self.now();
        let dt = (now - self.last_frame).clamp(0.0, 0.1) as f32;
        self.last_frame = now;
        self.refresh_area(now);
        self.manage_windows(now);
        self.update_tier(now, false);

        let size = Pal::window_size(self.cfg.scale);
        self.pal_win.resize(&self.x11, size);
        self.update_input_shape();

        let may_move = self.may_move(now);
        let senses = self.senses(now, may_move);
        self.pal.update(
            now,
            dt,
            self.area,
            &self.cfg,
            &self.sprites,
            may_move,
            &senses,
        );
        let night = self.cfg.day_night && senses.night;
        self.fight.update(
            now,
            dt,
            &mut self.pal,
            &self.sprites,
            &self.cfg,
            self.area,
            may_move,
            night,
        );

        if self.pal_visible {
            let pos = self.pal.pos.round();
            self.pal_win.move_to(&self.x11, pos);
            let canvas = &self.pal_win.canvas;
            canvas.clear();
            let rect = canvas.rect();
            self.pal.paint(canvas, rect, &self.sprites, &self.cfg, now);
            // Stones that fly toward Link.
            self.fight.paint(
                canvas,
                rect,
                self.pal.pos,
                &self.sprites,
                &self.cfg,
                self.area,
                night,
                now,
                true,
            );
            self.pal_win.present(&self.x11);
        }
        self.show_enemy(now, night);
        if self.is_open(Window::Chat) {
            self.send_anchor();
        }
        self.update_focus(now);

        // The next pass comes when the pal changes, and at least once per
        // second for the periodic checks (monitor area, window states, idle
        // time, night).
        let may_move = self.may_move(now);
        let mut next = self
            .pal
            .next_change(now, &self.cfg, may_move, self.senses.idle)
            .min(MAX_WAIT);
        if !self.focus_pending.is_empty() {
            next = next.min(FOCUS_INTERVAL);
        }
        if self.fight.is_active() {
            next = next.min(1.0 / 60.0);
        }
        if !self.pal_visible {
            next = MAX_WAIT;
        }
        next.max(0.001)
    }
}
