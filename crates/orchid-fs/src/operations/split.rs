//! Split a file into numbered parts and join them back.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{FsError, Result};
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// Split `src` into chunks of `chunk_bytes` named `stem.001`, `stem.002`, …
///
/// # Errors
///
/// Fails when `chunk_bytes` is 0 or the provider cannot read/write.
pub async fn split_file(
    registry: &FsProviderRegistry,
    src: &FsPath,
    chunk_bytes: u64,
) -> Result<Vec<FsPath>> {
    if chunk_bytes == 0 {
        return Err(FsError::InvalidPath {
            reason: "split size must be greater than 0".into(),
        });
    }
    let provider = registry
        .for_path(src)
        .ok_or_else(|| FsError::ProviderNotMounted(src.to_string()))?;
    let parent = src.parent().ok_or_else(|| FsError::InvalidPath {
        reason: "file has no parent".into(),
    })?;
    let stem = src.file_name().unwrap_or("part").to_string();
    let mut reader = provider.read_stream(src).await?;
    let mut parts = Vec::new();
    let mut idx = 1u32;
    let chunk = chunk_bytes as usize;
    loop {
        let mut buf = vec![0u8; chunk];
        let mut filled = 0usize;
        while filled < chunk {
            let n = reader.read(&mut buf[filled..]).await?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        buf.truncate(filled);
        let dest = parent.join(&format!("{stem}.{idx:03}"));
        provider.write(&dest, &buf).await?;
        parts.push(dest);
        idx += 1;
        if filled < chunk {
            break;
        }
    }
    Ok(parts)
}

/// Concatenate `parts` (in the given order) into `dest`.
///
/// # Errors
///
/// Propagates provider / I/O errors.
pub async fn join_files(
    registry: &FsProviderRegistry,
    parts: &[FsPath],
    dest: &FsPath,
) -> Result<()> {
    if parts.is_empty() {
        return Err(FsError::InvalidPath {
            reason: "no parts to join".into(),
        });
    }
    let provider = registry
        .for_path(dest)
        .ok_or_else(|| FsError::ProviderNotMounted(dest.to_string()))?;
    let mut writer = provider.write_stream(dest).await?;
    let mut buf = vec![0u8; 1024 * 1024];
    for p in parts {
        let src = registry
            .for_path(p)
            .ok_or_else(|| FsError::ProviderNotMounted(p.to_string()))?;
        let mut reader = src.read_stream(p).await?;
        loop {
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n]).await?;
        }
    }
    writer.flush().await?;
    Ok(())
}

/// If `path` looks like `name.001`, collect sibling `name.002`… in order.
///
/// # Errors
///
/// Listing errors from the parent directory.
pub async fn discover_parts(registry: &FsProviderRegistry, path: &FsPath) -> Result<Vec<FsPath>> {
    let Some(name) = path.file_name() else {
        return Ok(vec![path.clone()]);
    };
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return Ok(vec![path.clone()]);
    };
    if ext.len() != 3 || !ext.chars().all(|c| c.is_ascii_digit()) {
        return Ok(vec![path.clone()]);
    }
    let Some(parent) = path.parent() else {
        return Ok(vec![path.clone()]);
    };
    let provider = registry
        .for_path(&parent)
        .ok_or_else(|| FsError::ProviderNotMounted(parent.to_string()))?;
    let mut found: Vec<(u32, FsPath)> = Vec::new();
    for e in provider.list(&parent).await? {
        let Some(n) = e.path.file_name() else {
            continue;
        };
        let Some((s, x)) = n.rsplit_once('.') else {
            continue;
        };
        if s != stem {
            continue;
        }
        if let Ok(i) = x.parse::<u32>() {
            found.push((i, e.path));
        }
    }
    found.sort_by_key(|(i, _)| *i);
    if found.is_empty() {
        Ok(vec![path.clone()])
    } else {
        Ok(found.into_iter().map(|(_, p)| p).collect())
    }
}

/// Suggested output name for joining `name.001` → `name`.
#[must_use]
pub fn join_output_name(first: &FsPath) -> String {
    let name = first.file_name().unwrap_or("joined").to_string();
    if let Some((stem, ext)) = name.rsplit_once('.') {
        if ext.len() == 3 && ext.chars().all(|c| c.is_ascii_digit()) {
            return stem.to_string();
        }
    }
    format!("{name}.joined")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::FsPath;
    use crate::provider::{FsProviderRegistry, LocalProvider};
    use std::sync::Arc;

    #[tokio::test]
    async fn split_join_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("blob.bin");
        let data: Vec<u8> = (0..250u8).collect();
        std::fs::write(&src, &data).unwrap();
        let reg = FsProviderRegistry::new();
        reg.register(Arc::new(LocalProvider::new())).unwrap();
        let src_p = FsPath::from_local(&src).unwrap();
        let parts = split_file(&reg, &src_p, 100).await.unwrap();
        assert_eq!(parts.len(), 3);
        let dest = FsPath::from_local(&dir.path().join("out.bin")).unwrap();
        join_files(&reg, &parts, &dest).await.unwrap();
        assert_eq!(std::fs::read(dest.to_local().unwrap()).unwrap(), data);
    }
}
