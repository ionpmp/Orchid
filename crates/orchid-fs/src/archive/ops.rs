//! Create, mutate, extract, and test archives.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::archive::cli;
use crate::archive::reader::open_archive;
use crate::archive::types::{
    looks_like_archive_path, ArchiveFormat, ArchiveTestReport, CreateArchiveOptions,
};
use crate::error::{FsError, Result};
use crate::path::FsPath;

/// Extract the whole archive (or `inners`) next to it / into `dest`.
pub async fn extract_archive(
    archive: &FsPath,
    dest: &FsPath,
    inners: &[String],
    password: Option<&str>,
) -> Result<u64> {
    let src = archive.to_local()?;
    let out = dest.to_local()?;
    let inners = inners.to_vec();
    let password = password.map(str::to_string);
    tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = inners.iter().map(String::as_str).collect();
        let fmt =
            ArchiveFormat::from_file_name(src.file_name().and_then(|n| n.to_str()).unwrap_or(""));
        let use_native = password.is_none() && fmt.is_some_and(|f| f.native_readable());
        if use_native {
            if refs.is_empty() {
                return extract_native_all(&src, &out);
            }
            return extract_native_selected(&src, &out, &refs);
        }
        cli::extract(&src, &out, &refs, password.as_deref())
    })
    .await
    .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
}

fn extract_native_all(src: &Path, dest: &Path) -> Result<u64> {
    let reader = open_archive(src)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    rt.block_on(reader.extract_all(dest))
}

fn extract_native_selected(src: &Path, dest: &Path, inners: &[&str]) -> Result<u64> {
    let reader = open_archive(src)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    rt.block_on(async {
        let mut n = 0_u64;
        for inner in inners {
            let out = dest.join(inner.replace('/', std::path::MAIN_SEPARATOR_STR));
            reader.extract_entry(inner, &out).await?;
            n += 1;
        }
        Ok(n)
    })
}

/// Create `dest` from `sources` (files or folders).
pub async fn create_archive(
    dest: &FsPath,
    sources: &[FsPath],
    opts: CreateArchiveOptions,
) -> Result<()> {
    let dest_os = dest.to_local()?;
    let files: Vec<PathBuf> = sources.iter().filter_map(|s| s.to_local().ok()).collect();
    let fmt = ArchiveFormat::from_file_name(
        dest_os
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("archive.zip"),
    )
    .unwrap_or(ArchiveFormat::Zip);
    let needs_cli = opts.password.is_some()
        || opts.volume_bytes.is_some()
        || opts.sfx
        || !fmt.native_writable();
    tokio::task::spawn_blocking(move || {
        if needs_cli {
            return cli::add(
                &dest_os,
                &files,
                opts.password.as_deref(),
                opts.volume_bytes,
                opts.sfx,
            );
        }
        match fmt {
            ArchiveFormat::Zip => create_zip(&dest_os, &files),
            ArchiveFormat::Tar => create_tar(&dest_os, &files, TarEnc::Plain),
            ArchiveFormat::TarGz => create_tar(&dest_os, &files, TarEnc::Gz),
            ArchiveFormat::TarXz => create_tar(&dest_os, &files, TarEnc::Xz),
            ArchiveFormat::TarBz2 => create_tar(&dest_os, &files, TarEnc::Bz2),
            _ => cli::add(
                &dest_os,
                &files,
                opts.password.as_deref(),
                opts.volume_bytes,
                opts.sfx,
            ),
        }
    })
    .await
    .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
}

/// Add local files to an existing archive.
pub async fn add_to_archive(archive: &FsPath, sources: &[FsPath]) -> Result<()> {
    let dest_os = archive.to_local()?;
    let files: Vec<PathBuf> = sources.iter().filter_map(|s| s.to_local().ok()).collect();
    let fmt =
        ArchiveFormat::from_file_name(dest_os.file_name().and_then(|n| n.to_str()).unwrap_or(""))
            .unwrap_or(ArchiveFormat::Zip);
    tokio::task::spawn_blocking(move || {
        if fmt == ArchiveFormat::Zip {
            add_to_zip(&dest_os, &files)
        } else {
            cli::add(&dest_os, &files, None, None, false)
        }
    })
    .await
    .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
}

/// Remove inner paths from an archive.
pub async fn delete_from_archive(archive: &FsPath, inners: &[String]) -> Result<()> {
    let dest_os = archive.to_local()?;
    let inners = inners.to_vec();
    let fmt =
        ArchiveFormat::from_file_name(dest_os.file_name().and_then(|n| n.to_str()).unwrap_or(""))
            .unwrap_or(ArchiveFormat::Zip);
    tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = inners.iter().map(String::as_str).collect();
        if fmt == ArchiveFormat::Zip {
            delete_from_zip(&dest_os, &refs)
        } else {
            cli::delete(&dest_os, &refs)
        }
    })
    .await
    .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
}

