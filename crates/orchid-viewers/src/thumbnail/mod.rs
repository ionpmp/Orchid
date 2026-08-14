//! Thumbnail service.

pub mod cache;
pub mod contact_sheet;
pub mod exif_preview;
pub mod generator;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::sync::Notify;

use crate::error::Result;

pub use cache::ThumbnailCache;

/// Thumbnail size bucket.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbnailSize {
    Small,
    Medium,
    Large,
}

impl ThumbnailSize {
    /// Pixel dimension (thumbnails are square with aspect-preserving fit).
    #[must_use]
    pub fn to_pixels(self) -> u32 {
        match self {
            Self::Small => 64,
            Self::Medium => 128,
            Self::Large => 256,
        }
    }

    /// Short suffix used in cache filenames.
    #[must_use]
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Small => "s",
            Self::Medium => "m",
            Self::Large => "l",
        }
    }

    /// Persist / snapshot encoding (`0` small, `1` medium, `2` large).
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Small => 0,
            Self::Medium => 1,
            Self::Large => 2,
        }
    }

    /// Inverse of [`Self::as_u8`]; unknown values become medium.
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Small,
            2 => Self::Large,
            _ => Self::Medium,
        }
    }

    /// Cycle small → medium → large → small.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Small => Self::Medium,
            Self::Medium => Self::Large,
            Self::Large => Self::Small,
        }
    }
}

/// A decoded thumbnail ready for the UI.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct Thumbnail {
    pub rgba: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

const MEMORY_LRU_CAP: usize = 256;

type CacheKey = ([u8; 32], ThumbnailSize);

/// In-process LRU of decoded RGBA thumbnails (avoids PNG re-decode on scroll).
struct MemoryLru {
    map: HashMap<CacheKey, Thumbnail>,
    order: VecDeque<CacheKey>,
}

impl MemoryLru {
    fn new() -> Self {
        Self {
            map: HashMap::with_capacity(MEMORY_LRU_CAP),
            order: VecDeque::with_capacity(MEMORY_LRU_CAP),
        }
    }

    fn get(&mut self, key: &CacheKey) -> Option<Thumbnail> {
        let thumb = self.map.get(key)?.clone();
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
            self.order.push_back(*key);
        }
        Some(thumb)
    }

    fn put(&mut self, key: CacheKey, thumb: Thumbnail) {
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.map.entry(key) {
            e.insert(thumb);
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key);
            return;
        }
        while self.map.len() >= MEMORY_LRU_CAP {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            } else {
                break;
            }
        }
        self.map.insert(key, thumb);
        self.order.push_back(key);
    }
}

/// Thumbnail-generation / cache facade.
pub struct ThumbnailService {
    cache: Arc<ThumbnailCache>,
    memory: Mutex<MemoryLru>,
    /// In-flight generation: waiters await the first job via [`Notify`].
    in_flight: DashMap<[u8; 32], Arc<Notify>>,
}

impl std::fmt::Debug for ThumbnailService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailService")
            .field("in_flight", &self.in_flight.len())
            .finish_non_exhaustive()
    }
}

impl ThumbnailService {
    /// Build a service backed by a disk cache at `cache_dir`.
    ///
    /// # Errors
    ///
    /// Propagates IO errors when creating the cache directory.
    pub fn new(cache_dir: std::path::PathBuf) -> Result<Self> {
        let cache = Arc::new(ThumbnailCache::new(cache_dir)?);
        Ok(Self {
            cache,
            memory: Mutex::new(MemoryLru::new()),
            in_flight: DashMap::new(),
        })
    }

    /// Cache handle (exposed for tests / diagnostics).
    #[must_use]
    pub fn cache(&self) -> &ThumbnailCache {
        &self.cache
    }

