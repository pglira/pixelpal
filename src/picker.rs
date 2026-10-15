//! File and folder dialogs through `zenity` or `kdialog`, on a thread, so that
//! the window stays responsive and pixelpal needs no GUI toolkit of its own.

use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Folder,
    Font,
}

/// The result of a dialog: the chosen path, `None` for a cancel, or an error.
pub type PickResult = Result<Option<String>, String>;

/// Opens a dialog in `start`. The result arrives on the returned channel;
/// `ctx` repaints when it arrives.
pub fn open(pick: Pick, start: &Path, ctx: egui::Context) -> Receiver<PickResult> {
    let (tx, rx) = channel();
    let start = start.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(run(pick, &start));
        ctx.request_repaint();
    });
    rx
}

fn run(pick: Pick, start: &Path) -> PickResult {
    // zenity selects a start folder by a path with a trailing slash.
    let mut dir = start.display().to_string();
    if !dir.ends_with('/') {
        dir.push('/');
    }
    let zenity = match pick {
        Pick::Folder => vec!["--file-selection", "--directory"],
        Pick::Font => vec![
            "--file-selection",
            "--file-filter=Fonts | *.ttf *.otf *.TTF *.OTF",
        ],
    };
    let kdialog = match pick {
        Pick::Folder => vec!["--getexistingdirectory", dir.as_str()],
        Pick::Font => vec!["--getopenfilename", dir.as_str(), "*.ttf *.otf *.TTF *.OTF"],
    };
    let filename = format!("--filename={dir}");
    let tries = [
        ("zenity", [zenity, vec![filename.as_str()]].concat()),
        ("kdialog", kdialog),
    ];
    for (program, args) in tries {
        match Command::new(program).args(&args).output() {
            // Exit code 1 is a cancel for both programs.
            Ok(out) if out.status.success() => {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
                return Ok((!path.is_empty()).then_some(path));
            }
            Ok(_) => return Ok(None),
            Err(_) => continue,
        }
    }
    Err("No file dialog found. Install zenity or kdialog, or type the path.".into())
}
