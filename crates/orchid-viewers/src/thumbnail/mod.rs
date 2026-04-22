//! Thumbnail service.

pub mod cache;
pub mod generator;

use std::sync::Arc;

use dashmap::DashMap;
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
}

/// A decoded thumbnail ready for the UI.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct Thumbnail {
    pub rgba: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

/// Thumbnail-generation / cache facade.
pub struct ThumbnailService {
    cache: Arc<ThumbnailCache>,
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
        self.cache.get(key, size).await
    }

    /// Generate a thumbnail from raw image bytes and store it in the cache.
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
        // Collapse duplicate generations for the same key.
        let notify = self
            .in_flight
            .entry(key)
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone();

        let thumb =
            tokio::task::spawn_blocking(move || generator::image_thumbnail(&bytes, size.to_pixels()))
                .await
                .map_err(|e| crate::error::ViewerError::ThumbnailFailed(e.to_string()))??;
        self.cache.put(&key, size, &thumb).await?;
        self.in_flight.remove(&key);
        notify.notify_waiters();
        Ok(thumb)
    }

    /// Drop every cached size for the given key.
    ///
    /// # Errors
    ///
    /// Propagates IO errors.
    pub async fn invalidate(&self, key: &[u8; 32]) -> Result<()> {
        self.cache.invalidate_prefix(key).await
    }
}