/// Test archive integrity (CRC / 7z t).
pub async fn test_archive(archive: &FsPath, password: Option<&str>) -> Result<ArchiveTestReport> {
    let src = archive.to_local()?;
    let password = password.map(str::to_string);
    tokio::task::spawn_blocking(move || {
        if cli::find_7z().is_some() {
            return cli::test(&src, password.as_deref());
        }
        test_zip_or_list(&src)
    })
    .await
    .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
}

fn test_zip_or_list(src: &Path) -> Result<ArchiveTestReport> {
    if src
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        return test_zip(src);
    }
    let reader = open_archive(src)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    let entries = rt.block_on(reader.list())?;
    Ok(ArchiveTestReport {
        summary: format!("Listed {} entries", entries.len()),
        ok: true,
    })
}

fn create_zip(dest: &Path, files: &[PathBuf]) -> Result<()> {
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for src in files {
        append_path_to_zip(
            &mut zip,
            src,
            src.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
            opts,
        )?;
    }
    zip.finish()?;
    Ok(())
}

fn append_path_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    src: &Path,
    name: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<()> {
    if src.is_dir() {
        let prefix = name.trim_end_matches('/').to_string();
        zip.add_directory(format!("{prefix}/"), opts)?;
        for ent in walkdir::WalkDir::new(src).into_iter().flatten() {
            let p = ent.path();
            if p == src {
                continue;
            }
            let rel = p.strip_prefix(src).unwrap_or(p);
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            let inner = format!("{prefix}/{rel_s}");
            if p.is_dir() {
                zip.add_directory(format!("{inner}/"), opts)?;
            } else {
                zip.start_file(&inner, opts)?;
                let mut f = std::fs::File::open(p)?;
                std::io::copy(&mut f, zip)?;
            }
        }
    } else {
        zip.start_file(name, opts)?;
        let mut f = std::fs::File::open(src)?;
        std::io::copy(&mut f, zip)?;
    }
    Ok(())
}

fn add_to_zip(archive: &Path, files: &[PathBuf]) -> Result<()> {
    let replace: std::collections::HashSet<String> = files
        .iter()
        .filter_map(|f| f.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();
    let tmp = archive.with_extension("zip.partial");
    {
        let existing = std::fs::File::open(archive)?;
        let mut src = zip::ZipArchive::new(existing)?;
        let out = std::fs::File::create(&tmp)?;
        let mut dest = zip::ZipWriter::new(out);
        for i in 0..src.len() {
            let entry = src.by_index(i)?;
            let name = entry.name().trim_end_matches('/').to_string();
            if replace
                .iter()
                .any(|r| name == *r || name.starts_with(&format!("{r}/")))
            {
                continue;
            }
            dest.raw_copy_file(entry)?;
        }
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for f in files {
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            append_path_to_zip(&mut dest, f, name, opts)?;
        }
        dest.finish()?;
    }
    std::fs::rename(&tmp, archive)?;
    Ok(())
}

fn delete_from_zip(archive: &Path, inners: &[&str]) -> Result<()> {
    let skip: std::collections::HashSet<&str> = inners.iter().copied().collect();
    let tmp = archive.with_extension("zip.partial");
    {
        let existing = std::fs::File::open(archive)?;
        let mut src = zip::ZipArchive::new(existing)?;
        let out = std::fs::File::create(&tmp)?;
        let mut dest = zip::ZipWriter::new(out);
        for i in 0..src.len() {
            let entry = src.by_index(i)?;
            let name = entry.name().to_string();
            let trimmed = name.trim_end_matches('/');
            if skip.contains(trimmed)
                || skip
                    .iter()
                    .any(|s| trimmed == *s || trimmed.starts_with(&format!("{s}/")))
            {
                continue;
            }
            dest.raw_copy_file(entry)?;
        }
        dest.finish()?;
    }
    std::fs::rename(&tmp, archive)?;
    Ok(())
}

fn test_zip(src: &Path) -> Result<ArchiveTestReport> {
    let file = std::fs::File::open(src)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut ok = 0_u32;
    let mut bad = 0_u32;
    let mut lines = String::new();
    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(e) => {
                bad += 1;
                lines.push_str(&format!("index {i}: {e}\n"));
                continue;
            }
        };
        if entry.is_dir() {
            continue;
        }
        let mut sink = std::io::sink();
        match std::io::copy(&mut entry, &mut sink) {
            Ok(_) => ok += 1,
            Err(e) => {
                bad += 1;
                lines.push_str(&format!("{}: {e}\n", entry.name()));
            }
        }
    }
    Ok(ArchiveTestReport {
        summary: format!("{ok} ok, {bad} failed\n{lines}"),
        ok: bad == 0,
    })
}

enum TarEnc {
    Plain,
    Gz,
    Xz,
    Bz2,
}

