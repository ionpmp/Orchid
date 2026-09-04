//! Copy operation with streaming, progress, cancellation, and optional
//! content-hash verification.

use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use filetime::FileTime;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::error::{FsError, Result};
use crate::operations::progress::{OperationProgress, ProgressSink, TransferControl};
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// Tunable knobs for [`copy`].
#[derive(Debug, Clone, Copy)]
pub struct CopyOptions {
    /// Overwrite an existing destination.
    pub overwrite: bool,
    /// If the destination exists, skip it instead of failing.
    pub skip_existing: bool,
    /// Overwrite only when the source is newer than the destination.
    pub overwrite_older: bool,
    /// Create directories but do not copy file contents.
    pub structure_only: bool,
    /// Resume a partial destination (append remaining bytes).
    pub resume: bool,
    /// Compute BLAKE3 on source + destination; fail if they differ.
    pub verify_content_hash: bool,
    /// Preserve `modified` (and where supported, `accessed`) timestamps.
    pub preserve_timestamps: bool,
    /// Preserve readonly / hidden / system attributes.
    pub preserve_attributes: bool,
    /// Copy NTFS alternate data streams (Windows local only; best-effort).
    pub copy_ads: bool,
    /// Follow symlinks rather than copying the link itself.
    pub follow_symlinks: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            overwrite: false,
            skip_existing: false,
            overwrite_older: false,
            structure_only: false,
            resume: false,
            verify_content_hash: false,
            preserve_timestamps: true,
            preserve_attributes: true,
            copy_ads: true,
            follow_symlinks: true,
        }
    }
}

