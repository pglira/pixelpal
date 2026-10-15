//! The configuration file and the settings window.

use std::path::PathBuf;

use egui::{ComboBox, DragValue, RichText, Ui};
use serde::{Deserialize, Serialize};

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
    /// Walk speed in points per second.
    pub walk_speed: f32,
    /// Pause between two walks, in seconds.
    pub pause_min: f32,
    pub pause_max: f32,
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
    pub theme: ChatTheme,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            walk: true,
            walk_speed: 40.0,
            pause_min: 3.0,
            pause_max: 10.0,
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
            theme: ChatTheme::default(),
        }
    }
}

impl Config {
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
        match toml::from_str(&text) {
            Ok(cfg) => (cfg, None),
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
}

impl SettingsUi {
    /// Shows the settings and changes `cfg` in place.
    pub fn show(&mut self, ui: &mut Ui, cfg: &mut Config) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
            ui.heading("Movement");
            egui::Grid::new("movement").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Walk on the desktop");
                ui.checkbox(&mut cfg.walk, "");
                ui.end_row();
                ui.label("Walk speed");
                ui.add(DragValue::new(&mut cfg.walk_speed).range(5.0..=400.0).suffix(" px/s"));
                ui.end_row();
                ui.label("Pause between walks");
                ui.horizontal(|ui| {
                    ui.add(DragValue::new(&mut cfg.pause_min).range(0.0..=600.0).speed(0.2).suffix(" s"));
                    ui.label("to");
                    ui.add(DragValue::new(&mut cfg.pause_max).range(0.0..=600.0).speed(0.2).suffix(" s"));
                });
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
                        let mut dialog = rfd::FileDialog::new();
                        if let Some(dir) = cfg.working_dir() {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(dir) = dialog.pick_folder() {
                            cfg.working_dir = dir.display().to_string();
                        }
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
                        let mut dialog = rfd::FileDialog::new().add_filter("Fonts", &["ttf", "otf", "TTF", "OTF"]);
                        if let Some(dir) = std::path::Path::new(t.font_file.trim()).parent() {
                            dialog = dialog.set_directory(dir);
                        } else if std::path::Path::new("/usr/share/fonts").is_dir() {
                            dialog = dialog.set_directory("/usr/share/fonts");
                        }
                        if let Some(file) = dialog.pick_file() {
                            t.font_file = file.display().to_string();
                        }
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
