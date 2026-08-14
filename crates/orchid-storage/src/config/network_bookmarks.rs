//! Runtime network-place bookmarks persisted beside `config.toml`.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::schema::NetworkMountConfig;
use crate::error::Result;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct BookmarkFile {
    mounts: Vec<NetworkMountConfig>,
}

/// Load bookmarks. Missing or empty files yield an empty list.
#[must_use]
pub fn load_network_bookmarks(path: &Path) -> Vec<NetworkMountConfig> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    toml::from_str::<BookmarkFile>(&text)
        .map(|f| f.mounts)
        .unwrap_or_default()
}

/// Write bookmarks atomically.
///
/// # Errors
///
/// I/O or TOML serialization failures.
pub fn save_network_bookmarks(path: &Path, mounts: &[NetworkMountConfig]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(&BookmarkFile {
        mounts: mounts.to_vec(),
    })?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Config mounts first, then bookmarks whose URI is not already present.
#[must_use]
pub fn merge_network_places(
    config: &[NetworkMountConfig],
    bookmarks: &[NetworkMountConfig],
) -> Vec<NetworkMountConfig> {
    let mut out = config.to_vec();
    for b in bookmarks {
        let key = b.uri.trim();
        if key.is_empty() {
            continue;
        }
        if out.iter().any(|m| m.uri.trim() == key) {
            continue;
        }
        out.push(b.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_skips_duplicate_uri() {
        let cfg = vec![NetworkMountConfig {
            name: "A".into(),
            uri: "sftp:host/home".into(),
            ..NetworkMountConfig::default()
        }];
        let marks = vec![
            NetworkMountConfig {
                name: "A2".into(),
                uri: "sftp:host/home".into(),
                ..NetworkMountConfig::default()
            },
            NetworkMountConfig {
                name: "B".into(),
                uri: "s3:bucket".into(),
                ..NetworkMountConfig::default()
            },
        ];
        let merged = merge_network_places(&cfg, &marks);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].name, "B");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("network-bookmarks.toml");
        let mounts = vec![NetworkMountConfig {
            name: "Lab".into(),
            uri: "sftp:host/tmp".into(),
            user: Some("alice".into()),
            ..NetworkMountConfig::default()
        }];
        save_network_bookmarks(&path, &mounts).unwrap();
        let loaded = load_network_bookmarks(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Lab");
        assert_eq!(loaded[0].uri, "sftp:host/tmp");
        assert_eq!(loaded[0].user.as_deref(), Some("alice"));
    }
}
