//! CLI / Explorer argv helpers: open files passed to `orchid.exe`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Collect existing file paths from process arguments (skip the program name).
///
/// Skips flag-like tokens (`-…` / `--…`). Relative paths are resolved against
/// the current working directory. Missing paths are ignored.
#[must_use]
pub fn collect_cli_open_paths<I, S>(args: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut out = Vec::new();
    for arg in args {
        let os = arg.as_ref();
        if os.is_empty() {
            continue;
        }
        if let Some(s) = os.to_str() {
            if s.starts_with('-') {
                continue;
            }
        }
        let candidate = PathBuf::from(os);
        let resolved = if candidate.is_absolute() {
            candidate
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&candidate))
                .unwrap_or(candidate)
        };
        if resolved.is_file() {
            out.push(resolved.canonicalize().unwrap_or(resolved));
        }
    }
    out
}

/// True when `path` should open in the document/viewer pipeline as an Orchid container.
#[must_use]
pub fn is_orchid_cli_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("orchid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_flags_and_missing() {
        let paths = collect_cli_open_paths(["--help", "-v", "nope.orchid"]);
        assert!(paths.is_empty());
    }

    #[test]
    fn collects_existing_file() {
        let dir = std::env::temp_dir().join(format!("orchid-cli-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("note.orchid");
        std::fs::write(&file, b"ORCD").unwrap();
        let paths = collect_cli_open_paths([file.as_os_str()]);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("note.orchid"));
        assert!(is_orchid_cli_path(&paths[0]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
