//! Folder compare, byte-level file compare, and folder sync / merge.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use tokio::io::AsyncReadExt;

use crate::entry::{FsEntryKind, FsMetadata};
use crate::error::{FsError, Result};
use crate::operations::copy::{copy_with_control, CopyOptions};
use crate::operations::progress::TransferControl;
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// How a relative path differs between two folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// Present only in the left tree.
    LeftOnly,
    /// Present only in the right tree.
    RightOnly,
    /// Present in both but size / mtime / bytes differ.
    Different,
    /// Present in both and considered equal.
    Identical,
}

/// One relative path in a folder comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirDiffEntry {
    /// Path relative to both roots, using `/`.
    pub rel: String,
    /// Classification.
    pub kind: DiffKind,
}

/// Result of comparing two directory trees.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirDiff {
    /// Every compared file (directories are not listed).
    pub entries: Vec<DirDiffEntry>,
}

impl DirDiff {
    /// Count entries of each [`DiffKind`].
    #[must_use]
    pub fn counts(&self) -> DiffCounts {
        let mut c = DiffCounts::default();
        for e in &self.entries {
            match e.kind {
                DiffKind::LeftOnly => c.left_only += 1,
                DiffKind::RightOnly => c.right_only += 1,
                DiffKind::Different => c.different += 1,
                DiffKind::Identical => c.identical += 1,
            }
        }
        c
    }

    /// Human-readable report, listing at most `limit` paths per section.
    #[must_use]
    pub fn format_report(&self, limit: usize) -> String {
        let c = self.counts();
        let mut s = String::new();
        let _ = writeln!(
            &mut s,
            "← only {left}  → only {right}  different {diff}  identical {same}",
            left = c.left_only,
            right = c.right_only,
            diff = c.different,
            same = c.identical
        );
        write_section(
            &mut s,
            "Left only",
            DiffKind::LeftOnly,
            &self.entries,
            limit,
        );
        write_section(
            &mut s,
            "Right only",
            DiffKind::RightOnly,
            &self.entries,
            limit,
        );
        write_section(
            &mut s,
            "Different",
            DiffKind::Different,
            &self.entries,
            limit,
        );
        s
    }
}

/// Totals from [`DirDiff::counts`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffCounts {
    /// Files only on the left.
    pub left_only: usize,
    /// Files only on the right.
    pub right_only: usize,
    /// Files that differ.
    pub different: usize,
    /// Matching files.
    pub identical: usize,
}

fn write_section(
    s: &mut String,
    title: &str,
    kind: DiffKind,
    entries: &[DirDiffEntry],
    limit: usize,
) {
    let rows: Vec<&str> = entries
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.rel.as_str())
        .collect();
    if rows.is_empty() {
        return;
    }
    let _ = writeln!(s, "\n{title}:");
    for r in rows.iter().take(limit) {
        let _ = writeln!(s, "  {r}");
    }
    if rows.len() > limit {
        let _ = writeln!(s, "  … {} more", rows.len() - limit);
    }
}

/// Compare two folders. When `byte_level` is true, files with equal size are
/// compared by content; otherwise size + modified time are used.
///
/// # Errors
///
/// Propagates listing / read errors.
pub async fn compare_dirs(
    registry: &FsProviderRegistry,
    left: &FsPath,
    right: &FsPath,
    byte_level: bool,
) -> Result<DirDiff> {
    let left_tree = collect_files(registry, left).await?;
    let right_tree = collect_files(registry, right).await?;
    let mut rels: Vec<String> = left_tree.keys().chain(right_tree.keys()).cloned().collect();
    rels.sort();
    rels.dedup();
    let mut entries = Vec::with_capacity(rels.len());
    for rel in rels {
        match (left_tree.get(&rel), right_tree.get(&rel)) {
            (Some(_), None) => entries.push(DirDiffEntry {
                rel,
                kind: DiffKind::LeftOnly,
            }),
            (None, Some(_)) => entries.push(DirDiffEntry {
                rel,
                kind: DiffKind::RightOnly,
            }),
            (Some((lp, lm)), Some((rp, rm))) => {
                let kind = classify_pair(registry, lp, lm, rp, rm, byte_level).await?;
                entries.push(DirDiffEntry { rel, kind });
            }
            (None, None) => {}
        }
    }
    Ok(DirDiff { entries })
}

async fn classify_pair(
    registry: &FsProviderRegistry,
    left: &FsPath,
    lm: &FsMetadata,
    right: &FsPath,
    rm: &FsMetadata,
    byte_level: bool,
) -> Result<DiffKind> {
    if lm.size != rm.size {
        return Ok(DiffKind::Different);
    }
    if byte_level {
        return Ok(if files_bytes_equal(registry, left, right).await? {
            DiffKind::Identical
        } else {
            DiffKind::Different
        });
    }
    if lm.modified == rm.modified {
        Ok(DiffKind::Identical)
    } else {
        Ok(DiffKind::Different)
    }
}

