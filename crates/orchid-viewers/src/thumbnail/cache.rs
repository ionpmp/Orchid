//! Disk-backed thumbnail cache.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs;

use crate::error::{Result, ViewerError};

use super::{Thumbnail, ThumbnailSize};

/// Cache rooted at a directory; entries keyed by `BLAKE3(path + mtime)`.
pub struct ThumbnailCache {
    root: PathBuf,
}

impl ThumbnailCache {
    /// Build the cache, creating the root directory if needed.
    ///
    /// # Errors
    ///
    /// Propagates IO errors from directory creation.
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file_for(&self, key: &[u8; 32], size: ThumbnailSize) -> PathBuf {
        let hex = hex_lower(key);
        let shard = &hex[..2];
        self.root
            .join(shard)
            .join(format!("{}_{}.png", &hex[2..], size.suffix()))
    }

    /// Fetch a cached thumbnail, if present.
    ///
    /// # Errors
    ///
    /// Propagates IO / decode errors.
    pub async fn get(&self, key: &[u8; 32], size: ThumbnailSize) -> Result<Option<Thumbnail>> {
        let path = self.file_for(key, size);
        if !fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(None);
        }
        let bytes = fs::read(&path).await?;
        let decoded = tokio::task::spawn_blocking(move || {
            image::load_from_memory(&bytes)
                .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))
                .map(|img| {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    Thumbnail {
                        rgba: Arc::new(rgba.into_raw()),
                        width: w,
                        height: h,
                    }
                })
        })
        .await
        .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))??;
        Ok(Some(decoded))
    }

    /// Store a thumbnail.
    ///
    /// # Errors
    ///
    /// Propagates IO / encode errors.
    pub async fn put(
        &self,
        key: &[u8; 32],
        size: ThumbnailSize,
        thumb: &Thumbnail,
    ) -> Result<()> {
        let path = self.file_for(key, size);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let rgba = Arc::try_unwrap(Arc::clone(&thumb.rgba)).unwrap_or_else(|a| (*a).clone());
        let w = thumb.width;
        let h = thumb.height;
        let bytes = tokio::task::spawn_blocking(move || {
            let img = image::RgbaImage::from_raw(w, h, rgba)
                .ok_or_else(|| ViewerError::ThumbnailFailed("invalid RGBA".into()))?;
            let mut buf = std::io::Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
            Ok::<_, ViewerError>(buf.into_inner())
        })
        .await
        .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))??;
        fs::write(path, bytes).await?;
        Ok(())
    }

    /// Remove every size for the given key.
    ///
    /// # Errors
    ///
    /// Propagates IO errors; missing files are not errors.
    pub async fn invalidate_prefix(&self, key: &[u8; 32]) -> Result<()> {
        for size in [ThumbnailSize::Small, ThumbnailSize::Medium, ThumbnailSize::Large] {
            let path = self.file_for(key, size);
            if fs::try_exists(&path).await.unwrap_or(false) {
                let _ = fs::remove_file(path).await;
            }
        }
        Ok(())
    }
}

fn hex_lower(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_then_get_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ThumbnailCache::new(tmp.path().to_path_buf()).unwrap();
        let rgba: Vec<u8> = (0..(4 * 4 * 4)).map(|i| i as u8).collect();
        let thumb = Thumbnail {
            rgba: Arc::new(rgba.clone()),
            width: 4,
            height: 4,
        };
        let key = [7u8; 32];
        cache.put(&key, ThumbnailSize::Small, &thumb).await.unwrap();
        let got = cache.get(&key, ThumbnailSize::Small).await.unwrap().unwrap();
        assert_eq!(got.width, 4);
        assert_eq!(got.height, 4);
        assert_eq!(got.rgba.len(), rgba.len());
    }

    #[tokio::test]
    async fn invalidate_removes_all_sizes() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ThumbnailCache::new(tmp.path().to_path_buf()).unwrap();
        let rgba: Vec<u8> = vec![0u8; 16];
        let thumb = Thumbnail {
            rgba: Arc::new(rgba),
            width: 2,
            height: 2,
        };
        let key = [3u8; 32];
        for size in [ThumbnailSize::Small, ThumbnailSize::Medium, ThumbnailSize::Large] {
            cache.put(&key, size, &thumb).await.unwrap();
        }
        cache.invalidate_prefix(&key).await.unwrap();
        for size in [ThumbnailSize::Small, ThumbnailSize::Medium, ThumbnailSize::Large] {
            assert!(cache.get(&key, size).await.unwrap().is_none());
        }
    }
}
