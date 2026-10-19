//! The egui application: the pal window, the chat panel, the menu, the settings
//! window and the tray icon.

use egui::{
    Id, PointerButton, Pos2, Rect, Sense, Ui, Vec2, ViewportBuilder, ViewportCommand, ViewportId,
    pos2, vec2,
};

use std::time::{Duration, Instant};

use crate::chat::{Chat, TurnEnd};
use crate::enemy::{ENEMY_TITLE, Fight};
use crate::hotkey::Hotkey;
use crate::link::Sprites;
use crate::pal::{Area, Pal, Senses};
use crate::progress::Progress;
use crate::settings::{Config, SettingsUi};
use crate::theme;
use crate::tray::{Tray, TrayCommand};
use crate::workarea::{PxRect, X11};

/// Title of the pal window.
pub const PAL_TITLE: &str = "pixelpal";
const CHAT_TITLE: &str = "pixelpal chat";
const MENU_TITLE: &str = "pixelpal menu";
const SETTINGS_TITLE: &str = "pixelpal settings";
/// Titles of the windows that open on request, in the order of `PalApp::was_open`.
const WINDOW_TITLES: [&str; 3] = [CHAT_TITLE, MENU_TITLE, SETTINGS_TITLE];
/// Seconds during which pixelpal tries to activate a new window.
const FOCUS_SECONDS: f64 = 3.0;
/// Seconds between two activation requests.
const FOCUS_INTERVAL: f64 = 0.2;
/// Time between showing the hidden pal window and showing its child windows.
const REVEAL_DELAY: Duration = Duration::from_millis(300);
/// Seconds between two queries of the usable area.
const AREA_INTERVAL: f64 = 2.0;
/// Seconds between two updates of the window states.
const MANAGE_INTERVAL: f64 = 1.0;
const MENU_SIZE: Vec2 = vec2(150.0, 122.0);
/// Width of the resize zones at the edges of the chat panel.
const GRIP: f32 = 6.0;
/// Smallest size of the chat panel.
const PANEL_MIN: Vec2 = vec2(280.0, 240.0);

/// The edges of the chat panel that a resize drag moves.
#[derive(Clone, Copy)]
struct Edges {
    left: bool,
    right: bool,
    top: bool,
}

/// A running resize drag of the chat panel.
struct PanelResize {
    edges: Edges,
    /// Global pointer position at the start of the drag, in points.
    start: Pos2,
    start_size: Vec2,
    /// The X11 window of the chat panel.
    window: Option<u32>,
}

