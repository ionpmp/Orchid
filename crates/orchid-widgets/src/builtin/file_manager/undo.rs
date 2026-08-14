//! Session-only undo / redo for file-manager operations.
//!
//! Only honest inverses are recorded: newly created copy/move destinations,
//! rename pairs, new files/folders, and Recycle Bin deletes that we can match
//! after `trash::delete`. Permanent deletes and overwrites are not stacked.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use super::map_fs_error;
use super::FileManagerInner;
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

const UNDO_CAP: usize = 50;

/// One reversible file-manager operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FsUndoOp {
    /// Copy `src` → `dest` (dest was created by the copy).
    Copy { pairs: Vec<(String, String)> },
    /// Move `src` → `dest`.
    Move { pairs: Vec<(String, String)> },
    /// Rename `old` → `new`.
    Rename { pairs: Vec<(String, String)> },
    /// Delete-to-recycle; `items` are `virtual:recycle/…` paths.
    Recycle { items: Vec<String> },
    /// New file or folder. `recycle_items` is filled after an undo recycle.
    Create {
        paths: Vec<String>,
        recycle_items: Vec<String>,
    },
}

impl FsUndoOp {
    fn is_empty(&self) -> bool {
        match self {
            Self::Copy { pairs } | Self::Move { pairs } | Self::Rename { pairs } => {
                pairs.is_empty()
            }
            Self::Recycle { items } => items.is_empty(),
            Self::Create { paths, .. } => paths.is_empty(),
        }
    }
}

/// In-memory undo / redo stacks for one file-manager instance.
#[derive(Debug, Default)]
pub(super) struct FsUndoStack {
    undo: Vec<FsUndoOp>,
    redo: Vec<FsUndoOp>,
}