    /// Compute the canonical cache key for a path + modified timestamp.
    #[must_use]
    pub fn cache_key(path: &orchid_fs::FsPath, modified_ms: i64) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(path.as_str().as_bytes());
        hasher.update(&modified_ms.to_le_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Fetch a cached thumbnail or return `None` if the cache is cold.
    ///
    /// # Errors
    ///
    /// Propagates IO errors from the cache.
    pub async fn get_cached(
        &self,
        key: &[u8; 32],
        size: ThumbnailSize,
    ) -> Result<Option<Thumbnail>> {
        let ck = (*key, size);
        if let Some(thumb) = self.memory.lock().get(&ck) {
            return Ok(Some(thumb));
        }
        let Some(thumb) = self.cache.get(key, size).await? else {
            return Ok(None);
        };
        self.memory.lock().put(ck, thumb.clone());
        Ok(Some(thumb))
    }

    /// Generate a thumbnail from raw image bytes and store it in the cache.
    ///
    /// Concurrent callers for the same key wait for the first generation instead
    /// of decoding in parallel.
    ///
    /// # Errors
    ///
    /// Propagates generation and IO errors.
    pub async fn generate_from_image_bytes(
        &self,
        key: [u8; 32],
        size: ThumbnailSize,
        bytes: Vec<u8>,
    ) -> Result<Thumbnail> {
        if let Some(thumb) = self.get_cached(&key, size).await? {
            return Ok(thumb);
        }
        self.generate_coalesced(key, size, move || {
            generator::image_thumbnail(&bytes, size.to_pixels())
        })
        .await
    }

    /// Generate a thumbnail by memory-mapping a local image file.
    ///
    /// Avoids copying the whole file into a [`Vec`] before decode. Files larger
    /// than 16 MiB are rejected.
    ///
    /// # Errors
    ///
    /// Propagates IO / decode / cache errors.
    pub async fn generate_from_local_path(
        &self,
        key: [u8; 32],
        size: ThumbnailSize,
        path: std::path::PathBuf,
    ) -> Result<Thumbnail> {
        if let Some(thumb) = self.get_cached(&key, size).await? {
            return Ok(thumb);
        }
        let target = size.to_pixels();
        self.generate_coalesced(key, size, move || {
            const MAX_BYTES: u64 = 16 * 1024 * 1024;
            let file = std::fs::File::open(&path)?;
            let meta = file.metadata()?;
            if meta.len() > MAX_BYTES {
                return Err(crate::error::ViewerError::ThumbnailFailed(
                    "image too large for thumbnail".into(),
                ));
            }
            // SAFETY: the file is opened read-only and not truncated by us while mapped.
            let map = unsafe { memmap2::Mmap::map(&file) }?;
            generator::image_thumbnail(&map, target)
        })
        .await
    }

    async fn generate_coalesced<F>(
        &self,
        key: [u8; 32],
        size: ThumbnailSize,
        work: F,
    ) -> Result<Thumbnail>
    where
        F: FnOnce() -> Result<Thumbnail> + Send + 'static,
    {
        // If another task is already generating this key, wait then re-read cache.
        if let Some(existing) = self.in_flight.get(&key) {
            let notify = existing.clone();
            drop(existing);
            notify.notified().await;
            if let Some(thumb) = self.get_cached(&key, size).await? {
                return Ok(thumb);
            }
            // Leader failed or raced — fall through and try again as leader.
        }

        let notify = Arc::new(Notify::new());
        match self.in_flight.entry(key) {
            dashmap::mapref::entry::Entry::Occupied(entry) => {
                let notify = entry.get().clone();
                drop(entry);
                notify.notified().await;
                if let Some(thumb) = self.get_cached(&key, size).await? {
                    return Ok(thumb);
                }
                // Still missing — become a new leader below.
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(Arc::clone(&notify));
            }
        }

        // Re-check we own the slot (another leader may have finished meanwhile).
        if let Some(thumb) = self.get_cached(&key, size).await? {
            self.in_flight.remove(&key);
            notify.notify_waiters();
            return Ok(thumb);
        }
        if !self
            .in_flight
            .get(&key)
            .is_some_and(|n| Arc::ptr_eq(n.value(), &notify))
        {
            // We are not the leader; wait for whoever is.
            if let Some(existing) = self.in_flight.get(&key) {
                let n = existing.clone();
                drop(existing);
                n.notified().await;
            }
            if let Some(thumb) = self.get_cached(&key, size).await? {
                return Ok(thumb);
            }
        }

        let result = tokio::task::spawn_blocking(work)
            .await
            .map_err(|e| crate::error::ViewerError::ThumbnailFailed(e.to_string()));

        let outcome = match result {
            Ok(inner) => inner,
            Err(e) => {
                self.in_flight.remove(&key);
                notify.notify_waiters();
                return Err(e);
            }
        };

        match outcome {
            Ok(thumb) => {
                self.memory.lock().put((key, size), thumb.clone());
                if let Err(e) = self.cache.put(&key, size, &thumb).await {
                    self.in_flight.remove(&key);
                    notify.notify_waiters();
                    return Err(e);
                }
                self.in_flight.remove(&key);
                notify.notify_waiters();
                Ok(thumb)
            }
            Err(e) => {
                self.in_flight.remove(&key);
                notify.notify_waiters();
                Err(e)
            }
        }
    }

    /// Drop every cached size for the given key.
    ///
    /// # Errors
    ///
    /// Propagates IO errors.
    pub async fn invalidate(&self, key: &[u8; 32]) -> Result<()> {
        {
            let mut mem = self.memory.lock();
            for size in [
                ThumbnailSize::Small,
                ThumbnailSize::Medium,
                ThumbnailSize::Large,
            ] {
                let ck = (*key, size);
                mem.map.remove(&ck);
                if let Some(pos) = mem.order.iter().position(|k| k == &ck) {
                    mem.order.remove(pos);
                }
            }
        }
        self.cache.invalidate_prefix(key).await
    }
}
