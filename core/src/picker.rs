//! File and folder dialogs through `zenity` or `kdialog`, so that pixelpal
//! needs no GUI toolkit for them. Each dialog blocks until the user closes it.

use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Folder,
    Font,
}

/// The result of a dialog: the chosen path, `None` for a cancel, or an error.
pub type PickResult = Result<Option<String>, String>;

/// Opens a dialog for a ROM file and waits for it.
pub fn pick_rom(title: &str, start: &Path) -> PickResult {
    let dir = format!("{}/", start.display());
    let zenity = vec![
        "--file-selection".to_owned(),
        format!("--title={title}"),
        "--file-filter=ROM files | *.sfc *.smc *.SFC *.SMC".to_owned(),
        "--file-filter=All files | *".to_owned(),
        format!("--filename={dir}"),
    ];
    let kdialog = vec![
        "--title".to_owned(),
        title.to_owned(),
        "--getopenfilename".to_owned(),
        dir,
        "*.sfc *.smc *.SFC *.SMC".to_owned(),
    ];
    dialog(&[("zenity", zenity), ("kdialog", kdialog)])
}

/// Shows an error message in a dialog and waits until the user closes it.
/// Without a dialog program, writes the message to stderr.
pub fn show_error(text: &str) {
    eprintln!("pixelpal: {text}");
    let tries = [
        ("zenity", ["--error", "--no-markup", "--text"]),
        ("kdialog", ["--title", "pixelpal", "--error"]),
    ];
    for (program, args) in tries {
        if Command::new(program).args(args).arg(text).output().is_ok() {
            return;
        }
    }
}

/// Opens a folder or font dialog in `start` and waits for it.
pub fn pick(pick: Pick, start: &Path) -> PickResult {
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
    let owned = |v: Vec<&str>| v.into_iter().map(str::to_owned).collect();
    dialog(&[
        ("zenity", owned([zenity, vec![filename.as_str()]].concat())),
        ("kdialog", owned(kdialog)),
    ])
}

/// Runs the first dialog program that exists, and returns the path that it
/// prints.
fn dialog(tries: &[(&str, Vec<String>)]) -> PickResult {
    for (program, args) in tries {
        match Command::new(program).args(args).output() {
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