impl FsUndoStack {
    fn push(&mut self, op: FsUndoOp) {
        if op.is_empty() {
            return;
        }
        self.undo.push(op);
        if self.undo.len() > UNDO_CAP {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn pop_undo(&mut self) -> Option<FsUndoOp> {
        self.undo.pop()
    }

    fn pop_redo(&mut self) -> Option<FsUndoOp> {
        self.redo.pop()
    }

    fn push_redo(&mut self, op: FsUndoOp) {
        if !op.is_empty() {
            self.redo.push(op);
        }
    }

    fn restore_undo(&mut self, op: FsUndoOp) {
        if !op.is_empty() {
            self.undo.push(op);
        }
    }

    fn restore_redo(&mut self, op: FsUndoOp) {
        if !op.is_empty() {
            self.redo.push(op);
        }
    }

    fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

impl FileManagerInner {
    pub(super) fn record_undo(&self, op: FsUndoOp) {
        self.undo.lock().push(op);
    }

    pub(super) fn record_transfer_undo(&self, is_copy: bool, pairs: Vec<(String, String)>) {
        if is_copy {
            self.record_undo(FsUndoOp::Copy { pairs });
        } else {
            self.record_undo(FsUndoOp::Move { pairs });
        }
    }

    pub(super) fn can_undo(&self) -> bool {
        self.undo.lock().can_undo()
    }

    pub(super) fn can_redo(&self) -> bool {
        self.undo.lock().can_redo()
    }

    pub(super) async fn undo_last(self: &Arc<Self>) -> WidgetResult<()> {
        let Some(op) = self.undo.lock().pop_undo() else {
            return Ok(());
        };
        match self.apply_op(&op, true).await {
            Ok(updated) => {
                self.undo.lock().push_redo(updated);
                self.set_activity_notice("fm-undo-done", None);
                self.refresh_all_tabs().await;
                Ok(())
            }
            Err(e) => {
                self.undo.lock().restore_undo(op);
                Err(e)
            }
        }
    }

    pub(super) async fn redo_last(self: &Arc<Self>) -> WidgetResult<()> {
        let Some(op) = self.undo.lock().pop_redo() else {
            return Ok(());
        };
        match self.apply_op(&op, false).await {
            Ok(updated) => {
                self.undo.lock().restore_undo(updated);
                self.set_activity_notice("fm-redo-done", None);
                self.refresh_all_tabs().await;
                Ok(())
            }
            Err(e) => {
                self.undo.lock().restore_redo(op);
                Err(e)
            }
        }
    }

    async fn apply_op(&self, op: &FsUndoOp, undoing: bool) -> WidgetResult<FsUndoOp> {
        match op {
            FsUndoOp::Copy { pairs } => {
                if undoing {
                    let dests: Vec<String> = pairs.iter().map(|(_, d)| d.clone()).collect();
                    let existing = existing_paths(self, &dests).await;
                    if !existing.is_empty() {
                        self.delete_paths(&existing, Some(true)).await?;
                    }
                    Ok(op.clone())
                } else {
                    for (src, dest) in pairs {
                        copy_pair(self, src, dest).await?;
                    }
                    Ok(op.clone())
                }
            }
            FsUndoOp::Move { pairs } | FsUndoOp::Rename { pairs } => {
                let directed: Vec<(String, String)> = if undoing {
                    pairs.iter().map(|(a, b)| (b.clone(), a.clone())).collect()
                } else {
                    pairs.clone()
                };
                for (from, to) in &directed {
                    rename_pair(self, from, to).await?;
                }
                Ok(op.clone())
            }
            FsUndoOp::Recycle { items } => {
                if undoing {
                    orchid_fs::restore_recycle(items)
                        .await
                        .map_err(map_fs_error)?;
                    Ok(op.clone())
                } else {
                    let originals = originals_from_recycle_items(items);
                    if !originals.is_empty() {
                        self.delete_paths(&originals, Some(true)).await?;
                    }
                    let new_items = match_recycle_virtual_paths(&originals).await;
                    Ok(FsUndoOp::Recycle { items: new_items })
                }
            }
            FsUndoOp::Create {
                paths,
                recycle_items,
            } => {
                if undoing {
                    let existing = existing_paths(self, paths).await;
                    if !existing.is_empty() {
                        self.delete_paths(&existing, Some(true)).await?;
                    }
                    let items = match_recycle_virtual_paths(paths).await;
                    Ok(FsUndoOp::Create {
                        paths: paths.clone(),
                        recycle_items: items,
                    })
                } else if !recycle_items.is_empty() {
                    orchid_fs::restore_recycle(recycle_items)
                        .await
                        .map_err(map_fs_error)?;
                    Ok(FsUndoOp::Create {
                        paths: paths.clone(),
                        recycle_items: Vec::new(),
                    })
                } else {
                    Err(WidgetError::InvalidStateForOperation(
                        "fm-error-redo-failed".into(),
                    ))
                }
            }
        }
    }
}

async fn copy_pair(inner: &FileManagerInner, src: &str, dest: &str) -> WidgetResult<()> {
    let src = orchid_fs::FsPath::new(src).map_err(map_fs_error)?;
    let dest = orchid_fs::FsPath::new(dest).map_err(map_fs_error)?;
    orchid_fs::copy(
        &inner.deps.registry,
        &src,
        &dest,
        orchid_fs::CopyOptions::default(),
        None,
        None,
    )
    .await
    .map_err(map_fs_error)
}

async fn rename_pair(inner: &FileManagerInner, from: &str, to: &str) -> WidgetResult<()> {
    let old = orchid_fs::FsPath::new(from).map_err(map_fs_error)?;
    let new = orchid_fs::FsPath::new(to).map_err(map_fs_error)?;
    let provider = inner
        .deps
        .registry
        .for_path(&old)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-no-provider-path".into()))?;
    provider.rename(&old, &new).await.map_err(map_fs_error)
}

async fn existing_paths(inner: &FileManagerInner, paths: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for p in paths {
        let Ok(fp) = orchid_fs::FsPath::new(p) else {
            continue;
        };
        let Some(prov) = inner.deps.registry.for_path(&fp) else {
            continue;
        };
        if prov.exists(&fp).await.unwrap_or(false) {
            out.push(p.clone());
        }
    }
    out
}

fn originals_from_recycle_items(items: &[String]) -> Vec<String> {
    items
        .iter()
        .filter_map(|p| {
            let orig = orchid_fs::recycle_original_path(p)?;
            orchid_fs::FsPath::from_local(Path::new(&orig))
                .ok()
                .map(|fp| fp.as_str().to_string())
        })
        .collect()
}

/// After a recycle-bin delete, match trash rows back to the deleted FM paths.
pub(super) async fn match_recycle_virtual_paths(deleted: &[String]) -> Vec<String> {
    let Ok(mut items) = orchid_fs::list_recycle().await else {
        return Vec::new();
    };
    items.sort_by_key(|item| std::cmp::Reverse(item.deleted_at));
    let mut used = HashSet::new();
    let mut matched = Vec::new();
    for path in deleted {
        if let Some(item) = items
            .iter()
            .find(|it| !used.contains(&it.id) && original_matches_deleted(&it.original_path, path))
        {
            used.insert(item.id.clone());
            if let Ok(vp) = item.virtual_path() {
                matched.push(vp.as_str().to_string());
            }
        }
    }
    matched
}

fn original_matches_deleted(original: &Path, deleted: &str) -> bool {
    if let Ok(fp) = orchid_fs::FsPath::new(deleted) {
        if let Ok(local) = fp.to_local() {
            if paths_eq(&local, original) {
                return true;
            }
        }
    }
    if let Ok(from_orig) = orchid_fs::FsPath::from_local(original) {
        return from_orig.as_str().eq_ignore_ascii_case(deleted);
    }
    false
}

fn paths_eq(a: &Path, b: &Path) -> bool {
    normalize_os_path(a) == normalize_os_path(b)
}

fn normalize_os_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        s.to_ascii_lowercase()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_clears_redo_and_caps() {
        let mut stack = FsUndoStack::default();
        stack.push(FsUndoOp::Create {
            paths: vec!["a".into()],
            recycle_items: vec![],
        });
        stack.push_redo(FsUndoOp::Create {
            paths: vec!["old".into()],
            recycle_items: vec![],
        });
        assert!(stack.can_redo());
        stack.push(FsUndoOp::Create {
            paths: vec!["b".into()],
            recycle_items: vec![],
        });
        assert!(!stack.can_redo());
        assert!(stack.can_undo());

        for i in 0..UNDO_CAP + 5 {
            stack.push(FsUndoOp::Create {
                paths: vec![format!("p{i}")],
                recycle_items: vec![],
            });
        }
        assert_eq!(stack.undo.len(), UNDO_CAP);
    }

    #[test]
    fn empty_ops_are_ignored() {
        let mut stack = FsUndoStack::default();
        stack.push(FsUndoOp::Copy { pairs: vec![] });
        stack.push(FsUndoOp::Recycle { items: vec![] });
        assert!(!stack.can_undo());
    }

    #[test]
    fn pop_undo_then_redo_roundtrip() {
        let mut stack = FsUndoStack::default();
        let op = FsUndoOp::Rename {
            pairs: vec![("old".into(), "new".into())],
        };
        stack.push(op.clone());
        let popped = stack.pop_undo().unwrap();
        assert_eq!(popped, op);
        assert!(!stack.can_undo());
        stack.push_redo(popped);
        assert!(stack.can_redo());
        let redone = stack.pop_redo().unwrap();
        stack.restore_undo(redone);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn path_match_is_slash_and_case_tolerant() {
        let os = Path::new(r"C:\Users\Alice\file.txt");
        assert!(original_matches_deleted(
            os,
            "local:c:/Users/Alice/file.txt"
        ));
        assert!(paths_eq(
            Path::new(r"C:\Users\Alice"),
            Path::new("c:/Users/Alice")
        ));
    }
}