fn create_tar(dest: &Path, files: &[PathBuf], enc: TarEnc) -> Result<()> {
    let file = std::fs::File::create(dest)?;
    match enc {
        TarEnc::Plain => write_tar(file, files),
        TarEnc::Gz => {
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            write_tar(enc, files)
        }
        TarEnc::Xz => {
            let enc = xz2::write::XzEncoder::new(file, 6);
            write_tar(enc, files)
        }
        TarEnc::Bz2 => {
            let enc = bzip2::write::BzEncoder::new(file, bzip2::Compression::default());
            write_tar(enc, files)
        }
    }
}

fn write_tar<W: Write>(w: W, files: &[PathBuf]) -> Result<()> {
    let mut builder = tar::Builder::new(w);
    for src in files {
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        if src.is_dir() {
            builder.append_dir_all(name, src)?;
        } else {
            builder.append_path_with_name(src, name)?;
        }
    }
    builder.finish()?;
    Ok(())
}

/// Default extract folder next to `archive` (`name` without last extension).
pub fn default_extract_dir(archive: &FsPath) -> Result<FsPath> {
    let os = archive.to_local()?;
    let stem = os
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("extracted");
    let parent = os.parent().unwrap_or(Path::new("."));
    let dest = parent.join(stem);
    FsPath::from_local(&dest)
}

/// Whether `path` is an archive file on disk (by name).
#[must_use]
pub fn is_archive_file(path: &FsPath) -> bool {
    path.to_local()
        .ok()
        .is_some_and(|p| p.is_file() && looks_like_archive_path(&p))
}

/// Add `src` to `archive` under the inner path `inner` (overwrite if present).
pub(crate) fn add_named_file_sync(archive: &FsPath, src: &Path, inner: &str) -> Result<()> {
    let dest_os = archive.to_local()?;
    let inner = inner.trim_matches('/');
    if inner.is_empty() {
        return Err(FsError::InvalidPath {
            reason: "empty archive entry name".into(),
        });
    }
    let fmt =
        ArchiveFormat::from_file_name(dest_os.file_name().and_then(|n| n.to_str()).unwrap_or(""))
            .unwrap_or(ArchiveFormat::Zip);
    if fmt == ArchiveFormat::Zip {
        let _ = delete_from_zip(&dest_os, &[inner]);
        return add_to_zip_named(&dest_os, src, inner);
    }
    let _ = cli::delete(&dest_os, &[inner]);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("orchid-arc-as-{stamp}"));
    let staged = root.join(inner.replace('/', std::path::MAIN_SEPARATOR_STR));
    if let Some(parent) = staged.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if src.is_dir() {
        copy_dir_recursive(src, &staged)?;
    } else {
        std::fs::copy(src, &staged)?;
    }
    let first = inner.split('/').next().unwrap_or(inner);
    let add_path = root.join(first);
    let result = cli::add(&dest_os, &[add_path], None, None, false);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn add_to_zip_named(archive: &Path, src: &Path, inner: &str) -> Result<()> {
    add_to_zip_with_name(archive, src, inner)
}

fn add_to_zip_with_name(archive: &Path, src: &Path, inner: &str) -> Result<()> {
    let tmp = archive.with_extension("zip.partial");
    {
        let existing = std::fs::File::open(archive)?;
        let mut src_zip = zip::ZipArchive::new(existing)?;
        let out = std::fs::File::create(&tmp)?;
        let mut dest = zip::ZipWriter::new(out);
        for i in 0..src_zip.len() {
            let entry = src_zip.by_index(i)?;
            dest.raw_copy_file(entry)?;
        }
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        append_path_to_zip(&mut dest, src, inner, opts)?;
        dest.finish()?;
    }
    std::fs::rename(&tmp, archive)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for ent in walkdir::WalkDir::new(src).into_iter().flatten() {
        let p = ent.path();
        if p == src {
            continue;
        }
        let rel = p.strip_prefix(src).unwrap_or(p);
        let to = dest.join(rel);
        if p.is_dir() {
            std::fs::create_dir_all(&to)?;
        } else if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
            std::fs::copy(p, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::FsPath;

    #[tokio::test]
    async fn zip_create_extract_test_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("hello.txt");
        std::fs::write(&src, b"hello-archive").unwrap();
        let zip = dir.path().join("pack.zip");
        let dest = FsPath::from_local(&zip).unwrap();
        let sources = [FsPath::from_local(&src).unwrap()];
        create_archive(&dest, &sources, CreateArchiveOptions::default())
            .await
            .unwrap();
        let report = test_archive(&dest, None).await.unwrap();
        assert!(report.ok, "{}", report.summary);
        let out = dir.path().join("out");
        let n = extract_archive(&dest, &FsPath::from_local(&out).unwrap(), &[], None)
            .await
            .unwrap();
        assert!(n >= 1);
        assert_eq!(
            std::fs::read(out.join("hello.txt")).unwrap(),
            b"hello-archive"
        );
        add_to_archive(&dest, &[FsPath::from_local(&src).unwrap()])
            .await
            .unwrap();
        delete_from_archive(&dest, &["hello.txt".into()])
            .await
            .unwrap();
    }
}
