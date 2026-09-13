//! Support bundle: logs + sanitized config + environment notes. No vault.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::config::schema::OrchidConfig;
use crate::error::{Result, StorageError};
use crate::paths::OrchidPaths;

/// Default file name for a support zip (`orchid-support-YYYYMMDD.zip`).
#[must_use]
pub fn default_support_filename() -> String {
    let day = chrono::Local::now().format("%Y%m%d");
    format!("orchid-support-{day}.zip")
}

/// Extra facts the UI knows (exe folder, companion DLLs).
#[derive(Debug, Clone, Default)]
pub struct SupportBundleExtras {
    /// Directory that contains `orchid.exe` (and copied native DLLs).
    pub exe_dir: Option<PathBuf>,
    /// App / workspace version string.
    pub app_version: String,
}

/// Write a support bundle to `dest_zip`.
///
/// Includes sanitized `config.toml` (network mount passwords redacted),
/// recent log files (capped), and an environment note. Never copies the
/// vault, chunk store, or `state.redb`.
///
/// # Errors
///
/// Returns [`StorageError::Io`] or [`StorageError::Archive`].
pub fn write_support_bundle(
    paths: &OrchidPaths,
    dest_zip: &Path,
    extras: &SupportBundleExtras,
) -> Result<PathBuf> {
    if let Some(parent) = dest_zip.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(dest_zip)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let notes = environment_notes(paths, extras);
    zip.start_file("environment.txt", opts)
        .map_err(|e| StorageError::Archive(e.to_string()))?;
    zip.write_all(notes.as_bytes())?;

    if let Some(sanitized) = sanitized_config_toml(&paths.config_file) {
        zip.start_file("config.toml", opts)
            .map_err(|e| StorageError::Archive(e.to_string()))?;
        zip.write_all(sanitized.as_bytes())?;
    } else if paths.config_file.is_file() {
        zip.start_file("config.toml.unparsed", opts)
            .map_err(|e| StorageError::Archive(e.to_string()))?;
        let mut f = File::open(&paths.config_file)?;
        io::copy(&mut f, &mut zip)?;
    }

    add_recent_logs(&mut zip, opts, &paths.logs_dir)?;

    zip.finish()
        .map_err(|e| StorageError::Archive(e.to_string()))?;
    Ok(dest_zip.to_path_buf())
}

fn environment_notes(paths: &OrchidPaths, extras: &SupportBundleExtras) -> String {
    let mut lines = vec![
        format!("orchid-version: {}", extras.app_version),
        format!("storage-crate: {}", env!("CARGO_PKG_VERSION")),
        format!("os: {}", std::env::consts::OS),
        format!("arch: {}", std::env::consts::ARCH),
        format!("config-dir: {}", paths.config_dir.display()),
        format!("data-dir: {}", paths.data_dir.display()),
        format!("logs-dir: {}", paths.logs_dir.display()),
        format!(
            "state.redb: {}",
            if paths.state_db_path.is_file() {
                "present"
            } else {
                "absent"
            }
        ),
        format!(
            "vault: {}",
            if paths.passwords_db_path.is_file() {
                "present (not included)"
            } else {
                "absent"
            }
        ),
    ];
    if let Some(exe) = &extras.exe_dir {
        lines.push(format!("exe-dir: {}", exe.display()));
        for name in ["pdfium.dll", "mpv-1.dll", "libmpv-2.dll"] {
            let p = exe.join(name);
            lines.push(format!(
                "{name}: {}",
                if p.is_file() { "present" } else { "absent" }
            ));
        }
    }
    for (label, var) in [("rclone", "RCLONE_BIN"), ("log-filter", "RUST_LOG")] {
        match std::env::var(var) {
            Ok(v) => lines.push(format!("{label}: {v}")),
            Err(_) => lines.push(format!("{label}: (unset)")),
        }
    }
    lines.push(String::new());
    lines.push("This bundle has no vault, chunks, or state.redb.".into());
    lines.join("\n")
}

fn sanitized_config_toml(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let mut cfg: OrchidConfig = toml::from_str(&raw).ok()?;
    for mount in &mut cfg.file_manager.network_mounts {
        if mount.password.as_ref().is_some_and(|p| !p.is_empty()) {
            mount.password = Some("***".into());
        }
    }
    toml::to_string_pretty(&cfg).ok()
}

fn add_recent_logs(
    zip: &mut ZipWriter<File>,
    opts: SimpleFileOptions,
    logs_dir: &Path,
) -> Result<()> {
    if !logs_dir.is_dir() {
        return Ok(());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(logs_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort_by_key(|p| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    files.reverse();
    const MAX_FILES: usize = 8;
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    for path in files.into_iter().take(MAX_FILES) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let zip_name = format!("logs/{name}");
        zip.start_file(&zip_name, opts)
            .map_err(|e| StorageError::Archive(e.to_string()))?;
        let mut f = File::open(&path)?;
        let mut limited = std::io::Read::by_ref(&mut f).take(MAX_BYTES);
        io::copy(&mut limited, zip)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::NetworkMountConfig;

    #[test]
    fn support_bundle_redacts_mount_passwords() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = OrchidPaths::for_testing(tmp.path());
        paths.ensure_directories().unwrap();
        let mut cfg = OrchidConfig::default();
        cfg.file_manager.network_mounts.push(NetworkMountConfig {
            name: "lab".into(),
            uri: "sftp://host/path".into(),
            password: Some("hunter2".into()),
            ..Default::default()
        });
        fs::write(&paths.config_file, toml::to_string_pretty(&cfg).unwrap()).unwrap();
        fs::write(paths.logs_dir.join("orchid.log"), "hello log\n").unwrap();

        let dest = tmp.path().join("support.zip");
        write_support_bundle(
            &paths,
            &dest,
            &SupportBundleExtras {
                exe_dir: None,
                app_version: "0.1.0-test".into(),
            },
        )
        .unwrap();

        let file = File::open(&dest).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "environment.txt"));
        assert!(names.iter().any(|n| n == "config.toml"));
        assert!(names.iter().any(|n| n == "logs/orchid.log"));
        assert!(!names.iter().any(|n| n.contains("passwords.kdbx")));

        let mut cfg_file = archive.by_name("config.toml").unwrap();
        let mut body = String::new();
        io::Read::read_to_string(&mut cfg_file, &mut body).unwrap();
        assert!(!body.contains("hunter2"));
        assert!(body.contains("***"));
    }
}
