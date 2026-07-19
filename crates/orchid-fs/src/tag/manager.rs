//! CRUD facade over the file-tag table.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use chrono::Utc;
use orchid_storage::{ColorLabel, FileTag};

use crate::error::{FsError, Result};
use crate::path::FsPath;
use crate::tag::TagsChangedEvent;

/// Tag manager operating on [`orchid_storage::StateStore`] under the hood.
#[derive(Clone)]
pub struct TagManager {
    storage: Arc<orchid_storage::StateStore>,
    bus: Arc<orchid_core::EventBus>,
}

impl std::fmt::Debug for TagManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TagManager").finish_non_exhaustive()
    }
}

impl TagManager {
    /// Construct a manager.
    #[must_use]
    pub fn new(
        storage: Arc<orchid_storage::StateStore>,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        Self { storage, bus }
    }

    /// Fetch the current record for `path`, if any.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn get(&self, path: &FsPath) -> Result<Option<FileTag>> {
        let r = self.storage.read()?;
        Ok(r.get_file_tag(path.as_str())?)
    }

    /// Fetch tags for many paths in a single read transaction.
    ///
    /// Only paths that have a stored record are present in the returned map.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn get_many(&self, paths: &[FsPath]) -> Result<HashMap<String, FileTag>> {
        if paths.is_empty() {
            return Ok(HashMap::new());
        }
        let r = self.storage.read()?;
        let mut out = HashMap::with_capacity(paths.len());
        for path in paths {
            if let Some(tag) = r.get_file_tag(path.as_str())? {
                out.insert(path.as_str().to_string(), tag);
            }
        }
        Ok(out)
    }

    /// Replace the tag set entirely.
    ///
    /// # Errors
    ///
    /// Propagates storage errors; [`FsError::InvalidPath`] if any
    /// normalised tag string is empty.
    pub fn set_tags(&self, path: &FsPath, tags: Vec<String>) -> Result<()> {
        let mut existing = self.load_or_new(path)?;
        let normalised = normalise_tag_list(tags)?;
        existing.tags = normalised;
        existing.updated_at = Utc::now();
        self.write(existing)?;
        self.publish(path);
        Ok(())
    }

    /// Append a tag, preserving existing ones and de-duplicating.
    ///
    /// # Errors
    ///
    /// Same as [`Self::set_tags`].
    pub fn add_tag(&self, path: &FsPath, tag: &str) -> Result<()> {
        let mut existing = self.load_or_new(path)?;
        let normalised = normalise_tag(tag)?;
        let mut set: BTreeSet<String> = existing.tags.iter().cloned().collect();
        set.insert(normalised);
        existing.tags = set.into_iter().collect();
        existing.updated_at = Utc::now();
        self.write(existing)?;
        self.publish(path);
        Ok(())
    }

    /// Remove a tag; no-op if absent.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn remove_tag(&self, path: &FsPath, tag: &str) -> Result<()> {
        let Some(mut existing) = self.get(path)? else {
            return Ok(());
        };
        let Ok(norm) = normalise_tag(tag) else {
            return Ok(());
        };
        existing.tags.retain(|t| t != &norm);
        existing.updated_at = Utc::now();
        self.write(existing)?;
        self.publish(path);
        Ok(())
    }

    /// Set (or clear) the colour label.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn set_color(&self, path: &FsPath, color: Option<ColorLabel>) -> Result<()> {
        let mut existing = self.load_or_new(path)?;
        existing.color_label = color;
        existing.updated_at = Utc::now();
        self.write(existing)?;
        self.publish(path);
        Ok(())
    }

    /// Set the starred flag.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn set_starred(&self, path: &FsPath, starred: bool) -> Result<()> {
        let mut existing = self.load_or_new(path)?;
        existing.starred = starred;
        existing.updated_at = Utc::now();
        self.write(existing)?;
        self.publish(path);
        Ok(())
    }

    /// Every distinct tag string present in the database.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn all_tags(&self) -> Result<Vec<String>> {
        let all = self.all_file_tags()?;
        let mut set: BTreeSet<String> = BTreeSet::new();
        for rec in all {
            for t in rec.tags {
                set.insert(t);
            }
        }
        Ok(set.into_iter().collect())
    }

    /// Paths that carry the given tag.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn paths_with_tag(&self, tag: &str) -> Result<Vec<FsPath>> {
        let needle = normalise_tag(tag).unwrap_or_else(|_| tag.to_lowercase());
        let mut out = Vec::new();
        for rec in self.all_file_tags()? {
            if rec.tags.iter().any(|t| t == &needle) {
                if let Ok(p) = FsPath::new(rec.path) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }

    /// Paths with the given colour label.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn paths_with_color(&self, color: ColorLabel) -> Result<Vec<FsPath>> {
        let mut out = Vec::new();
        for rec in self.all_file_tags()? {
            if rec.color_label == Some(color) {
                if let Ok(p) = FsPath::new(rec.path) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }

    /// Paths flagged as starred.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn starred_paths(&self) -> Result<Vec<FsPath>> {
        let mut out = Vec::new();
        for rec in self.all_file_tags()? {
            if rec.starred {
                if let Ok(p) = FsPath::new(rec.path) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }

    /// Full dump of the file-tag table. Direct redb access is needed because
    /// `StateStore` does not expose an iterator helper yet.
    fn all_file_tags(&self) -> Result<Vec<FileTag>> {
        use orchid_storage::state::tables::FILE_TAGS_TABLE;
        use redb::{ReadableDatabase, ReadableTable};

        let db = self.storage.raw_database();
        let txn = db
            .begin_read()
            .map_err(|e| crate::error::FsError::Storage(e.into()))?;
        // `fs_tags` may not exist yet if nothing has been written.
        let table = match txn.open_table(FILE_TAGS_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(crate::error::FsError::Storage(e.into())),
        };
        let mut out = Vec::new();
        for entry in table
            .iter()
            .map_err(|e| crate::error::FsError::Storage(e.into()))?
        {
            let (_, v) = entry.map_err(|e| crate::error::FsError::Storage(e.into()))?;
            out.push(v.value());
        }
        Ok(out)
    }

    // -----------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------

    fn load_or_new(&self, path: &FsPath) -> Result<FileTag> {
        Ok(self.get(path)?.unwrap_or_else(|| FileTag {
            path: path.as_str().to_string(),
            tags: Vec::new(),
            color_label: None,
            starred: false,
            updated_at: Utc::now(),
        }))
    }

    fn write(&self, tag: FileTag) -> Result<()> {
        let mut w = self.storage.write()?;
        w.put_file_tag(&tag)?;
        w.commit()?;
        Ok(())
    }

    fn publish(&self, path: &FsPath) {
        self.bus.publish(
            orchid_core::EventSource::Subsystem("fs.tag".into()),
            TagsChangedEvent {
                path: path.clone(),
                at: Utc::now(),
            },
        );
    }
}

fn normalise_tag(raw: &str) -> Result<String> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err(FsError::InvalidPath {
            reason: "tag cannot be empty".into(),
        });
    }
    Ok(trimmed)
}

fn normalise_tag_list(raw: Vec<String>) -> Result<Vec<String>> {
    let mut set = BTreeSet::new();
    for t in raw {
        set.insert(normalise_tag(&t)?);
    }
    Ok(set.into_iter().collect())
}

