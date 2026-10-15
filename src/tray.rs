//! The tray icon (StatusNotifierItem) with a menu.

use std::sync::mpsc::{Receiver, Sender, channel};

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, ToolTip};

pub enum TrayCommand {
    TogglePal,
    ToggleChat,
    OpenSettings,
    Quit,
}

struct PalTray {
    tx: Sender<TrayCommand>,
    ctx: egui::Context,
    icon: Icon,
    pal_visible: bool,
}

impl PalTray {
    fn send(&self, cmd: TrayCommand) {
        let _ = self.tx.send(cmd);
        self.ctx.request_repaint();
    }
}

impl ksni::Tray for PalTray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "pixelpal".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![self.icon.clone()]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "pixelpal".into(),
            description: "Click: open or close the chat".into(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayCommand::ToggleChat);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let item = |label: &str, cmd: fn() -> TrayCommand| -> MenuItem<Self> {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |t: &mut Self| t.send(cmd())),
                ..Default::default()
            }
            .into()
        };
        let pal = if self.pal_visible {
            "Hide pal"
        } else {
            "Show pal"
        };
        vec![
            item(pal, || TrayCommand::TogglePal),
            item("Open or close the chat", || TrayCommand::ToggleChat),
            item("Settings…", || TrayCommand::OpenSettings),
            MenuItem::Separator,
            item("Quit", || TrayCommand::Quit),
        ]
    }
}

/// The running tray icon. The icon disappears when this value is dropped.
pub struct Tray {
    handle: Handle<PalTray>,
    rx: Receiver<TrayCommand>,
}

impl Tray {
    /// Starts the tray icon. `rgba` is the icon image with straight alpha.
    pub fn spawn(ctx: egui::Context, width: u32, height: u32, rgba: &[u8]) -> Result<Self, String> {
        // StatusNotifierItem icons are ARGB32 in network byte order.
        let data = rgba
            .chunks_exact(4)
            .flat_map(|p| [p[3], p[0], p[1], p[2]])
            .collect();
        let icon = Icon {
            width: width as i32,
            height: height as i32,
            data,
        };
        let (tx, rx) = channel();
        let handle = PalTray {
            tx,
            ctx,
            icon,
            pal_visible: true,
        }
        .spawn()
        .map_err(|e| e.to_string())?;
        Ok(Self { handle, rx })
    }

    /// Sets the label of the hide/show item of the menu.
    pub fn set_pal_visible(&self, visible: bool) {
        self.handle.update(|t| t.pal_visible = visible);
    }

    pub fn try_recv(&self) -> Option<TrayCommand> {
        self.rx.try_recv().ok()
    }
}
