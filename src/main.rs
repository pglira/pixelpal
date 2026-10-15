//! pixelpal: a desktop pal that walks along the bottom of the screen and chats
//! through the Claude Code CLI.

mod actions;
mod app;
mod chat;
mod claude;
mod files;
mod history;
mod link;
mod pal;
mod picker;
mod progress;
mod settings;
mod theme;
mod tray;
mod workarea;

use egui::{ViewportBuilder, pos2};

use crate::pal::{Area, Pal};
use crate::settings::Config;
use crate::workarea::X11;

fn main() -> eframe::Result {
    // `pixelpal --write-icon <file.png>` writes the application icon and exits.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--write-icon") {
        let Some(path) = args.get(2) else {
            eprintln!("usage: pixelpal --write-icon <file.png>");
            std::process::exit(2);
        };
        let (w, h, rgba) = link::icon(96);
        if let Err(e) = image::save_buffer(path, &rgba, w, h, image::ColorType::Rgba8) {
            eprintln!("pixelpal: cannot write {path}: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let (cfg, cfg_error) = Config::load();
    if let Some(e) = &cfg_error {
        eprintln!("pixelpal: {e}");
    }
    let x11 = X11::connect();

    // Start on the monitor under the pointer, at a random x position, above the ground.
    // The pal then falls down to the bottom edge.
    let (px, py) = x11.as_ref().and_then(X11::pointer).unwrap_or((0, 0));
    let area = x11
        .as_ref()
        .and_then(|x| x.work_area_at(px, py))
        .map(|r| Area {
            left: r.x as f32,
            top: r.y as f32,
            right: (r.x + r.w) as f32,
            bottom: (r.y + r.h) as f32,
        })
        .unwrap_or(Area {
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
    );

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title(app::PAL_TITLE)
            .with_app_id("pixelpal")
            .with_inner_size(size)
            .with_position(start)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false),
        ..Default::default()
    };
    eframe::run_native(
        "pixelpal",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::PalApp::new(
                cc, cfg, cfg_error, x11, start, area,
            )))
        }),
    )
}
