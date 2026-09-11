//! In-app backup of Orchid data (config, state, vault, chunks).
//!
//! The zip is written to a caller-chosen path. Search index and logs are
//! omitted (rebuildable / noisy). The Hello/DPAPI sidecar is included and
//! remains bound to the exporting Windows user and machine.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::error::{Result, StorageError};
use crate::paths::OrchidPaths;

/// Default file name for a backup zip (`orchid-backup-YYYYMMDD.zip`).
#[must_use]
pub fn default_backup_filename() -> String {
    let day = chrono::Local::now().format("%Y%m%d");
    format!("orchid-backup-{day}.zip")
}

/// Write a backup zip of the live Orchid profile to `dest_zip`.
///
/// Existing `dest_zip` is overwritten. Parent directories are created.
///
/// # Errors
///
/// Returns [`StorageError::Io`] or [`StorageError::Archive`] when the zip
/// cannot be written.
pub fn write_backup_zip(paths: &OrchidPaths, dest_zip: &Path) -> Result<PathBuf> {
    if let Some(parent) = dest_zip.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(dest_zip)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let manifest = format!(
        "Orchid backup\ncreated: {}\nversion: {}\n\nIncludes: config, state.redb, vault, chunks, network bookmarks.\nOmits: search_index, logs, cache.\nDPAPI sidecar (passwords.master.dpapi) is machine/user bound.\n",
        chrono::Utc::now().to_rfc3339(),
        env!("CARGO_PKG_VERSION"),
    );
    zip.start_file("MANIFEST.txt", opts)
        .map_err(|e| StorageError::Archive(e.to_string()))?;
    zip.write_all(manifest.as_bytes())?;

    add_path_tree(&mut zip, opts, &paths.config_dir, "config")?;
    add_optional_file(&mut zip, opts, &paths.state_db_path, "data/state.redb")?;
    add_optional_file(
        &mut zip,
        opts,
        &paths.passwords_db_path,
        "data/passwords.kdbx",
    )?;
    let dpapi = paths.data_dir.join("passwords.master.dpapi");
    add_optional_file(&mut zip, opts, &dpapi, "data/passwords.master.dpapi")?;
    add_optional_file(
        &mut zip,
        opts,
        &paths.network_bookmarks_file,
        "data/network-bookmarks.toml",
    )?;
    add_path_tree(&mut zip, opts, &paths.chunks_dir, "data/chunks")?;

    zip.finish()
        .map_err(|e| StorageError::Archive(e.to_string()))?;
    Ok(dest_zip.to_path_buf())
}

fn add_optional_file(
    zip: &mut ZipWriter<File>,
    opts: SimpleFileOptions,
    src: &Path,
    name: &str,
) -> Result<()> {
    if !src.is_file() {
        return Ok(());
    }
    add_file(zip, opts, src, name)
}

fn add_file(
    zip: &mut ZipWriter<File>,
    opts: SimpleFileOptions,
    src: &Path,
    name: &str,
) -> Result<()> {
    zip.start_file(name, opts)
        .map_err(|e| StorageError::Archive(e.to_string()))?;
    let mut f = File::open(src)?;
    io::copy(&mut f, zip)?;
    Ok(())
}

fn add_path_tree(
    zip: &mut ZipWriter<File>,
    opts: SimpleFileOptions,
    root: &Path,
    zip_prefix: &str,
) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    if root.is_file() {
        return add_file(zip, opts, root, zip_prefix);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let read = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        for entry in read {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let name = format!(
                "{}/{}",
                zip_prefix,
                rel.to_string_lossy().replace('\\', "/")
            );
            add_file(zip, opts, &path, &name)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_zip_includes_manifest_and_config() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = OrchidPaths::for_testing(tmp.path());
        paths.ensure_directories().unwrap();
        fs::write(paths.config_file.as_path(), "theme = \"orchid-dark\"\n").unwrap();
        fs::write(paths.state_db_path.as_path(), b"redb-stub").unwrap();

        let dest = tmp.path().join("out").join("backup.zip");
        write_backup_zip(&paths, &dest).unwrap();
        assert!(dest.is_file());

        let file = File::open(&dest).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "MANIFEST.txt"));
        assert!(names.iter().any(|n| n == "config/config.toml"));
        assert!(names.iter().any(|n| n == "data/state.redb"));
        assert!(!names.iter().any(|n| n.contains("search_index")));
    }
}