pub struct PalApp {
    cfg: Config,
    saved_cfg: Config,
    sprites: Sprites,
    pal: Pal,
    x11: Option<X11>,
    area: Area,
    area_time: f64,
    chat: Chat,
    panel_open: bool,
    /// The edited copy of the configuration while the settings window is open.
    settings_draft: Option<Config>,
    settings_ui: SettingsUi,
    /// The font file of the registered chat font.
    loaded_font: String,
    tray: Option<Tray>,
    /// The grabbed chat shortcut, and the shortcut text of the last grab.
    hotkey: Option<Hotkey>,
    hotkey_spec: Option<String>,
    pal_visible: bool,
    /// The time at which the hidden pal window became visible again.
    revealed_at: Option<Instant>,
    manage_time: f64,
    /// Windows that do not have the keyboard focus yet, with the time until
    /// which pixelpal tries to activate them.
    focus_pending: Vec<(&'static str, f64)>,
    focus_time: f64,
    /// Open state of the windows in `WINDOW_TITLES` in the last frame.
    was_open: [bool; 3],
    /// Top-left corner of the open menu, in screen points.
    menu: Option<Pos2>,
    menu_had_focus: bool,
    /// Pointer position minus pal position at the start of a drag.
    drag_offset: Option<Vec2>,
    sent_pos: Option<Pos2>,
    panel_resize: Option<PanelResize>,
    sent_scale: u32,
    progress: Progress,
    /// The last senses of the pal, and the time of their last full query.
    senses: Senses,
    senses_time: f64,
    /// The last input shape of the pal window.
    input_shape: Option<(u32, PxRect)>,
    /// The enemies and Link's fights with them.
    fight: Fight,
    /// The last position sent to the enemy window.
    enemy_pos: Option<Pos2>,
}

impl PalApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cfg: Config,
        cfg_error: Option<String>,
        x11: Option<X11>,
        start: Pos2,
        area: Area,
    ) -> Self {
        let chat = Chat::new(&cfg);
        let panel_open = chat.has_question();
        let mut settings_ui = SettingsUi::default();
        settings_ui.status = cfg_error;
        settings_ui.font_error = theme::set_fonts(&cc.egui_ctx, &cfg.theme.font_file).err();
        let (w, h, rgba) = crate::link::icon(48);
        let tray = Tray::spawn(cc.egui_ctx.clone(), w, h, &rgba)
            .map_err(|e| eprintln!("pixelpal: no tray icon: {e}"))
            .ok();
        let progress = Progress::load();
        settings_ui.progress = progress.summary();
        let tier = if cfg.gear_progress {
            progress.tier()
        } else {
            0
        };
        Self {
            sprites: Sprites::load(&cc.egui_ctx, tier),
            fight: Fight::default(),
            enemy_pos: None,
            progress,
            pal: Pal::new(start),
            x11,
            area,
            area_time: f64::NEG_INFINITY,
            chat,
            panel_open,
            settings_draft: None,
            settings_ui,
            loaded_font: cfg.theme.font_file.clone(),
            tray,
            hotkey: None,
            hotkey_spec: None,
            pal_visible: true,
            revealed_at: None,
            manage_time: f64::NEG_INFINITY,
            focus_pending: Vec::new(),
            focus_time: f64::NEG_INFINITY,
            was_open: [false; 3],
            menu: None,
            menu_had_focus: false,
            drag_offset: None,
            sent_pos: None,
            panel_resize: None,
            sent_scale: cfg.scale,
            senses: Senses::default(),
            senses_time: f64::NEG_INFINITY,
            input_shape: None,
            saved_cfg: cfg.clone(),
            cfg,
        }
    }

    fn pixels_per_point(ctx: &egui::Context) -> f32 {
        ctx.input(|i| i.viewport().native_pixels_per_point)
            .unwrap_or(1.0)
    }

    /// Queries the usable area of the monitor under the pal.
    fn refresh_area(&mut self, ctx: &egui::Context, now: f64) {
        if now - self.area_time < AREA_INTERVAL {
            return;
        }
        self.area_time = now;
        let ppp = Self::pixels_per_point(ctx);
        let size = Pal::window_size(self.cfg.scale);
        let mut center = self.pal.pos + size / 2.0;
        // Only a drag moves the pal to another monitor. A pal that drops from
        // above the screen stays on its monitor.
        if !self.pal.is_held() {
            center.y = center.y.clamp(self.area.top, self.area.bottom - 1.0);
        }
        if let Some(r) = self
            .x11
            .as_ref()
            .and_then(|x| x.work_area_at((center.x * ppp) as i32, (center.y * ppp) as i32))
        {
            self.area = Area {
                left: r.x as f32 / ppp,
                top: r.y as f32 / ppp,
                right: (r.x + r.w) as f32 / ppp,
                bottom: (r.y + r.h) as f32 / ppp,
            };
        } else if let Some(m) = ctx.input(|i| i.viewport().monitor_size) {
            self.area = Area {
                left: 0.0,
                top: 0.0,
                right: m.x,
                bottom: m.y,
            };
        }
    }

    /// What the pal perceives now. The idle time and the hour are queried at
    /// most once per second.
    /// The pointer is queried only while the pal uses it.
    fn senses(&mut self, ctx: &egui::Context, now: f64, may_move: bool) -> Senses {
        if now - self.senses_time >= 1.0 {
            self.senses_time = now;
            self.senses.idle = self.x11.as_ref().and_then(X11::idle_time);
            self.senses.night = self
                .cfg
                .is_night_hour(chrono::Timelike::hour(&chrono::Local::now()));
        }
        self.senses.pointer = if self.pal.wants_pointer(now, &self.cfg, may_move) {
            self.pointer(ctx)
        } else {
            None
        };
        self.senses
    }

    /// The global pointer position in points.
    fn pointer(&self, ctx: &egui::Context) -> Option<Pos2> {
        let ppp = Self::pixels_per_point(ctx);
        let (x, y) = self.x11.as_ref()?.pointer()?;
        Some(pos2(x as f32 / ppp, y as f32 / ppp))
    }

    fn handle_input(&mut self, ui: &mut Ui, rect: Rect) {
        let ctx = ui.ctx().clone();
        let resp = ui.interact(rect, Id::new("pal"), Sense::click_and_drag());

        // A drag lifts the pal. While the chat panel is open, a drag moves the
        // pal only along the ground, and the panel follows him.
        if resp.drag_started_by(PointerButton::Primary) {
            let offset = match self.pointer(&ctx) {
                Some(p) => p - self.pal.pos,
                None => resp
                    .interact_pointer_pos()
                    .map(|p| p - rect.min)
                    .unwrap_or_default(),
            };
            self.drag_offset = Some(offset);
            if !self.panel_open {
                self.pal.hold();
            }
        }
        if let Some(offset) = self.drag_offset {
            if resp.dragged_by(PointerButton::Primary) {
                let pos = match self.pointer(&ctx) {
                    Some(p) => p - offset,
                    None => self.pal.pos + resp.drag_delta(),
                };
                if self.pal.is_held() {
                    self.pal.pos = pos;
                } else {
                    let max_x =
                        (self.area.right - Pal::window_size(self.cfg.scale).x).max(self.area.left);
                    self.pal.pos.x = pos.x.clamp(self.area.left, max_x);
                }
            }
            if resp.drag_stopped() || !ctx.input(|i| i.pointer.primary_down()) {
                self.drag_offset = None;
                if self.pal.is_held() {
                    // The drop may be on another monitor: find its area before
                    // the pal falls.
                    self.area_time = f64::NEG_INFINITY;
                    self.refresh_area(&ctx, ctx.input(|i| i.time));
                    self.pal.release();
                }
            }
        }
        if resp.clicked_by(PointerButton::Middle) {
            self.pal.poke(ctx.input(|i| i.time), &self.sprites);
        }
        if resp.clicked_by(PointerButton::Primary) {
            self.menu = None;
            self.panel_open = !self.panel_open;
            if self.panel_open {
                self.chat.focus_input();
            }
        }
        if resp.secondary_clicked() {
            let ppp_pos = self.pointer(&ctx).unwrap_or(self.pal.pos);
            self.menu = Some(pos2(ppp_pos.x, ppp_pos.y - MENU_SIZE.y));
            self.menu_had_focus = false;
        }
    }

    /// The place of the chat panel on the screen: centered above the pal,
    /// inside the area.
    fn panel_rect(&self) -> Rect {
        let size = vec2(self.cfg.panel_width, self.cfg.panel_height);
        let pal_size = Pal::window_size(self.cfg.scale);
        let a = self.area;
        let x = (self.pal.pos.x + pal_size.x / 2.0 - size.x / 2.0)
            .clamp(a.left, (a.right - size.x).max(a.left));
        let y = (self.pal.pos.y - size.y - 4.0).max(a.top);
        Rect::from_min_size(pos2(x, y).round(), size)
    }

    fn show_panel(&mut self, ctx: &egui::Context) {
        let rect = self.panel_rect();
        let builder = ViewportBuilder::default()
            .with_title(CHAT_TITLE)
            .with_transparent(true)
            .with_position(rect.min)
            .with_inner_size(rect.size())
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false);
        let mut out = Default::default();
        let mut resize = None;
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("pixelpal_chat"),
            builder,
            |ui, _| {
                let full = ui.max_rect();
                ui.set_style(self.cfg.theme.style(ui.style()));
                out = self.chat.ui(ui, &mut self.cfg);
                resize = resize_zones(ui, full);
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    out.close_panel = true;
                }
            },
        );
        let out: crate::chat::ChatOutput = out;
        self.handle_panel_resize(ctx, resize);
        if out.close_panel {
            self.panel_open = false;
        }
        if out.open_settings {
            self.open_settings();
        }
    }

    /// Changes the size of the chat panel during a resize drag. The panel stays
    /// centered above the pal, so a side edge changes the width on both sides.
    /// The new size goes into the configuration, and into the settings draft at
    /// the end of the drag.
    ///
    /// egui only starts the drag. Then each frame reads the pointer and the
    /// left mouse button from X11, so that the drag continues when the pointer
    /// leaves the panel before the panel grows, and ends when the button is up.
    fn handle_panel_resize(&mut self, ctx: &egui::Context, started: Option<Edges>) {
        if let Some(edges) = started
            && let Some(start) = self.pointer(ctx)
        {
            self.panel_resize = Some(PanelResize {
                edges,
                start,
                start_size: vec2(self.cfg.panel_width, self.cfg.panel_height),
                window: self
                    .x11
                    .as_ref()
                    .and_then(|x| x.find_window(std::process::id(), CHAT_TITLE)),
            });
        }
        let Some(r) = &self.panel_resize else {
            return;
        };
        let ppp = Self::pixels_per_point(ctx);
        let state = self.x11.as_ref().and_then(|x| x.pointer_and_button());
        let Some((px, py, true)) = state else {
            self.panel_resize = None;
            if let Some(d) = &mut self.settings_draft {
                d.panel_width = self.cfg.panel_width;
                d.panel_height = self.cfg.panel_height;
            }
            return;
        };
        let d = pos2(px as f32 / ppp, py as f32 / ppp) - r.start;
        let mut size = r.start_size;
        if r.edges.right {
            size.x += 2.0 * d.x;
        }
        if r.edges.left {
            size.x -= 2.0 * d.x;
        }
        if r.edges.top {
            size.y -= d.y;
        }
        let a = self.area;
        let max = vec2(a.right - a.left, a.bottom - a.top).max(PANEL_MIN);
        let size = size.clamp(PANEL_MIN, max).round();
        let window = r.window;
        ctx.request_repaint();
        if size == vec2(self.cfg.panel_width, self.cfg.panel_height) {
            return;
        }
        self.cfg.panel_width = size.x;
        self.cfg.panel_height = size.y;
        // The window takes the new size and position at once and in one
        // request, not with the next frame of the panel.
        if let (Some(x), Some(win)) = (&self.x11, window) {
            let rect = self.panel_rect();
            x.move_resize(
                win,
                PxRect {
                    x: (rect.min.x * ppp).round() as i32,
                    y: (rect.min.y * ppp).round() as i32,
                    w: (rect.width() * ppp).round() as i32,
                    h: (rect.height() * ppp).round() as i32,
                },
            );
        }
    }

    /// Activates the windows that opened but do not have the keyboard focus yet.
    ///
    /// A click on the pal gives the pal window the focus, and the window manager
    /// does not always move the focus to a window that opens after the click.
    fn update_focus(&mut self, now: f64) {
        let open = [
            self.panel_open,
            self.menu.is_some(),
            self.settings_draft.is_some(),
        ];
        for (i, title) in WINDOW_TITLES.iter().enumerate() {
            if open[i] && !self.was_open[i] {
                self.focus_pending.retain(|(t, _)| t != title);
                self.focus_pending.push((title, now + FOCUS_SECONDS));
            }
            if !open[i] {
                self.focus_pending.retain(|(t, _)| t != title);
            }
        }
        self.was_open = open;
        let Some(x) = &self.x11 else {
            self.focus_pending.clear();
            return;
        };
        if now - self.focus_time < FOCUS_INTERVAL {
            return;
        }
        self.focus_time = now;
        for (title, until) in &mut self.focus_pending {
            let Some(win) = x.find_window(std::process::id(), title) else {
                continue;
            };
            if x.active_window() == Some(win) {
                *until = now;
            } else {
                x.activate(win);
            }
        }
        self.focus_pending.retain(|(_, until)| *until > now);
    }

    /// Shows or hides the pal. A hidden pal has no open chat panel and no menu.
    fn set_pal_visible(&mut self, ctx: &egui::Context, visible: bool) {
        if visible == self.pal_visible {
            return;
        }
        self.pal_visible = visible;
        self.revealed_at = visible.then(Instant::now);
        if !visible {
            self.panel_open = false;
            self.menu = None;
        }
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(visible));
        if let Some(t) = &self.tray {
            t.set_pal_visible(visible);
        }
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        while let Some(cmd) = self.tray.as_ref().and_then(Tray::try_recv) {
            match cmd {
                TrayCommand::TogglePal => self.set_pal_visible(ctx, !self.pal_visible),
                TrayCommand::ToggleChat => self.toggle_chat(ctx),
                TrayCommand::OpenSettings => {
                    self.set_pal_visible(ctx, true);
                    self.open_settings();
                }
                TrayCommand::Quit => {
                    if self.pal_visible {
                        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
                    } else {
                        // A hidden window gets no egui pass, so the close
                        // command would wait until the window is visible again.
                        self.chat.stop();
                        std::process::exit(0);
                    }
                }
            }
        }
    }

    /// Opens or closes the chat panel, and shows a hidden pal.
    fn toggle_chat(&mut self, ctx: &egui::Context) {
        // The chat panel and the settings window need the pal window: egui
        // shows them only while the pal window is visible.
        self.set_pal_visible(ctx, true);
        self.panel_open = !self.panel_open;
        if self.panel_open {
            self.chat.focus_input();
        }
    }

    /// Grabs the chat shortcut of the configuration when it differs from the
    /// last grab, and toggles the chat panel when the shortcut was pressed.
    fn handle_hotkey(&mut self, ctx: &egui::Context) {
        let spec = self.cfg.chat_shortcut.trim();
        if self.hotkey_spec.as_deref() != Some(spec) {
            self.hotkey = None;
            self.settings_ui.shortcut_error = None;
            if !spec.is_empty() {
                match Hotkey::grab(spec, ctx.clone()) {
                    Ok(h) => self.hotkey = Some(h),
                    Err(e) => self.settings_ui.shortcut_error = Some(e),
                }
            }
            self.hotkey_spec = Some(spec.to_owned());
        }
        if self.hotkey.as_ref().is_some_and(Hotkey::pressed) {
            self.menu = None;
            self.toggle_chat(ctx);
        }
    }

    fn open_settings(&mut self) {
        if self.settings_draft.is_none() {
            self.settings_draft = Some(self.cfg.clone());
            self.settings_ui.status = None;
        }
    }

    /// Applies the settings draft. Returns false if the draft has an error.
    fn apply_settings(&mut self) -> bool {
        let Some(mut draft) = self.settings_draft.clone() else {
            return true;
        };
        if draft.autostart != self.cfg.autostart
            && let Err(e) = draft.apply_autostart()
        {
            self.settings_ui.status = Some(format!("Autostart: {e}"));
            return false;
        }
        // The model selector of the chat panel can change the model while the
        // settings window is open.
        if draft.models.contains(&self.cfg.model) {
            draft.model = self.cfg.model.clone();
        } else if !draft.models.contains(&draft.model) {
            draft.model = draft
                .models
                .first()
                .cloned()
                .unwrap_or_else(|| "default".into());
        }
        self.cfg = draft;
        self.cfg.save();
        self.saved_cfg = self.cfg.clone();
        true
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        let builder = ViewportBuilder::default()
            .with_title(SETTINGS_TITLE)
            .with_inner_size([520.0, 640.0])
            .with_always_on_top();
        let (mut ok, mut apply, mut cancel) = (false, false, false);
        let Some(draft) = self.settings_draft.as_mut() else {
            return;
        };
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("pixelpal_settings"),
            builder,
            |ui, _| {
                egui::Panel::bottom("settings_buttons")
                    .resizable(false)
                    .exact_size(40.0)
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            cancel = ui.button("Cancel").clicked();
                            apply = ui.button("Apply").clicked();
                            ok = ui.button("  OK  ").clicked();
                        });
                        ui.add_space(4.0);
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    self.settings_ui.show(ui, draft);
                });
                cancel |= ui.ctx().input(|i| i.viewport().close_requested());
            },
        );
        if (ok && self.apply_settings()) || cancel {
            self.settings_draft = None;
        } else if apply {
            self.apply_settings();
        }
    }

    /// Shows the enemy window while an enemy or a drop is on the screen. The
    /// window lets all clicks through.
    ///
    /// The window manager does not manage the enemy window (override
    /// redirect), so a new enemy never takes the keyboard focus from the
    /// active window.
    fn show_enemy(&mut self, ctx: &egui::Context, now: f64, night: bool) {
        let Some(pos) = self.fight.window_pos(&self.cfg, self.area) else {
            self.enemy_pos = None;
            return;
        };
        let builder = ViewportBuilder::default()
            .with_title(ENEMY_TITLE)
            .with_position(pos)
            .with_inner_size(crate::enemy::WINDOW * self.cfg.scale as f32)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_mouse_passthrough(true)
            .with_override_redirect(true)
            .with_window_type(egui::X11WindowType::Notification);
        let move_to = (self.enemy_pos != Some(pos)).then_some(pos);
        self.enemy_pos = Some(pos);
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("pixelpal_enemy"),
            builder,
            |ui, _| {
                if let Some(pos) = move_to {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::OuterPosition(pos));
                }
                let rect = ui.max_rect();
                self.fight.paint(
                    ui.painter(),
                    rect,
                    pos,
                    &self.sprites,
                    &self.cfg,
                    self.area,
                    night,
                    now,
                    false,
                );
            },
        );
    }

    fn show_menu(&mut self, ctx: &egui::Context, at: Pos2) {
        let builder = ViewportBuilder::default()
            .with_title(MENU_TITLE)
            .with_position(at.round())
            .with_inner_size(MENU_SIZE)
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false);
        let mut action = None;
        let mut focused = None;
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("pixelpal_menu"),
            builder,
            |ui, _| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.vertical_centered_justified(|ui| {
                        let chat = if self.panel_open {
                            "Hide chat"
                        } else {
                            "Open chat"
                        };
                        if ui.button(chat).clicked() {
                            action = Some("chat");
                        }
                        if ui.button("Settings…").clicked() {
                            action = Some("settings");
                        }
                        if ui.button("Hide").clicked() {
                            action = Some("hide");
                        }
                        if ui.button("Quit").clicked() {
                            action = Some("quit");
                        }
                    });
                });
                let i = ui
                    .ctx()
                    .input(|i| (i.viewport().focused, i.key_pressed(egui::Key::Escape)));
                focused = i.0;
                if i.1 || ui.ctx().input(|i| i.viewport().close_requested()) {
                    action = Some("close");
                }
            },
        );
        match focused {
            Some(true) => self.menu_had_focus = true,
            Some(false) if self.menu_had_focus => action = action.or(Some("close")),
            _ => {}
        }
        match action {
            Some("chat") => {
                self.panel_open = !self.panel_open;
                self.menu = None;
            }
            Some("settings") => {
                self.open_settings();
                self.menu = None;
            }
            Some("hide") => self.set_pal_visible(ctx, false),
            Some("quit") => ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close),
            Some(_) => self.menu = None,
            None => {}
        }
    }
}

