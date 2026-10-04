//! Filesystem scan and delete. Symlinks and junctions are skipped so a link
//! inside a temp folder cannot pull the cleaner outside that folder.

use std::fs::{self, DirEntry};
use std::path::Path;

/// Bytes and files a scan or a clean accounted for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanStats {
    /// Files matched (scan) or removed (clean).
    pub files: u64,
    /// Total size of those files.
    pub bytes: u64,
    /// Entries that could not be read or removed (typically locked).
    pub skipped: u64,
}

impl CleanStats {
    fn add(&mut self, other: Self) {
        self.files += other.files;
        self.bytes += other.bytes;
        self.skipped += other.skipped;
    }
}

/// A filesystem cleanup step. Paths are produced by the resolver, not typed in.
#[derive(Debug, Clone)]
pub enum FsJob {
    /// Delete children of `dir` and keep `dir` itself.
    Descend { dir: std::path::PathBuf },
    /// Delete files directly in `dir` whose names start with `prefix`.
    Prefix {
        dir: std::path::PathBuf,
        prefix: String,
    },
    /// Delete files directly in `dir` whose names end with `suffix`.
    Suffix {
        dir: std::path::PathBuf,
        suffix: String,
    },
    /// Delete these files if they exist.
    Files { paths: Vec<std::path::PathBuf> },
}

/// Count files the jobs would remove.
#[must_use]
pub fn scan_jobs(jobs: &[FsJob]) -> CleanStats {
    let mut stats = CleanStats::default();
    for job in jobs {
        stats.add(match job {
            FsJob::Descend { dir } => scan_children(dir),
            FsJob::Prefix { dir, prefix } => scan_named(dir, |name| {
                name.to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
            }),
            FsJob::Suffix { dir, suffix } => scan_named(dir, |name| {
                name.to_ascii_lowercase()
                    .ends_with(&suffix.to_ascii_lowercase())
            }),
            FsJob::Files { paths } => scan_files(paths),
        });
    }
    stats
}

/// Remove files the jobs name. The parent directories of [`FsJob::Descend`] stay.
#[must_use]
pub fn clean_jobs(jobs: &[FsJob]) -> CleanStats {
    let mut stats = CleanStats::default();
    for job in jobs {
        stats.add(match job {
            FsJob::Descend { dir } => clean_children(dir),
            FsJob::Prefix { dir, prefix } => clean_named(dir, |name| {
                name.to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
            }),
            FsJob::Suffix { dir, suffix } => clean_named(dir, |name| {
                name.to_ascii_lowercase()
                    .ends_with(&suffix.to_ascii_lowercase())
            }),
            FsJob::Files { paths } => clean_files(paths),
        });
    }
    stats
}

fn scan_children(dir: &Path) -> CleanStats {
    let mut stats = CleanStats::default();
    walk(dir, true, &mut stats, false);
    stats
}

fn clean_children(dir: &Path) -> CleanStats {
    let mut stats = CleanStats::default();
    walk(dir, true, &mut stats, true);
    stats
}

fn scan_named(dir: &Path, pred: impl Fn(&str) -> bool) -> CleanStats {
    let mut stats = CleanStats::default();
    named(dir, &pred, &mut stats, false);
    stats
}

fn clean_named(dir: &Path, pred: impl Fn(&str) -> bool) -> CleanStats {
    let mut stats = CleanStats::default();
    named(dir, &pred, &mut stats, true);
    stats
}

fn scan_files(paths: &[std::path::PathBuf]) -> CleanStats {
    let mut stats = CleanStats::default();
    for path in paths {
        account_file(path, &mut stats, false);
    }
    stats
}

fn clean_files(paths: &[std::path::PathBuf]) -> CleanStats {
    let mut stats = CleanStats::default();
    for path in paths {
        account_file(path, &mut stats, true);
    }
    stats
}

fn named(dir: &Path, pred: &impl Fn(&str) -> bool, stats: &mut CleanStats, delete: bool) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        if is_link(&entry) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            stats.skipped += 1;
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !pred(&name) {
            continue;
        }
        account_meta(&entry.path(), meta.len(), stats, delete);
    }
}

fn walk(dir: &Path, recurse: bool, stats: &mut CleanStats, delete: bool) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        if is_link(&entry) {
            stats.skipped += 1;
            continue;
        }
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            stats.skipped += 1;
            continue;
        };
        if meta.is_dir() {
            if recurse {
                walk(&path, true, stats, delete);
            }
            if delete && fs::remove_dir(&path).is_err() {
                // Still holding a locked file. Counted when that file was skipped.
            }
        } else if meta.is_file() {
            account_meta(&path, meta.len(), stats, delete);
        }
    }
}

fn account_file(path: &Path, stats: &mut CleanStats, delete: bool) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        stats.skipped += 1;
        return;
    }
    account_meta(path, meta.len(), stats, delete);
}

fn account_meta(path: &Path, len: u64, stats: &mut CleanStats, delete: bool) {
    if !delete {
        stats.files += 1;
        stats.bytes += len;
        return;
    }
    clear_readonly(path);
    if fs::remove_file(path).is_ok() {
        stats.files += 1;
        stats.bytes += len;
    } else {
        stats.skipped += 1;
    }
}

fn is_link(entry: &DirEntry) -> bool {
    entry.file_type().is_ok_and(|t| t.is_symlink())
}

fn clear_readonly(path: &Path) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    let mut perms = meta.permissions();
    if perms.readonly() {
        perms.set_readonly(false);
        let _ = fs::set_permissions(path, perms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn touch(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut f = File::create(path).expect("create");
        f.write_all(bytes).expect("write");
    }

    #[test]
    fn descend_removes_children_and_keeps_the_root() {
        let root = tempfile::tempdir().expect("temp");
        let dir = root.path().join("temp");
        touch(&dir.join("a.txt"), b"hello");
        touch(&dir.join("sub").join("b.txt"), b"world!!");
        let jobs = [FsJob::Descend { dir: dir.clone() }];
        let scanned = scan_jobs(&jobs);
        assert_eq!(scanned.files, 2);
        assert_eq!(scanned.bytes, 12);
        let cleaned = clean_jobs(&jobs);
        assert_eq!(cleaned.files, 2);
        assert!(dir.is_dir());
        assert!(!dir.join("a.txt").exists());
        assert!(!dir.join("sub").join("b.txt").exists());
        assert_eq!(scan_jobs(&jobs).files, 0);
    }

    #[test]
    fn prefix_and_suffix_leave_unrelated_files() {
        let root = tempfile::tempdir().expect("temp");
        let dir = root.path().join("explorer");
        touch(&dir.join("thumbcache_32.db"), b"xxxx");
        touch(&dir.join("iconcache.db"), b"yy");
        touch(&dir.join("note.lnk"), b"z");
        let jobs = [
            FsJob::Prefix {
                dir: dir.clone(),
                prefix: "thumbcache_".into(),
            },
            FsJob::Suffix {
                dir: dir.clone(),
                suffix: ".lnk".into(),
            },
        ];
        assert_eq!(clean_jobs(&jobs).files, 2);
        assert!(dir.join("iconcache.db").is_file());
        assert!(!dir.join("thumbcache_32.db").exists());
        assert!(!dir.join("note.lnk").exists());
    }
}
