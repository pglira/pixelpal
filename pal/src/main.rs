//! pixelpal: a desktop pal that walks along the bottom of the screen and chats
//! through the Claude Code CLI.
//!
//! This program draws the pal and the enemies in X11 windows of its own. The
//! program `pixelpal-chat` shows the chat panel, the menu and the settings.

mod actions;
mod app;
mod canvas;
mod chat_process;
mod enemy;
mod geom;
mod hotkey;
mod link;
mod pal;
mod rom;
mod tray;
mod wake;
mod x11;

use pixelpal_core::config::Config;

use crate::x11::X11;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let option = args.get(1).map(String::as_str);

    // `pixelpal --import-rom <file>` checks and stores the ROM and exits.
    if option == Some("--import-rom") {
        let Some(path) = args.get(2) else {
            eprintln!("usage: pixelpal --import-rom <file>");
            std::process::exit(2);
        };
        match rom::import(std::path::Path::new(path)) {
            Ok(_) => println!("pixelpal: the ROM is imported."),
            Err(e) => {
                eprintln!("pixelpal: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // All graphics come from the ROM. Without it, pixelpal asks for the file.
    if let Err(e) = rom::obtain().and_then(|data| link::init(&data)) {
        eprintln!("pixelpal: {e}");
        std::process::exit(1);
    }

    // `pixelpal --write-icon <file.png>` writes the application icon and exits.
    if option == Some("--write-icon") {
        let Some(path) = args.get(2) else {
            eprintln!("usage: pixelpal --write-icon <file.png>");
            std::process::exit(2);
        };
        let (w, h, rgba) = link::icon(96);
        if let Err(e) = image::save_buffer(path, &rgba, w, h, image::ColorType::Rgba8) {
            eprintln!("pixelpal: cannot write {path}: {e}");
            std::process::exit(1);
        }
        return;
    }

    let (cfg, cfg_error) = Config::load();
    if let Some(e) = &cfg_error {
        eprintln!("pixelpal: {e}");
    }
    let app = X11::connect().and_then(|x11| app::App::new(cfg, x11));
    match app {
        Ok(app) => app.run(),
        Err(e) => {
            eprintln!("pixelpal: {e}");
            std::process::exit(1);
        }
    }
}