impl eframe::App for PalApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);
        self.handle_hotkey(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.refresh_area(&ctx, now);

        if now - self.manage_time >= MANAGE_INTERVAL {
            self.manage_time = now;
            if let Some(x) = &self.x11 {
                x.manage_windows(std::process::id(), &[PAL_TITLE]);
                if self.enemy_pos.is_some() {
                    x.raise_toplevel(std::process::id(), ENEMY_TITLE);
                }
                // Only Link's body takes clicks; the rest of the pal window
                // lets them through to the windows below.
                if let Some(win) = x.find_window(std::process::id(), PAL_TITLE) {
                    let ppp = Self::pixels_per_point(&ctx);
                    let full = Rect::from_min_size(Pos2::ZERO, Pal::window_size(self.cfg.scale));
                    let click = Pal::click_rect(full, &self.sprites, self.cfg.scale as f32);
                    let shape = PxRect {
                        x: (click.min.x * ppp).floor() as i32,
                        y: (click.min.y * ppp).floor() as i32,
                        w: (click.width() * ppp).ceil() as i32,
                        h: (click.height() * ppp).ceil() as i32,
                    };
                    if self.input_shape != Some((win, shape)) {
                        x.set_input_rect(win, shape);
                        self.input_shape = Some((win, shape));
                    }
                }
            }
        }

        // The chat font follows the theme.
        let font = &self.cfg.theme.font_file;
        if *font != self.loaded_font {
            self.loaded_font = font.clone();
            self.settings_ui.font_error = theme::set_fonts(&ctx, font).err();
        }

        // Pal mood from the chat. `unlocked` is true when this turn unlocks a
        // new gear tier.
        let mut unlocked = false;
        match self.chat.poll(&self.cfg) {
            Some(TurnEnd::Success) => {
                let before = self.progress.tier();
                self.progress.completed += 1;
                unlocked = self.progress.tier() > before;
                self.progress.save();
                self.settings_ui.progress = self.progress.summary();
                self.pal.celebrate(now);
            }
            Some(TurnEnd::Failure) => self.pal.set_thinking(false),
            None => self.pal.set_thinking(self.chat.is_running()),
        }
        // The gear follows the progress. A new, higher tier is a level up.
        let tier = if self.cfg.gear_progress {
            self.progress.tier().min(crate::link::tier_count() - 1)
        } else {
            0
        };
        if tier != self.sprites.tier {
            self.sprites = Sprites::load(&ctx, tier);
            if unlocked && self.cfg.gear_progress {
                self.pal.level_up(now, &self.sprites);
            }
        }
        for activity in self.chat.take_tool_activity() {
            if self.cfg.tool_reactions {
                self.pal.react_to_tool(&activity, now, &self.sprites);
            }
        }

        // Window size follows the scale.
        if self.cfg.scale != self.sent_scale {
            self.sent_scale = self.cfg.scale;
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Pal::window_size(self.cfg.scale)));
        }

        let rect = ui.max_rect();
        // Only Link's body reacts to the mouse, not the empty space around him.
        let click = Pal::click_rect(rect, &self.sprites, self.cfg.scale as f32);
        self.handle_input(ui, click);

        let may_move = !self.panel_open && !self.chat.is_running() && self.menu.is_none();
        let senses = self.senses(&ctx, now, may_move);
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

        let pos = self.pal.pos.round();
        if self.sent_pos != Some(pos) {
            self.sent_pos = Some(pos);
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
        }

        self.pal
            .paint(ui.painter(), rect, &self.sprites, &self.cfg, now);
        // Stones that fly toward Link.
        self.fight.paint(
            ui.painter(),
            rect,
            self.pal.pos,
            &self.sprites,
            &self.cfg,
            self.area,
            night,
            now,
            true,
        );
        self.show_enemy(&ctx, now, night);

        // egui shows child windows only while the pal window is visible. After
        // the hidden pal window becomes visible again, this takes some time.
        let ready =
            self.pal_visible && self.revealed_at.is_none_or(|t| t.elapsed() >= REVEAL_DELAY);
        if ready {
            if self.panel_open {
                self.show_panel(&ctx);
            }
            if let Some(at) = self.menu {
                self.show_menu(&ctx, at);
            }
            if self.settings_draft.is_some() {
                self.show_settings(&ctx);
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.update_focus(now);

        if self.cfg != self.saved_cfg
            && self.panel_resize.is_none()
            && !ctx.input(|i| i.pointer.any_down())
        {
            self.cfg.save();
            self.saved_cfg = self.cfg.clone();
        }

        // Repaint when the pal changes, and at least once per second for the
        // periodic checks (monitor area, window states, idle time, night).
        // Input events, the tray and the `claude` reader repaint at once.
        let may_move = !self.panel_open && !self.chat.is_running() && self.menu.is_none();
        let ppp = Self::pixels_per_point(&ctx);
        let mut next = self
            .pal
            .next_change(now, &self.cfg, ppp, may_move, self.senses.idle)
            .min(1.0);
        if !self.focus_pending.is_empty() {
            next = next.min(FOCUS_INTERVAL);
        }
        if self.fight.is_active() {
            next = next.min(1.0 / 60.0);
        }
        ctx.request_repaint_after(Duration::from_secs_f64(next));
    }
}