/// Byte-level compare of two files. Returns the first mismatch offset, or
/// `None` when the files are identical (including two empty files).
///
/// # Errors
///
/// Propagates provider / I/O errors.
pub async fn compare_files(
    registry: &FsProviderRegistry,
    left: &FsPath,
    right: &FsPath,
) -> Result<FileCompare> {
    let lp = registry
        .for_path(left)
        .ok_or_else(|| FsError::ProviderNotMounted(left.to_string()))?;
    let rp = registry
        .for_path(right)
        .ok_or_else(|| FsError::ProviderNotMounted(right.to_string()))?;
    let lm = lp.metadata(left).await?;
    let rm = rp.metadata(right).await?;
    if lm.size != rm.size {
        return Ok(FileCompare {
            equal: false,
            left_size: lm.size,
            right_size: rm.size,
            mismatch_at: Some(lm.size.min(rm.size)),
        });
    }
    let mut lr = lp.read_stream(left).await?;
    let mut rr = rp.read_stream(right).await?;
    let mut lb = vec![0u8; 64 * 1024];
    let mut rb = vec![0u8; 64 * 1024];
    let mut offset = 0u64;
    loop {
        let ln = lr.read(&mut lb).await?;
        let rn = rr.read(&mut rb).await?;
        let n = ln.min(rn);
        if n == 0 {
            let equal = ln == rn;
            return Ok(FileCompare {
                equal,
                left_size: lm.size,
                right_size: rm.size,
                mismatch_at: if equal { None } else { Some(offset) },
            });
        }
        if lb[..n] != rb[..n] {
            let at = lb[..n]
                .iter()
                .zip(&rb[..n])
                .position(|(a, b)| a != b)
                .unwrap_or(0) as u64;
            return Ok(FileCompare {
                equal: false,
                left_size: lm.size,
                right_size: rm.size,
                mismatch_at: Some(offset + at),
            });
        }
        if ln != rn {
            return Ok(FileCompare {
                equal: false,
                left_size: lm.size,
                right_size: rm.size,
                mismatch_at: Some(offset + n as u64),
            });
        }
        offset += n as u64;
    }
}

/// Outcome of [`compare_files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileCompare {
    /// Files have identical bytes.
    pub equal: bool,
    /// Left file size.
    pub left_size: u64,
    /// Right file size.
    pub right_size: u64,
    /// First differing byte, if any.
    pub mismatch_at: Option<u64>,
}

async fn files_bytes_equal(
    registry: &FsProviderRegistry,
    left: &FsPath,
    right: &FsPath,
) -> Result<bool> {
    Ok(compare_files(registry, left, right).await?.equal)
}

/// Sync / merge direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// Copy missing and different files left → right.
    ToRight,
    /// Copy missing and different files right → left.
    ToLeft,
    /// Two-way: copy missing both ways; different files take the newer mtime.
    Both,
    /// Copy left-only files to the right; never overwrite.
    MergeToRight,
}

/// How many files a sync copied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncStats {
    /// Files copied left → right.
    pub to_right: usize,
    /// Files copied right → left.
    pub to_left: usize,
}

/// Synchronise `left` and `right` according to `mode`.
///
/// # Errors
///
/// Propagates compare / copy errors.
pub async fn sync_dirs(
    registry: &FsProviderRegistry,
    left: &FsPath,
    right: &FsPath,
    mode: SyncMode,
    byte_level: bool,
) -> Result<SyncStats> {
    let diff = compare_dirs(registry, left, right, byte_level).await?;
    let left_tree = collect_files(registry, left).await?;
    let right_tree = collect_files(registry, right).await?;
    let opts = CopyOptions {
        overwrite: !matches!(mode, SyncMode::MergeToRight),
        skip_existing: matches!(mode, SyncMode::MergeToRight),
        overwrite_older: matches!(mode, SyncMode::Both),
        ..CopyOptions::default()
    };
    let mut stats = SyncStats::default();
    let ctl = TransferControl::default();
    for e in &diff.entries {
        match (mode, e.kind) {
            (SyncMode::ToRight | SyncMode::MergeToRight, DiffKind::LeftOnly)
            | (SyncMode::ToRight, DiffKind::Different) => {
                copy_rel(registry, left, right, &e.rel, opts, ctl.clone()).await?;
                stats.to_right += 1;
            }
            (SyncMode::ToLeft, DiffKind::RightOnly | DiffKind::Different) => {
                copy_rel(registry, right, left, &e.rel, opts, ctl.clone()).await?;
                stats.to_left += 1;
            }
            (SyncMode::Both, DiffKind::LeftOnly) => {
                copy_rel(registry, left, right, &e.rel, opts, ctl.clone()).await?;
                stats.to_right += 1;
            }
            (SyncMode::Both, DiffKind::RightOnly) => {
                copy_rel(registry, right, left, &e.rel, opts, ctl.clone()).await?;
                stats.to_left += 1;
            }
            (SyncMode::Both, DiffKind::Different) => {
                let l_newer = newer(left_tree.get(&e.rel), right_tree.get(&e.rel));
                if l_newer {
                    copy_rel(registry, left, right, &e.rel, opts, ctl.clone()).await?;
                    stats.to_right += 1;
                } else {
                    copy_rel(registry, right, left, &e.rel, opts, ctl.clone()).await?;
                    stats.to_left += 1;
                }
            }
            (SyncMode::MergeToRight, DiffKind::Different | DiffKind::RightOnly) => {}
            (_, DiffKind::Identical) => {}
            (SyncMode::ToRight, DiffKind::RightOnly) => {}
            (SyncMode::ToLeft, DiffKind::LeftOnly) => {}
        }
    }
    Ok(stats)
}

