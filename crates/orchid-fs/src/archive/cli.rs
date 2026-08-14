//! Optional 7-Zip CLI backend for formats the Rust crates cannot write
//! (RAR / CAB / ISO / ACE / ARJ / LZH, password, volumes, SFX).

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{TimeZone, Utc};

use crate::archive::types::{sanitise_entry_path, ArchiveEntry, ArchiveTestReport};
use crate::error::{FsError, Result};

/// Locate `7z` / `7z.exe`. Honours `ORCHID_7Z` and common install paths.
#[must_use]
pub fn find_7z() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ORCHID_7Z") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    for name in ["7z", "7z.exe", "7zz"] {
        if let Some(p) = which(name) {
            return Some(p);
        }
    }
    for p in [
        r"C:\Program Files\7-Zip\7z.exe",
        r"C:\Program Files (x86)\7-Zip\7z.exe",
        "/usr/bin/7z",
        "/usr/local/bin/7z",
        "/opt/homebrew/bin/7z",
    ] {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

fn run_7z(args: &[&str]) -> Result<String> {
    let bin = find_7z().ok_or_else(|| {
        FsError::UnsupportedArchive(
            "7-Zip is not installed. Install 7-Zip or set ORCHID_7Z.".into(),
        )
    })?;
    let mut cmd = Command::new(&bin);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if out.status.success() {
        Ok(format!("{stdout}{stderr}"))
    } else {
        Err(FsError::CorruptArchive(format!(
            "7z failed: {stderr}{stdout}"
        )))
    }
}

/// List archive contents via `7z l`.
pub fn list(path: &Path) -> Result<Vec<ArchiveEntry>> {
    let text = run_7z(&["l", "-ba", "-slt", &path.to_string_lossy()])?;
    Ok(parse_slt(&text))
}

fn parse_slt(text: &str) -> Vec<ArchiveEntry> {
    let mut out = Vec::new();
    let mut path = String::new();
    let mut size = 0_u64;
    let mut packed = None;
    let mut is_dir = false;
    let mut modified = None;
    let flush = |out: &mut Vec<ArchiveEntry>,
                 path: &mut String,
                 size: &mut u64,
                 packed: &mut Option<u64>,
                 is_dir: &mut bool,
                 modified: &mut Option<chrono::DateTime<Utc>>| {
        if path.is_empty() {
            return;
        }
        if let Some(safe) = sanitise_entry_path(path) {
            out.push(ArchiveEntry {
                path: safe,
                size: *size,
                compressed_size: *packed,
                modified: *modified,
                is_dir: *is_dir,
                crc32: None,
            });
        }
        path.clear();
        *size = 0;
        *packed = None;
        *is_dir = false;
        *modified = None;
    };
    for line in text.lines() {
        if line.is_empty() {
            flush(
                &mut out,
                &mut path,
                &mut size,
                &mut packed,
                &mut is_dir,
                &mut modified,
            );
            continue;
        }
        if let Some(v) = line.strip_prefix("Path = ") {
            if !path.is_empty() {
                flush(
                    &mut out,
                    &mut path,
                    &mut size,
                    &mut packed,
                    &mut is_dir,
                    &mut modified,
                );
            }
            path = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("Size = ") {
            size = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("Packed Size = ") {
            packed = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("Folder = ") {
            is_dir = v.trim().eq_ignore_ascii_case("+") || v.trim() == "1";
        } else if let Some(v) = line.strip_prefix("Attributes = ") {
            is_dir |= v.contains('D');
        } else if let Some(v) = line.strip_prefix("Modified = ") {
            modified = parse_7z_time(v.trim());
        }
    }
    flush(
        &mut out,
        &mut path,
        &mut size,
        &mut packed,
        &mut is_dir,
        &mut modified,
    );
    out
}

fn parse_7z_time(s: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .and_then(|n| Utc.from_local_datetime(&n).single())
}

/// Extract `inners` (empty = all) to `dest`.
pub fn extract(path: &Path, dest: &Path, inners: &[&str], password: Option<&str>) -> Result<u64> {
    std::fs::create_dir_all(dest)?;
    let dest_s = dest.to_string_lossy();
    let out_arg = format!("-o{dest_s}");
    let mut args: Vec<String> = vec![
        "x".into(),
        "-y".into(),
        out_arg,
        path.to_string_lossy().into(),
    ];
    if let Some(p) = password {
        args.push(format!("-p{p}"));
    }
    if !inners.is_empty() {
        args.push("--".into());
        for i in inners {
            args.push((*i).into());
        }
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_7z(&refs)?;
    Ok(if inners.is_empty() {
        1
    } else {
        inners.len() as u64
    })
}

/// Read one entry into memory via a temp extract.
pub fn read_entry(path: &Path, inner: &str, password: Option<&str>) -> Result<Vec<u8>> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("orchid-7z-{stamp}"));
    extract(path, &tmp, &[inner], password)?;
    let dest = tmp.join(inner.replace('/', std::path::MAIN_SEPARATOR_STR));
    let bytes = std::fs::read(&dest).map_err(|_| FsError::ArchiveEntryNotFound(inner.into()))?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(bytes)
}

/// Add files to an existing archive (or create it).
pub fn add(
    archive: &Path,
    files: &[PathBuf],
    password: Option<&str>,
    volume_bytes: Option<u64>,
    sfx: bool,
) -> Result<()> {
    let mut args: Vec<String> = vec!["a".into(), "-y".into()];
    if sfx {
        args.push("-sfx".into());
    }
    if let Some(p) = password {
        args.push(format!("-p{p}"));
        args.push("-mhe=on".into());
    }
    if let Some(n) = volume_bytes {
        args.push(format!("-v{n}b"));
    }
    args.push(archive.to_string_lossy().into());
    for f in files {
        args.push(f.to_string_lossy().into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_7z(&refs)?;
    Ok(())
}

/// Delete inner paths from an archive.
pub fn delete(archive: &Path, inners: &[&str]) -> Result<()> {
    let mut args: Vec<String> = vec!["d".into(), "-y".into(), archive.to_string_lossy().into()];
    for i in inners {
        args.push((*i).into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_7z(&refs)?;
    Ok(())
}

/// Test archive integrity.
pub fn test(path: &Path, password: Option<&str>) -> Result<ArchiveTestReport> {
    let mut args: Vec<String> = vec!["t".into(), path.to_string_lossy().into()];
    if let Some(p) = password {
        args.push(format!("-p{p}"));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match run_7z(&refs) {
        Ok(text) => {
            let ok = text.contains("Everything is Ok") || text.contains("Ok");
            Ok(ArchiveTestReport { summary: text, ok })
        }
        Err(e) => Ok(ArchiveTestReport {
            summary: e.to_string(),
            ok: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slt_entries() {
        let sample = "\
Path = dir/file.txt
Size = 12
Packed Size = 8
Folder = -
Modified = 2024-01-02 03:04:05

Path = dir
Size = 0
Attributes = D
";
        let rows = parse_slt(sample);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, "dir/file.txt");
        assert_eq!(rows[0].size, 12);
        assert!(!rows[0].is_dir);
        assert!(rows[1].is_dir);
    }
}