/// Resize zones at the top edge, the side edges and the top corners of the chat
/// panel `full`. Returns the edges of a resize drag that starts in this frame.
fn resize_zones(ui: &mut Ui, full: Rect) -> Option<Edges> {
    use egui::CursorIcon;
    let (l, r, t) = (full.left(), full.right(), full.top());
    let edges = |left, right, top| Edges { left, right, top };
    // Corners come last: of overlapping zones, egui gives the drag to the last one.
    let zones = [
        (
            Rect::from_min_max(pos2(l, t), pos2(r, t + GRIP)),
            edges(false, false, true),
            CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_max(pos2(l, t), pos2(l + GRIP, full.bottom())),
            edges(true, false, false),
            CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_max(pos2(r - GRIP, t), pos2(r, full.bottom())),
            edges(false, true, false),
            CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_max(pos2(l, t), pos2(l + 2.0 * GRIP, t + 2.0 * GRIP)),
            edges(true, false, true),
            CursorIcon::ResizeNorthWest,
        ),
        (
            Rect::from_min_max(pos2(r - 2.0 * GRIP, t), pos2(r, t + 2.0 * GRIP)),
            edges(false, true, true),
            CursorIcon::ResizeNorthEast,
        ),
    ];
    let mut start = None;
    for (i, (rect, e, icon)) in zones.into_iter().enumerate() {
        let resp = ui.interact(rect, Id::new(("panel_resize", i)), Sense::drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(icon);
        }
        if resp.drag_started() {
            start = Some(e);
        }
    }
    start
}
