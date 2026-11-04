//! The configuration file.

use std::path::{Path, PathBuf};

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
    pub const ALL: [Permission; 5] = [
        Permission::ChatOnly,
        Permission::ReadOnly,
        Permission::EditFiles,
        Permission::Everything,
        Permission::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Permission::ChatOnly => "Chat only (no tools)",
            Permission::ReadOnly => "Read-only",
            Permission::EditFiles => "Edit files",
            Permission::Everything => "Everything (no questions)",
            Permission::Custom => "Custom",
        }
    }
}

pub const READ_ONLY_TOOLS: [&str; 5] = ["Read", "Grep", "Glob", "WebSearch", "WebFetch"];

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
    /// Walk speed in pixels per second.
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
    /// Instructions that `claude` gets as an addition to its system prompt
    /// in each chat. Empty means no instructions.
    pub instructions: String,
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
            instructions: String::new(),
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

    /// The configuration file: `pixelpal/config.toml` in the configuration
    /// directory of the user.
    pub fn path() -> Option<PathBuf> {
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

    /// The flag that adds the custom instructions to the system prompt of
    /// `claude`. Empty instructions give no flag. The flag and the text are
    /// one argument, so that a text that starts with `-` stays a value.
    pub fn instruction_args(&self) -> Vec<String> {
        let text = self.instructions.trim();
        if text.is_empty() {
            Vec::new()
        } else {
            vec![format!("--append-system-prompt={text}")]
        }
    }

    /// Writes or removes the autostart entry of the desktop session. The
    /// entry starts the program `exe`.
    pub fn apply_autostart(&self, exe: &Path) -> Result<(), String> {
        let dir = dirs::config_dir()
            .ok_or("no config directory")?
            .join("autostart");
        let file = dir.join("pixelpal.desktop");
        if self.autostart {
            let exe_path = exe.display().to_string();
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

    #[test]
    fn instructions_go_into_the_system_prompt() {
        let mut cfg = Config::default();
        assert!(cfg.instruction_args().is_empty());
        cfg.instructions = "  \n ".into();
        assert!(cfg.instruction_args().is_empty());
        cfg.instructions = "\nBe brief.\nUse lists.\n".into();
        assert_eq!(
            cfg.instruction_args(),
            ["--append-system-prompt=Be brief.\nUse lists."]
        );
    }
}
