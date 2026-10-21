//! The configuration file and the settings window.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use egui::{ComboBox, DragValue, RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::picker::{self, Pick, PickResult};
use crate::theme::ChatTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ChatOnly,
    ReadOnly,
    EditFiles,
    Everything,
    Custom,
}

impl Permission {
    const ALL: [Permission; 5] = [
        Permission::ChatOnly,
        Permission::ReadOnly,
        Permission::EditFiles,
        Permission::Everything,
        Permission::Custom,
    ];

    fn label(self) -> &'static str {
        match self {
            Permission::ChatOnly => "Chat only (no tools)",
            Permission::ReadOnly => "Read-only",
            Permission::EditFiles => "Edit files",
            Permission::Everything => "Everything (no questions)",
            Permission::Custom => "Custom",
        }
    }
}

const READ_ONLY_TOOLS: [&str; 5] = ["Read", "Grep", "Glob", "WebSearch", "WebFetch"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub walk: bool,
    /// Plays random actions of the game between the walks.
    pub actions: bool,
    /// Probability that an action follows a pause instead of a walk, 0 to 1.
    pub action_chance: f32,
    /// Gives each walk a random gait: stroll, walk, hurry or dash.
    pub vary_speed: bool,
    /// Lets the pal hop at random moments of a walk.
    pub hops: bool,
    /// Link dashes with the Pegasus Shoes to the edge of the screen now and
    /// then, and bounces back from it.
    pub edge_dash: bool,
    /// Probability that a walk is a dash to the edge, 0 to 1.
    pub edge_dash_chance: f32,
    /// Link's gear grows with the number of completed chats.
    pub gear_progress: bool,
    /// Link uses an item of the game for each tool call of Claude.
    pub tool_reactions: bool,
    /// Calm behavior and a night tint between `night_start` and `night_end`.
    pub day_night: bool,
    /// First hour of the night, 0 to 23, local time.
    pub night_start: u32,
    /// First hour of the day, 0 to 23, local time.
    pub night_end: u32,
    /// Link chases the mouse pointer when it comes near the ground beside him.
    pub chase: bool,
    /// Enemies of the game appear now and then, and Link fights them.
    pub enemies: bool,
    /// Mean time between two enemies, in minutes.
    pub enemy_minutes: f32,
    /// Link sleeps while the user is away.
    pub idle_sleep: bool,
    /// Time without input after which the user counts as away, in minutes.
    pub idle_minutes: f32,
    /// Walk speed in points per second.
    pub walk_speed: f32,
    /// Pause between two walks, in seconds.
    pub pause_min: f32,
    pub pause_max: f32,
    /// Time in which the pal stays in place after the chat was active, in seconds.
    pub chat_rest: f32,
    /// Size of one sheet pixel on the screen.
    pub scale: u32,
    /// Extra distance between the feet of the pal and the bottom of the usable area.
    pub bottom_margin: f32,
    /// Walked distance in sheet pixels for one frame of the walk cycle.
    pub walk_stride: f32,
    /// Gravity of a fall after a drag, in m/s². 16 sheet pixels are 1 m.
    pub gravity: f32,
    pub claude_path: String,
    /// Working directory of `claude`. Empty means the home directory.
    pub working_dir: String,
    pub permission: Permission,
    /// Space-separated tools for `Permission::Custom`.
    pub custom_tools: String,
    /// Models in the selector of the chat panel. `default` means no `--model` flag.
    pub models: Vec<String>,
    pub model: String,
    pub panel_width: f32,
    pub panel_height: f32,
    pub autostart: bool,
    /// Global shortcut that opens or closes the chat panel, such as
    /// `Super+Shift+C`. Empty means no shortcut.
    pub chat_shortcut: String,
    /// Global shortcut that hides or shows the pal. Empty means no shortcut.
    pub pal_shortcut: String,
    pub theme: ChatTheme,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            walk: true,
            actions: true,
            action_chance: 0.5,
            vary_speed: true,
            hops: true,
            edge_dash: true,
            edge_dash_chance: 0.05,
            gear_progress: true,
            tool_reactions: true,
            day_night: true,
            night_start: 22,
            night_end: 7,
            chase: true,
            enemies: true,
            enemy_minutes: 5.0,
            idle_sleep: true,
            idle_minutes: 5.0,
            walk_speed: 40.0,
            pause_min: 3.0,
            pause_max: 10.0,
            chat_rest: 60.0,
            scale: 3,
            bottom_margin: 0.0,
            walk_stride: 2.5,
            gravity: 9.81,
            claude_path: "claude".into(),
            working_dir: String::new(),
            permission: Permission::ChatOnly,
            custom_tools: String::new(),
            models: ["default", "opus", "sonnet", "haiku"]
                .map(String::from)
                .to_vec(),
            model: "default".into(),
            panel_width: 420.0,
            panel_height: 560.0,
            autostart: false,
            chat_shortcut: String::new(),
            pal_shortcut: String::new(),
            theme: ChatTheme::default(),
        }
    }
}