/// Copy `from` → `to`, recursing through directories.
///
/// Progress is emitted on `progress` (if provided) after every file. The
/// `cancel` token is checked at every file boundary.
///
/// # Errors
///
/// Propagates any underlying provider / I/O error;
/// [`FsError::AlreadyExists`] if the target exists and `overwrite` is
/// false; [`FsError::Cancelled`] if the token fires.
pub async fn copy(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    options: CopyOptions,
    progress: Option<&ProgressSink>,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Result<()> {
    copy_with_control(
        registry,
        from,
        to,
        options,
        progress,
        TransferControl {
            cancel,
            pause: None,
        },
    )
    .await
}

/// [`copy`] with pause support.
pub async fn copy_with_control(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    options: CopyOptions,
    progress: Option<&ProgressSink>,
    ctl: TransferControl,
) -> Result<()> {
    let src_provider = registry
        .for_path(from)
        .ok_or_else(|| FsError::ProviderNotMounted(from.to_string()))?;
    let _dst_provider = registry
        .for_path(to)
        .ok_or_else(|| FsError::ProviderNotMounted(to.to_string()))?;

    if !from.is_local() || !to.is_local() {
        if let Some(provider) = registry.for_path(from) {
            if provider
                .copy_cross_scheme(registry, from, to, options, progress)
                .await?
            {
                return Ok(());
            }
        }
        if let Some(provider) = registry.for_path(to) {
            if provider
                .copy_cross_scheme(registry, from, to, options, progress)
                .await?
            {
                return Ok(());
            }
        }
    }

    let src_meta = src_provider.metadata(from).await?;
    if matches!(src_meta.kind, crate::entry::FsEntryKind::Directory) {
        copy_directory(registry, from, to, options, progress, ctl).await
    } else if options.structure_only {
        Ok(())
    } else {
        copy_file_with_progress(registry, from, to, options, progress, ctl, 0, 0).await?;
        Ok(())
    }
}

async fn copy_directory(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    options: CopyOptions,
    progress: Option<&ProgressSink>,
    ctl: TransferControl,
) -> Result<()> {
    // MVP: directory copy only supported between local paths, since that's
    // the only provider that exists today. Walk with `walkdir`, stream each
    // file through the generic provider path.
    if !from.is_local() || !to.is_local() {
        return Err(FsError::InvalidPath {
            reason: "cross-provider directory copy is not supported in MVP".into(),
        });
    }
    let src_os = from.to_local()?;
    let dst_os = to.to_local()?;

    // First pass: enumerate work + total bytes so progress is meaningful.
    let mut files: Vec<(PathBuf, PathBuf, u64)> = Vec::new();
    let mut total_bytes: u64 = 0;
    for entry in walkdir::WalkDir::new(&src_os).follow_links(options.follow_symlinks) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(&src_os).unwrap_or(entry.path());
        let dst = dst_os.join(rel);
        if entry.file_type().is_dir() {
            tokio::fs::create_dir_all(&dst).await?;
            continue;
        }
        if options.structure_only {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        total_bytes += size;
        files.push((entry.path().to_path_buf(), dst, size));
    }

    let items_total = files.len() as u64;
    let mut bytes_done: u64 = 0;

    for (items_done, (src, dst, _size)) in (0_u64..).zip(files) {
        ctl.wait().await?;
        let src_fs = FsPath::from_local(&src)?;
        let dst_fs = FsPath::from_local(&dst)?;
        let bytes_copied = copy_file_with_progress(
            registry,
            &src_fs,
            &dst_fs,
            options,
            progress,
            ctl.clone(),
            bytes_done,
            total_bytes,
        )
        .await?;
        bytes_done += bytes_copied;
        if let Some(p) = progress {
            p.send(OperationProgress {
                total_bytes,
                processed_bytes: bytes_done,
                current_path: dst_fs,
                items_processed: items_done + 1,
                items_total,
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn copy_file_with_progress(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    options: CopyOptions,
    progress: Option<&ProgressSink>,
    ctl: TransferControl,
    bytes_before: u64,
    bytes_total: u64,
) -> Result<u64> {
    ctl.wait().await?;
    let src_provider = registry
        .for_path(from)
        .ok_or_else(|| FsError::ProviderNotMounted(from.to_string()))?;
    let dst_provider = registry
        .for_path(to)
        .ok_or_else(|| FsError::ProviderNotMounted(to.to_string()))?;

    let src_meta = src_provider.metadata(from).await?;
    let dest_exists = dst_provider.exists(to).await?;
    let mut resume_from = 0u64;
    if dest_exists {
        let dst_meta = dst_provider.metadata(to).await.ok();
        if options.skip_existing {
            return Ok(0);
        }
        if options.overwrite_older {
            let src_m = src_meta.modified;
            let dst_m = dst_meta.as_ref().and_then(|m| m.modified);
            if let (Some(s), Some(d)) = (src_m, dst_m) {
                if s <= d {
                    return Ok(0);
                }
            }
        } else if options.resume {
            let dest_size = dst_meta.as_ref().map(|m| m.size).unwrap_or(0);
            if dest_size > 0 && dest_size < src_meta.size && to.is_local() {
                resume_from = dest_size;
            } else if !options.overwrite {
                return Err(FsError::AlreadyExists(to.to_string()));
            }
        } else if !options.overwrite {
            return Err(FsError::AlreadyExists(to.to_string()));
        }
    }

    if options.verify_content_hash && resume_from > 0 {
        // Hash verification needs a full rewrite.
        resume_from = 0;
    }

    let total = if bytes_total == 0 {
        src_meta.size
    } else {
        bytes_total
    };

    if resume_from == 0 && !options.verify_content_hash && from.is_local() && to.is_local() {
        if let (Ok(src), Ok(dst)) = (from.to_local(), to.to_local()) {
            match copy_local_kernel(&src, &dst).await {
                Ok(()) => {
                    if options.preserve_timestamps {
                        if let Some(modified) = src_meta.modified {
                            let os = dst.clone();
                            let ft = FileTime::from_unix_time(
                                modified.timestamp(),
                                modified.timestamp_subsec_nanos(),
                            );
                            let _ = tokio::task::spawn_blocking(move || {
                                filetime::set_file_mtime(&os, ft)
                            })
                            .await;
                        }
                    }
                    if options.preserve_attributes {
                        apply_attributes(&dst, src_meta.readonly, src_meta.hidden, src_meta.system);
                    }
                    if options.copy_ads {
                        copy_ads_best_effort(&src, &dst);
                    }
                    if let Some(p) = progress {
                        p.send(OperationProgress {
                            total_bytes: total,
                            processed_bytes: bytes_before + src_meta.size,
                            current_path: to.clone(),
                            items_processed: 0,
                            items_total: 0,
                        });
                    }
                    return Ok(src_meta.size);
                }
                Err(e) => {
                    tracing::debug!(error = %e, "kernel copy fallback to stream");
                }
            }
        }
    }

    let mut reader = src_provider.read_stream(from).await?;
    if resume_from > 0 {
        let mut skipped = 0u64;
        let mut skip_buf = vec![0u8; 128 * 1024];
        while skipped < resume_from {
            let want = ((resume_from - skipped) as usize).min(skip_buf.len());
            let n = reader.read(&mut skip_buf[..want]).await?;
            if n == 0 {
                break;
            }
            skipped += n as u64;
        }
    }

    let mut writer: Box<dyn tokio::io::AsyncWrite + Unpin + Send> =
        if resume_from > 0 && to.is_local() {
            let os = to.to_local()?;
            let mut file = tokio::fs::OpenOptions::new().write(true).open(&os).await?;
            file.seek(SeekFrom::Start(resume_from)).await?;
            Box::new(file)
        } else {
            dst_provider.write_stream(to).await?
        };

    let mut hasher = if options.verify_content_hash {
        Some(orchid_crypto::StreamHasher::new())
    } else {
        None
    };

    let mut buf = vec![0u8; 128 * 1024];
    let mut written: u64 = resume_from;
    let mut last_progress = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    loop {
        ctl.wait().await?;
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        if let Some(h) = &mut hasher {
            h.update(&buf[..n]);
        }
        writer.write_all(&buf[..n]).await?;
        written += n as u64;
        if let Some(p) = progress {
            let now = Instant::now();
            if now.duration_since(last_progress) >= Duration::from_millis(150) {
                last_progress = now;
                p.send(OperationProgress {
                    total_bytes: total,
                    processed_bytes: bytes_before + written,
                    current_path: to.clone(),
                    items_processed: 0,
                    items_total: 0,
                });
            }
        }
    }
    if let Some(p) = progress {
        p.send(OperationProgress {
            total_bytes: total,
            processed_bytes: bytes_before + written,
            current_path: to.clone(),
            items_processed: 0,
            items_total: 0,
        });
    }
    writer.flush().await?;
    drop(writer);

    if let Some(h) = hasher {
        let src_hash = h.finalize();
        // Recompute hash of destination independently.
        let mut dst_reader = dst_provider.read_stream(to).await?;
        let mut dst_hasher = orchid_crypto::StreamHasher::new();
        loop {
            let n = dst_reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            dst_hasher.update(&buf[..n]);
        }
        let dst_hash = dst_hasher.finalize();
        if src_hash != dst_hash {
            // Remove the corrupted destination before surfacing the error.
            let _ = dst_provider.remove(to, false).await;
            return Err(FsError::EncryptedOp(
                "content-hash verification failed after copy".into(),
            ));
        }
    }

    if options.preserve_timestamps {
        if let (Some(modified), true) = (src_meta.modified, to.is_local()) {
            let os = to.to_local()?;
            let ft =
                FileTime::from_unix_time(modified.timestamp(), modified.timestamp_subsec_nanos());
            let _ = tokio::task::spawn_blocking(move || filetime::set_file_mtime(&os, ft)).await;
        }
    }

    if options.preserve_attributes && to.is_local() {
        let os = to.to_local()?;
        apply_attributes(&os, src_meta.readonly, src_meta.hidden, src_meta.system);
    }

    if options.copy_ads && from.is_local() && to.is_local() {
        let src_os = from.to_local()?;
        let dst_os = to.to_local()?;
        copy_ads_best_effort(&src_os, &dst_os);
    }

    Ok(written)
}

async fn copy_local_kernel(from: &Path, to: &Path) -> Result<()> {
    let src = from.to_path_buf();
    let dst = to.to_path_buf();
    tokio::task::spawn_blocking(move || copy_local_kernel_sync(&src, &dst))
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?
}

fn copy_local_kernel_sync(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::CopyFileExW;
        let src: Vec<u16> = from.as_os_str().encode_wide().chain([0]).collect();
        let dst: Vec<u16> = to.as_os_str().encode_wide().chain([0]).collect();
        unsafe {
            CopyFileExW(
                PCWSTR(src.as_ptr()),
                PCWSTR(dst.as_ptr()),
                None,
                None,
                None,
                windows::Win32::Storage::FileSystem::COPYFILE_FLAGS(0),
            )
        }
        .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::copy(from, to)?;
        Ok(())
    }
}

fn apply_attributes(path: &Path, readonly: bool, hidden: bool, system: bool) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL,
            FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM,
        };
        let mut attrs = FILE_ATTRIBUTE_NORMAL;
        if readonly {
            attrs |= FILE_ATTRIBUTE_READONLY;
        }
        if hidden {
            attrs |= FILE_ATTRIBUTE_HIDDEN;
        }
        if system {
            attrs |= FILE_ATTRIBUTE_SYSTEM;
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let _ = unsafe { SetFileAttributesW(PCWSTR(wide.as_ptr()), attrs) };
    }
    #[cfg(not(windows))]
    {
        let _ = hidden;
        let _ = system;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_readonly(readonly);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
}

fn copy_ads_best_effort(from: &Path, to: &Path) {
    #[cfg(windows)]
    {
        if let Err(e) = copy_ntfs_ads(from, to) {
            tracing::debug!(error = %e, "ADS copy skipped");
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (from, to);
    }
}

#[cfg(windows)]
fn copy_ntfs_ads(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows::Win32::Storage::FileSystem::{
        FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard,
        WIN32_FIND_STREAM_DATA,
    };

    let mut path: Vec<u16> = from.as_os_str().encode_wide().chain([0]).collect();
    let mut data = WIN32_FIND_STREAM_DATA::default();
    let handle = unsafe {
        FindFirstStreamW(
            PWSTR(path.as_mut_ptr()),
            FindStreamInfoStandard,
            &mut data as *mut _ as *mut _,
            Some(0),
        )
    };
    let handle = match handle {
        Ok(h) if h != INVALID_HANDLE_VALUE => h,
        _ => return Ok(()),
    };
    loop {
        let name = {
            let raw = &data.cStreamName;
            let len = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
            String::from_utf16_lossy(&raw[..len])
        };
        // `::$DATA` is the default unnamed stream (already copied).
        if !name.is_empty() && name != "::$DATA" && name != ":$DATA" {
            let stream = name.trim_end_matches(":$DATA");
            let src = ads_os_path(from, stream);
            let dst = ads_os_path(to, stream);
            if let Ok(bytes) = std::fs::read(&src) {
                let _ = std::fs::write(&dst, bytes);
            }
        }
        let next = unsafe { FindNextStreamW(handle, &mut data as *mut _ as *mut _) };
        if next.is_err() {
            break;
        }
    }
    unsafe {
        let _ = FindClose(handle);
    }
    Ok(())
}

#[cfg(windows)]
fn ads_os_path(path: &Path, stream: &str) -> PathBuf {
    let stream = stream.trim_start_matches(':');
    PathBuf::from(format!("{}:{stream}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{FsProviderRegistry, LocalProvider};
    use std::sync::Arc;

    fn registry() -> FsProviderRegistry {
        let r = FsProviderRegistry::new();
        r.register(Arc::new(LocalProvider::new()) as Arc<dyn crate::provider::FsProvider>)
            .unwrap();
        r
    }

    #[tokio::test]
    async fn skip_existing_leaves_dest() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("a.txt");
        let dst = td.path().join("b.txt");
        std::fs::write(&src, b"new").unwrap();
        std::fs::write(&dst, b"old").unwrap();
        let opts = CopyOptions {
            skip_existing: true,
            ..CopyOptions::default()
        };
        copy(
            &registry(),
            &FsPath::from_local(&src).unwrap(),
            &FsPath::from_local(&dst).unwrap(),
            opts,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"old");
    }

    #[tokio::test]
    async fn structure_only_skips_files() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("tree");
        let dst = td.path().join("out");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("sub/a.txt"), b"x").unwrap();
        let opts = CopyOptions {
            structure_only: true,
            ..CopyOptions::default()
        };
        copy(
            &registry(),
            &FsPath::from_local(&src).unwrap(),
            &FsPath::from_local(&dst).unwrap(),
            opts,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(dst.join("sub").is_dir());
        assert!(!dst.join("sub/a.txt").exists());
    }

    #[tokio::test]
    async fn local_file_copy_rewrites_dest() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("src.txt");
        let dst = td.path().join("dst.txt");
        std::fs::write(&src, b"payload").unwrap();
        std::fs::write(&dst, b"old").unwrap();
        copy(
            &registry(),
            &FsPath::from_local(&src).unwrap(),
            &FsPath::from_local(&dst).unwrap(),
            CopyOptions {
                overwrite: true,
                ..CopyOptions::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"payload");
    }

    #[tokio::test]
    async fn resume_appends_remaining_bytes() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("src.bin");
        let dst = td.path().join("dst.bin");
        std::fs::write(&src, b"hello world").unwrap();
        std::fs::write(&dst, b"hello").unwrap();
        let opts = CopyOptions {
            resume: true,
            ..CopyOptions::default()
        };
        copy(
            &registry(),
            &FsPath::from_local(&src).unwrap(),
            &FsPath::from_local(&dst).unwrap(),
            opts,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello world");
    }
}
