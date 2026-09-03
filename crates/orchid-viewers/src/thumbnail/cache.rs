//! Disk-backed thumbnail cache.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs;

use crate::error::{Result, ViewerError};

use super::{Thumbnail, ThumbnailSize};

/// `ORTH` + version + width + height, then packed RGBA8.
const RGBA_MAGIC: &[u8; 4] = b"ORTH";
const RGBA_VERSION: u8 = 1;
const RGBA_HEADER_LEN: usize = 13;

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
            .join(format!("{}_{}.rgba", &hex[2..], size.suffix()))
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
        let decoded = tokio::task::spawn_blocking(move || read_rgba_file(&path))
            .await
            .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))??;
        Ok(decoded)
    }

    /// Store a thumbnail.
    ///
    /// # Errors
    ///
    /// Propagates IO / encode errors.
    pub async fn put(&self, key: &[u8; 32], size: ThumbnailSize, thumb: &Thumbnail) -> Result<()> {
        let path = self.file_for(key, size);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let blob = encode_rgba_blob(thumb);
        fs::write(path, blob).await?;
        Ok(())
    }

    /// Remove every size for the given key.
    ///
    /// # Errors
    ///
    /// Propagates IO errors; missing files are not errors.
    pub async fn invalidate_prefix(&self, key: &[u8; 32]) -> Result<()> {
        for size in [
            ThumbnailSize::Small,
            ThumbnailSize::Medium,
            ThumbnailSize::Large,
        ] {
            let path = self.file_for(key, size);
            if fs::try_exists(&path).await.unwrap_or(false) {
                let _ = fs::remove_file(path).await;
            }
        }
        Ok(())
    }
}

fn encode_rgba_blob(thumb: &Thumbnail) -> Vec<u8> {
    let pixels = thumb.rgba.as_slice();
    let mut out = Vec::with_capacity(RGBA_HEADER_LEN + pixels.len());
    out.extend_from_slice(RGBA_MAGIC);
    out.push(RGBA_VERSION);
    out.extend_from_slice(&thumb.width.to_le_bytes());
    out.extend_from_slice(&thumb.height.to_le_bytes());
    out.extend_from_slice(pixels);
    out
}

fn parse_rgba_blob(bytes: &[u8]) -> Option<Thumbnail> {
    if bytes.len() < RGBA_HEADER_LEN || bytes[..4] != *RGBA_MAGIC || bytes[4] != RGBA_VERSION {
        return None;
    }
    let width = u32::from_le_bytes(bytes[5..9].try_into().ok()?);
    let height = u32::from_le_bytes(bytes[9..13].try_into().ok()?);
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    let pixels = bytes.get(RGBA_HEADER_LEN..)?;
    if pixels.len() != expected {
        return None;
    }
    Some(Thumbnail {
        rgba: Arc::new(pixels.to_vec()),
        width,
        height,
    })
}

fn read_rgba_file(path: &Path) -> Result<Option<Thumbnail>> {
    let file = std::fs::File::open(path)?;
    let map = unsafe { memmap2::Mmap::map(&file) }?;
    if let Some(thumb) = parse_rgba_blob(&map) {
        return Ok(Some(thumb));
    }
    Ok(None)
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
        let got = cache
            .get(&key, ThumbnailSize::Small)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.width, 4);
        assert_eq!(got.height, 4);
        assert_eq!(got.rgba.as_slice(), rgba.as_slice());
    }

    #[test]
    fn parse_rejects_truncated_header() {
        assert!(parse_rgba_blob(b"ORTH").is_none());
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
        for size in [
            ThumbnailSize::Small,
            ThumbnailSize::Medium,
            ThumbnailSize::Large,
        ] {
            cache.put(&key, size, &thumb).await.unwrap();
        }
        cache.invalidate_prefix(&key).await.unwrap();
        for size in [
            ThumbnailSize::Small,
            ThumbnailSize::Medium,
            ThumbnailSize::Large,
        ] {
            assert!(cache.get(&key, size).await.unwrap().is_none());
        }
    }
}
