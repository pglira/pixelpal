//! File helpers.

use std::io::Write;
use std::path::Path;

/// Writes `contents` to `path` atomically: to a temporary file in the same
/// directory, which then replaces the file. If `path` is a symlink, the target
/// of the symlink is replaced and the symlink stays.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp_name = target.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = target.with_file_name(tmp_name);
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    std::fs::rename(&tmp, &target)
}

/// Escapes a path for the `Exec` key of a desktop entry. The path is quoted,
/// `"`, `` ` ``, `$` and `\` get a backslash, and `%` is doubled. Then every
/// backslash is doubled, because the general escape rule for desktop entry
/// strings applies before the quoting rule.
pub fn desktop_exec_arg(path: &str) -> String {
    let mut s = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                s.push('\\');
                s.push(c);
            }
            '%' => s.push_str("%%"),
            _ => s.push(c),
        }
    }
    s.push('"');
    s.replace('\\', "\\\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_keeps_symlink() {
        let dir = std::env::temp_dir().join(format!("pixelpal-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.toml");
        let link = dir.join("link.toml");
        std::fs::write(&target, "old").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_atomic(&link, b"new").unwrap();

        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn desktop_exec_arg_escapes() {
        assert_eq!(desktop_exec_arg("/a b/pal"), "\"/a b/pal\"");
        assert_eq!(
            desktop_exec_arg("/a$\"`\\%"),
            "\"/a\\\\$\\\\\"\\\\`\\\\\\\\%%\""
        );
    }
}