impl Config {
    /// True if `hour` (0 to 23) is a night hour. A night can span midnight.
    /// Equal start and end hours mean a night that lasts all day.
    pub fn is_night_hour(&self, hour: u32) -> bool {
        let (start, end) = (self.night_start % 24, self.night_end % 24);
        if start < end {
            hour >= start && hour < end
        } else {
            hour >= start || hour < end
        }
    }

    /// Clamps the values to the ranges of the settings window, so that a
    /// hand-edited file cannot stop the pal. Values that are not numbers get
    /// the defaults.
    pub fn sanitize(&mut self) {
        let d = Config::default();
        let fix = |v: f32, lo: f32, hi: f32, default: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        };
        self.scale = self.scale.clamp(2, 4);
        self.walk_speed = fix(self.walk_speed, 5.0, 400.0, d.walk_speed);
        self.pause_min = fix(self.pause_min, 0.0, 600.0, d.pause_min);
        self.pause_max = fix(self.pause_max, 0.0, 600.0, d.pause_max);
        self.chat_rest = fix(self.chat_rest, 0.0, 3600.0, d.chat_rest);
        self.walk_stride = fix(self.walk_stride, 0.5, 10.0, d.walk_stride);
        self.gravity = fix(self.gravity, 0.5, 100.0, d.gravity);
        self.bottom_margin = fix(self.bottom_margin, 0.0, 500.0, d.bottom_margin);
        self.action_chance = fix(self.action_chance, 0.0, 1.0, d.action_chance);
        self.edge_dash_chance = fix(self.edge_dash_chance, 0.0, 1.0, d.edge_dash_chance);
        self.idle_minutes = fix(self.idle_minutes, 0.5, 240.0, d.idle_minutes);
        self.enemy_minutes = fix(self.enemy_minutes, 0.5, 240.0, d.enemy_minutes);
        self.night_start %= 24;
        self.night_end %= 24;
        self.panel_width = fix(self.panel_width, 280.0, 8000.0, d.panel_width);
        self.panel_height = fix(self.panel_height, 240.0, 8000.0, d.panel_height);
        if self.models.is_empty() {
            self.models = d.models;
        }
    }

    fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("pixelpal").join("config.toml"))
    }

    /// Loads the configuration file. A missing file gives the defaults. A file
    /// with an error also gives the defaults: it is then copied to
    /// `config.toml.bak` before a later save replaces it, and the error is
    /// returned.
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), None);
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };
        match toml::from_str::<Config>(&text) {
            Ok(mut cfg) => {
                cfg.sanitize();
                (cfg, None)
            }
            Err(e) => {
                let backup = path.with_extension("toml.bak");
                let _ = std::fs::write(&backup, &text);
                let msg = format!(
                    "{} has an error and pixelpal uses the defaults. A copy is in {}.\n{e}",
                    path.display(),
                    backup.display()
                );
                (Self::default(), Some(msg))
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Ok(s) = toml::to_string_pretty(self) {
            let _ = crate::files::write_atomic(&path, s.as_bytes());
        }
    }

    /// The working directory of `claude`. Falls back to the home directory.
    pub fn working_dir(&self) -> Option<PathBuf> {
        let dir = self.working_dir.trim();
        if dir.is_empty() {
            dirs::home_dir()
        } else {
            Some(PathBuf::from(dir))
        }
    }

    pub fn model_arg(&self) -> Option<String> {
        (self.model != "default" && !self.model.is_empty()).then(|| self.model.clone())
    }

    /// Permission and tool flags for `claude`.
    pub fn permission_args(&self) -> Vec<String> {
        let tools = |list: Vec<String>| {
            let mut args = vec!["--tools".to_owned()];
            args.extend(list.iter().cloned());
            args.push("--allowedTools".to_owned());
            args.extend(list);
            args
        };
        match self.permission {
            Permission::ChatOnly => vec!["--tools".into(), String::new()],
            Permission::ReadOnly => tools(READ_ONLY_TOOLS.map(String::from).to_vec()),
            Permission::EditFiles => vec!["--permission-mode".into(), "acceptEdits".into()],
            Permission::Everything => vec!["--permission-mode".into(), "bypassPermissions".into()],
            Permission::Custom => {
                // `claude` splits the list itself and keeps rules such as
                // `Bash(git log:*)` together, so the list is one argument.
                let list = self.custom_tools.trim();
                if list.is_empty() {
                    vec!["--tools".into(), String::new()]
                } else {
                    vec!["--allowedTools".into(), list.to_owned()]
                }
            }
        }
    }

    /// Writes or removes the autostart entry of the desktop session.
    pub fn apply_autostart(&self) -> Result<(), String> {
        let dir = dirs::config_dir()
            .ok_or("no config directory")?
            .join("autostart");
        let file = dir.join("pixelpal.desktop");
        if self.autostart {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            // After a reinstall, the path of the running binary ends with " (deleted)".
            let exe_path = exe.display().to_string();
            let exe_path = exe_path
                .strip_suffix(" (deleted)")
                .unwrap_or(&exe_path)
                .to_owned();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName=pixelpal\n\
                 Comment=Desktop pal with a Claude Code chat\nExec={}\nX-GNOME-Autostart-enabled=true\n",
                crate::files::desktop_exec_arg(&exe_path)
            );
            std::fs::write(&file, entry).map_err(|e| e.to_string())
        } else if file.exists() {
            std::fs::remove_file(&file).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
}

/// UI state of the settings window that is not part of the configuration.
#[derive(Default)]
pub struct SettingsUi {
    new_model: String,
    /// An error of the last OK, for example of the autostart entry.
    pub status: Option<String>,
    /// An error of the font file.
    pub font_error: Option<String>,
    /// An error of the chat shortcut.
    pub shortcut_error: Option<String>,
    /// An error of the pal shortcut.
    pub pal_shortcut_error: Option<String>,
    /// The progress of Link, as one line.
    pub progress: String,
    /// The open file or folder dialog, if any.
    picking: Option<(Pick, Receiver<PickResult>)>,
}

impl SettingsUi {
    /// Opens a file or folder dialog, unless one is open already.
    fn pick(&mut self, ui: &Ui, pick: Pick, start: &std::path::Path) {
        if self.picking.is_none() {
            self.picking = Some((pick, picker::open(pick, start, ui.ctx().clone())));
        }
    }

    /// Takes the result of the open dialog, if it has arrived.
    fn take_pick(&mut self, cfg: &mut Config) {
        let Some((pick, rx)) = &self.picking else {
            return;
        };
        let Ok(result) = rx.try_recv() else { return };
        match (pick, result) {
            (Pick::Folder, Ok(Some(path))) => cfg.working_dir = path,
            (Pick::Font, Ok(Some(path))) => cfg.theme.font_file = path,
            (_, Ok(None)) => {}
            (_, Err(e)) => self.status = Some(e),
        }
        self.picking = None;
    }

    /// Shows the settings and changes `cfg` in place.
    pub fn show(&mut self, ui: &mut Ui, cfg: &mut Config) {
        self.take_pick(cfg);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
            ui.heading("Movement");
            egui::Grid::new("movement").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Walk on the desktop");
                ui.checkbox(&mut cfg.walk, "");
                ui.end_row();
                ui.label("Play actions")
                    .on_hover_text("Sword, spin attack, swimming, items and more from the game, between the walks.");
                ui.checkbox(&mut cfg.actions, "");
                ui.end_row();
                ui.label("Action chance")
                    .on_hover_text("Probability that an action follows a pause instead of a walk.");
                ui.add_enabled(
                    cfg.actions,
                    egui::Slider::new(&mut cfg.action_chance, 0.0..=1.0)
                        .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                );
                ui.end_row();
                ui.label("Walk speed");
                ui.add(DragValue::new(&mut cfg.walk_speed).range(5.0..=400.0).suffix(" px/s"));
                ui.end_row();
                ui.label("Vary the walk speed")
                    .on_hover_text("Each walk is a stroll (½×), a walk (1×), a hurry (1.7×) or a dash with the Pegasus Shoes (3.5×).");
                ui.checkbox(&mut cfg.vary_speed, "");
                ui.end_row();
                ui.label("Hop while walking")
                    .on_hover_text("Link hops at random moments of a walk. His shadow stays on the ground.");
                ui.checkbox(&mut cfg.hops, "");
                ui.end_row();
                ui.label("Dash to the screen edge")
                    .on_hover_text("Now and then, Link dashes with the Pegasus Shoes to the edge of the screen and bounces back from it, as from a wall in the game. This value is the probability that a walk is such a dash.");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut cfg.edge_dash, "");
                    ui.add_enabled(
                        cfg.edge_dash,
                        egui::Slider::new(&mut cfg.edge_dash_chance, 0.0..=1.0)
                            .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                    );
                });
                ui.end_row();
                ui.label("React to Claude's tools")
                    .on_hover_text("Link reads for Read, digs for Edit, hammers for Bash, catches for WebSearch, and gets a shock when a tool fails.");
                ui.checkbox(&mut cfg.tool_reactions, "");
                ui.end_row();
                ui.label("Chase the mouse pointer")
                    .on_hover_text("When the pointer comes near the ground beside Link, he runs to it and catches it with the bug net.");
                ui.checkbox(&mut cfg.chase, "");
                ui.end_row();
                ui.label("Fight enemies")
                    .on_hover_text("Octoroks, soldiers and Buzzblobs appear now and then, Keese at night. Link fights them with his sword. This value is the mean time between two enemies.");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut cfg.enemies, "");
                    ui.add_enabled(
                        cfg.enemies,
                        DragValue::new(&mut cfg.enemy_minutes).range(0.5..=240.0).speed(0.1).suffix(" min"),
                    );
                });
                ui.end_row();
                ui.label("Day and night")
                    .on_hover_text("At night, Link is darker, strolls, and only dozes, sleeps, reads or prays. Equal hours mean a night that lasts all day.");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut cfg.day_night, "");
                    ui.add_enabled_ui(cfg.day_night, |ui| {
                        ui.label("night from");
                        ui.add(DragValue::new(&mut cfg.night_start).range(0..=23).suffix(":00"));
                        ui.label("to");
                        ui.add(DragValue::new(&mut cfg.night_end).range(0..=23).suffix(":00"));
                    });
                });
                ui.end_row();
                ui.label("Sleep when you are away")
                    .on_hover_text("Link sleeps after this time without keyboard or mouse input, and wakes up when you return.");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut cfg.idle_sleep, "");
                    ui.add_enabled(
                        cfg.idle_sleep,
                        DragValue::new(&mut cfg.idle_minutes).range(0.5..=240.0).speed(0.1).suffix(" min"),
                    );
                });
                ui.end_row();
                ui.label("Pause between walks");
                ui.horizontal(|ui| {
                    ui.add(DragValue::new(&mut cfg.pause_min).range(0.0..=600.0).speed(0.2).suffix(" s"));
                    ui.label("to");
                    ui.add(DragValue::new(&mut cfg.pause_max).range(0.0..=600.0).speed(0.2).suffix(" s"));
                });
                ui.end_row();
                ui.label("Rest after a chat")
                    .on_hover_text("Link stays in place for this time after the chat panel closes or Claude answers.");
                ui.add(DragValue::new(&mut cfg.chat_rest).range(0.0..=3600.0).speed(1.0).suffix(" s"));
                ui.end_row();
                ui.label("Scale");
                ui.horizontal(|ui| {
                    for s in 2..=4 {
                        ui.radio_value(&mut cfg.scale, s, format!("{s}×"));
                    }
                });
                ui.end_row();
                ui.label("Walk distance per frame")
                    .on_hover_text("Larger values give slower steps for the same walk speed.");
                ui.add(DragValue::new(&mut cfg.walk_stride).range(0.5..=10.0).speed(0.05).suffix(" px"));
                ui.end_row();
                ui.label("Gravity")
                    .on_hover_text("Acceleration of a fall after a drag. 9.81 m/s² is the gravity of the earth.");
                ui.add(DragValue::new(&mut cfg.gravity).range(0.5..=100.0).speed(0.1).suffix(" m/s²"));
                ui.end_row();
                ui.label("Bottom margin");
                ui.add(DragValue::new(&mut cfg.bottom_margin).range(0.0..=500.0).suffix(" px"));
                ui.end_row();
                ui.label("Gear grows with chats")
                    .on_hover_text("Completed chats unlock the Master Sword, the Blue Mail, the Tempered Sword and the Golden Sword.");
                ui.vertical(|ui| {
                    ui.checkbox(&mut cfg.gear_progress, "");
                    ui.label(&self.progress);
                });
                ui.end_row();
            });

            ui.add_space(12.0);
            ui.heading("Claude");
            egui::Grid::new("claude").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("claude binary");
                ui.text_edit_singleline(&mut cfg.claude_path);
                ui.end_row();
                ui.label("Working directory");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut cfg.working_dir)
                            .hint_text("home directory")
                            .desired_width(200.0),
                    );
                    if ui.button("Browse…").clicked() {
                        let start = cfg.working_dir().unwrap_or_default();
                        self.pick(ui, Pick::Folder, &start);
                    }
                });
                ui.end_row();
                ui.label("Permissions");
                ComboBox::from_id_salt("permission")
                    .selected_text(cfg.permission.label())
                    .show_ui(ui, |ui| {
                        for p in Permission::ALL {
                            ui.selectable_value(&mut cfg.permission, p, p.label());
                        }
                    });
                ui.end_row();
                if cfg.permission == Permission::Custom {
                    ui.label("Allowed tools");
                    ui.add(
                        egui::TextEdit::singleline(&mut cfg.custom_tools)
                            .hint_text("Read Grep Bash(git log:*)"),
                    );
                    ui.end_row();
                }
            });
            let note = match cfg.permission {
                Permission::ChatOnly => "Claude has no tools. It only writes text.".to_owned(),
                Permission::ReadOnly => format!("Claude can use: {}.", READ_ONLY_TOOLS.join(", ")),
                Permission::EditFiles => {
                    "Claude can change files in the working directory without a question.".to_owned()
                }
                Permission::Everything => String::new(),
                Permission::Custom => {
                    "Claude can use the listed tools without a question. Other tools that need a permission fail."
                        .to_owned()
                }
            };
            if cfg.permission == Permission::Everything {
                ui.label(
                    RichText::new(
                        "Warning: Claude can change files and run commands without a question.",
                    )
                    .color(ui.visuals().warn_fg_color),
                );
            } else {
                ui.label(RichText::new(note).weak());
            }

            ui.add_space(8.0);
            ui.label("Models in the chat panel");
            let mut remove = None;
            for (i, m) in cfg.models.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(m);
                    if cfg.models.len() > 1 && ui.small_button("×").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                cfg.models.remove(i);
                if !cfg.models.contains(&cfg.model) {
                    cfg.model = cfg.models[0].clone();
                }
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_model)
                        .hint_text("claude-opus-5-5")
                        .desired_width(200.0),
                );
                let name = self.new_model.trim().to_owned();
                if ui.button("Add").clicked() && !name.is_empty() && !cfg.models.contains(&name) {
                    cfg.models.push(name);
                    self.new_model.clear();
                }
            });

            ui.add_space(12.0);
            ui.heading("Chat panel");
            egui::Grid::new("panel").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Width");
                ui.add(DragValue::new(&mut cfg.panel_width).range(280.0..=1600.0).suffix(" px"));
                ui.end_row();
                ui.label("Height");
                ui.add(DragValue::new(&mut cfg.panel_height).range(240.0..=1400.0).suffix(" px"));
                ui.end_row();
            });

            ui.add_space(8.0);
            ui.label("Theme");
            let t = &mut cfg.theme;
            egui::Grid::new("theme").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Preset");
                ui.horizontal(|ui| {
                    if ui.button("Dark").clicked() {
                        *t = ChatTheme { font_file: t.font_file.clone(), font_size: t.font_size, opacity: t.opacity, ..ChatTheme::dark() };
                    }
                    if ui.button("Light").clicked() {
                        *t = ChatTheme { font_file: t.font_file.clone(), font_size: t.font_size, opacity: t.opacity, ..ChatTheme::light() };
                    }
                });
                ui.end_row();
                ui.label("Widget style");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut t.dark, true, "Dark");
                    ui.radio_value(&mut t.dark, false, "Light");
                });
                ui.end_row();
                for (label, color) in [
                    ("Background", &mut t.background),
                    ("Text", &mut t.text),
                    ("Input background", &mut t.input_background),
                    ("Your messages", &mut t.user_message),
                    ("Accent", &mut t.accent),
                ] {
                    ui.label(label);
                    ui.color_edit_button_srgb(color);
                    ui.end_row();
                }
                ui.label("Background opacity");
                ui.add(egui::Slider::new(&mut t.opacity, 0.1..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                ui.end_row();
                ui.label("Font size");
                ui.add(DragValue::new(&mut t.font_size).range(8.0..=40.0).speed(0.2).suffix(" pt"));
                ui.end_row();
                ui.label("Font file");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut t.font_file)
                            .hint_text("built-in font")
                            .desired_width(200.0),
                    );
                    if ui.button("Browse…").clicked() {
                        let start = std::path::Path::new(t.font_file.trim())
                            .parent()
                            .filter(|d| d.is_dir())
                            .map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from("/usr/share/fonts"));
                        self.pick(ui, Pick::Font, &start);
                    }
                    if !t.font_file.is_empty() && ui.small_button("×").on_hover_text("Built-in font").clicked() {
                        t.font_file.clear();
                    }
                });
                ui.end_row();
            });
            if let Some(e) = &self.font_error {
                ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
            }

            ui.add_space(12.0);
            ui.heading("General");
            ui.checkbox(&mut cfg.autostart, "Start pixelpal at login");
            ui.horizontal(|ui| {
                ui.label("Chat shortcut")
                    .on_hover_text("A key with one or more modifiers: Ctrl, Alt, Shift, Super. The shortcut works in all programs and opens or closes the chat.");
                ui.add(
                    egui::TextEdit::singleline(&mut cfg.chat_shortcut)
                        .hint_text("Super+Shift+C")
                        .desired_width(160.0),
                );
            });
            if let Some(e) = &self.shortcut_error {
                ui.label(RichText::new(format!("Chat shortcut: {e}")).color(ui.visuals().error_fg_color));
            }
            ui.horizontal(|ui| {
                ui.label("Pal shortcut")
                    .on_hover_text("A key with one or more modifiers: Ctrl, Alt, Shift, Super. The shortcut works in all programs and hides or shows the pal.");
                ui.add(
                    egui::TextEdit::singleline(&mut cfg.pal_shortcut)
                        .hint_text("Super+Shift+H")
                        .desired_width(160.0),
                );
            });
            if let Some(e) = &self.pal_shortcut_error {
                ui.label(RichText::new(format!("Pal shortcut: {e}")).color(ui.visuals().error_fg_color));
            }
            if let Some(s) = &self.status {
                ui.label(RichText::new(s).color(ui.visuals().error_fg_color));
            }
            ui.add_space(8.0);
            if let Some(p) = Config::path() {
                ui.label(RichText::new(format!("Settings file: {}", p.display())).weak().small());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_hours_span_midnight() {
        let cfg = Config::default();
        assert!(cfg.is_night_hour(23) && cfg.is_night_hour(0) && cfg.is_night_hour(6));
        assert!(!cfg.is_night_hour(7) && !cfg.is_night_hour(12) && !cfg.is_night_hour(21));
        let day = Config {
            night_start: 1,
            night_end: 5,
            ..Config::default()
        };
        assert!(day.is_night_hour(1) && day.is_night_hour(4) && !day.is_night_hour(5));
        let always = Config {
            night_start: 3,
            night_end: 3,
            ..Config::default()
        };
        assert!((0..24).all(|h| always.is_night_hour(h)));
    }

    #[test]
    fn sanitize_repairs_bad_values() {
        let mut cfg = Config {
            scale: 0,
            gravity: -1.0,
            walk_speed: 0.0,
            idle_minutes: f32::NAN,
            panel_width: -5.0,
            models: Vec::new(),
            ..Config::default()
        };
        cfg.sanitize();
        assert_eq!(cfg.scale, 2);
        assert_eq!(cfg.gravity, 0.5);
        assert_eq!(cfg.walk_speed, 5.0);
        assert_eq!(cfg.idle_minutes, Config::default().idle_minutes);
        assert_eq!(cfg.panel_width, 280.0);
        assert!(!cfg.models.is_empty());
    }
}