fn newer(left: Option<&(FsPath, FsMetadata)>, right: Option<&(FsPath, FsMetadata)>) -> bool {
    match (
        left.and_then(|(_, m)| m.modified),
        right.and_then(|(_, m)| m.modified),
    ) {
        (Some(l), Some(r)) => l >= r,
        (Some(_), None) => true,
        _ => false,
    }
}

async fn copy_rel(
    registry: &FsProviderRegistry,
    from_root: &FsPath,
    to_root: &FsPath,
    rel: &str,
    opts: CopyOptions,
    ctl: TransferControl,
) -> Result<()> {
    let src = from_root.join(rel);
    let dest = to_root.join(rel);
    if let Some(parent) = dest.parent() {
        if let Some(p) = registry.for_path(&parent) {
            let _ = p.create_dir(&parent, true).await;
        }
    }
    copy_with_control(registry, &src, &dest, opts, None, ctl).await
}

async fn collect_files(
    registry: &FsProviderRegistry,
    root: &FsPath,
) -> Result<BTreeMap<String, (FsPath, FsMetadata)>> {
    let provider = registry
        .for_path(root)
        .ok_or_else(|| FsError::ProviderNotMounted(root.to_string()))?;
    let mut out = BTreeMap::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let kids = provider.list(&dir).await?;
        for e in kids {
            let rel = relative_to(root, &e.path).unwrap_or_else(|| e.name.clone());
            match e.metadata.kind {
                FsEntryKind::Directory => stack.push(e.path),
                FsEntryKind::File | FsEntryKind::Other => {
                    out.insert(rel.replace('\\', "/"), (e.path, e.metadata));
                }
                FsEntryKind::Symlink => {}
            }
        }
    }
    Ok(out)
}

fn relative_to(root: &FsPath, path: &FsPath) -> Option<String> {
    let root_b = root.without_scheme().trim_end_matches('/');
    let path_b = path.without_scheme();
    let rest = path_b.strip_prefix(root_b)?;
    let rest = rest.trim_start_matches('/');
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{FsProviderRegistry, LocalProvider};
    use std::sync::Arc;

    fn registry() -> FsProviderRegistry {
        let r = FsProviderRegistry::new();
        r.register(Arc::new(LocalProvider::new())).unwrap();
        r
    }

    #[tokio::test]
    async fn compare_and_sync_local() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("l");
        let right = dir.path().join("r");
        std::fs::create_dir_all(left.join("sub")).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(left.join("a.txt"), b"aaa").unwrap();
        std::fs::write(left.join("sub/b.txt"), b"bbb").unwrap();
        std::fs::write(right.join("a.txt"), b"zzz").unwrap();
        let lp = FsPath::from_local(&left).unwrap();
        let rp = FsPath::from_local(&right).unwrap();
        let reg = registry();
        let diff = compare_dirs(&reg, &lp, &rp, true).await.unwrap();
        let c = diff.counts();
        assert_eq!(c.left_only, 1);
        assert_eq!(c.different, 1);
        let stats = sync_dirs(&reg, &lp, &rp, SyncMode::MergeToRight, true)
            .await
            .unwrap();
        assert_eq!(stats.to_right, 1);
        assert!(right.join("sub/b.txt").exists());
        assert_eq!(std::fs::read(right.join("a.txt")).unwrap(), b"zzz");
    }

    #[tokio::test]
    async fn byte_compare_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        std::fs::write(&a, b"abcd").unwrap();
        std::fs::write(&b, b"abXd").unwrap();
        let reg = registry();
        let r = compare_files(
            &reg,
            &FsPath::from_local(&a).unwrap(),
            &FsPath::from_local(&b).unwrap(),
        )
        .await
        .unwrap();
        assert!(!r.equal);
        assert_eq!(r.mismatch_at, Some(2));
    }
}
