//! Base64 and uuencode encode / decode of files.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::error::{FsError, Result};
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// Encode `src` as standard Base64 into `dest` (76-column wrapped).
///
/// # Errors
///
/// Propagates provider / I/O errors.
pub async fn encode_base64(
    registry: &FsProviderRegistry,
    src: &FsPath,
    dest: &FsPath,
) -> Result<()> {
    let provider = registry
        .for_path(src)
        .ok_or_else(|| FsError::ProviderNotMounted(src.to_string()))?;
    let bytes = provider.read(src).await?;
    let b64 = STANDARD.encode(&bytes);
    let wrapped = wrap76(&b64);
    let dest_p = registry
        .for_path(dest)
        .ok_or_else(|| FsError::ProviderNotMounted(dest.to_string()))?;
    dest_p.write(dest, wrapped.as_bytes()).await
}

/// Decode a Base64 file into `dest`.
///
/// # Errors
///
/// Invalid Base64 or I/O errors.
pub async fn decode_base64(
    registry: &FsProviderRegistry,
    src: &FsPath,
    dest: &FsPath,
) -> Result<()> {
    let provider = registry
        .for_path(src)
        .ok_or_else(|| FsError::ProviderNotMounted(src.to_string()))?;
    let text = String::from_utf8_lossy(&provider.read(src).await?).replace(['\r', '\n', ' '], "");
    let bytes = STANDARD
        .decode(text.as_bytes())
        .map_err(|e| FsError::InvalidPath {
            reason: format!("invalid base64: {e}"),
        })?;
    let dest_p = registry
        .for_path(dest)
        .ok_or_else(|| FsError::ProviderNotMounted(dest.to_string()))?;
    dest_p.write(dest, &bytes).await
}

/// Classic uuencode (`begin 644 name` … `` ` `` / `end`).
///
/// # Errors
///
/// Propagates provider / I/O errors.
pub async fn encode_uue(registry: &FsProviderRegistry, src: &FsPath, dest: &FsPath) -> Result<()> {
    let provider = registry
        .for_path(src)
        .ok_or_else(|| FsError::ProviderNotMounted(src.to_string()))?;
    let bytes = provider.read(src).await?;
    let name = src.file_name().unwrap_or("file").to_string();
    let body = uuencode(&bytes, &name);
    let dest_p = registry
        .for_path(dest)
        .ok_or_else(|| FsError::ProviderNotMounted(dest.to_string()))?;
    dest_p.write(dest, body.as_bytes()).await
}

/// Decode a uuencoded file.
///
/// # Errors
///
/// Malformed uuencode or I/O errors.
pub async fn decode_uue(registry: &FsProviderRegistry, src: &FsPath, dest: &FsPath) -> Result<()> {
    let provider = registry
        .for_path(src)
        .ok_or_else(|| FsError::ProviderNotMounted(src.to_string()))?;
    let text = String::from_utf8_lossy(&provider.read(src).await?).into_owned();
    let bytes = uudecode(&text)?;
    let dest_p = registry
        .for_path(dest)
        .ok_or_else(|| FsError::ProviderNotMounted(dest.to_string()))?;
    dest_p.write(dest, &bytes).await
}

/// Suggested destination next to `src` with `ext` (e.g. `"b64"`).
#[must_use]
pub fn sidecar_path(src: &FsPath, ext: &str) -> FsPath {
    let name = src.file_name().unwrap_or("file");
    src.parent()
        .map(|p| p.join(&format!("{name}.{ext}")))
        .unwrap_or_else(|| src.join(&format!("out.{ext}")))
}

/// Strip a known encode extension (`.b64`, `.base64`, `.uue`, `.uu`).
#[must_use]
pub fn decoded_path(src: &FsPath) -> FsPath {
    let name = src.file_name().unwrap_or("file");
    let stripped = name
        .strip_suffix(".b64")
        .or_else(|| name.strip_suffix(".base64"))
        .or_else(|| name.strip_suffix(".uue"))
        .or_else(|| name.strip_suffix(".uu"))
        .unwrap_or(name);
    let out = if stripped == name {
        format!("{name}.decoded")
    } else {
        stripped.to_string()
    };
    src.parent()
        .map(|p| p.join(&out))
        .unwrap_or_else(|| src.join(&out))
}

fn wrap76(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 76 + 1);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && i % 76 == 0 {
            out.push('\n');
        }
        out.push(c);
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn uuencode(data: &[u8], name: &str) -> String {
    let mut out = format!("begin 644 {name}\n");
    for chunk in data.chunks(45) {
        out.push(uue_len(chunk.len()));
        let mut i = 0;
        while i < chunk.len() {
            let a = chunk[i];
            let b = chunk.get(i + 1).copied().unwrap_or(0);
            let c = chunk.get(i + 2).copied().unwrap_or(0);
            let n = ((a as u32) << 16) | ((b as u32) << 8) | c as u32;
            out.push(uue_byte(((n >> 18) & 0x3f) as u8));
            out.push(uue_byte(((n >> 12) & 0x3f) as u8));
            out.push(uue_byte(((n >> 6) & 0x3f) as u8));
            out.push(uue_byte((n & 0x3f) as u8));
            i += 3;
        }
        out.push('\n');
    }
    out.push_str("`\nend\n");
    out
}

fn uudecode(text: &str) -> Result<Vec<u8>> {
    let mut lines = text.lines();
    let begin = lines
        .find(|l| l.trim_start().starts_with("begin "))
        .ok_or_else(|| FsError::InvalidPath {
            reason: "missing uuencode begin line".into(),
        })?;
    let _ = begin;
    let mut out = Vec::new();
    for line in lines {
        let t = line.trim_end_matches(['\r', '\n']);
        if t == "end" || t.starts_with("end") {
            break;
        }
        if t.is_empty() {
            continue;
        }
        let bytes = t.as_bytes();
        let len = uue_decode_len(bytes[0]);
        if len == 0 {
            continue;
        }
        let mut i = 1usize;
        let mut remaining = len;
        while remaining > 0 && i + 3 < bytes.len() {
            let n = ((uue_val(bytes[i]) as u32) << 18)
                | ((uue_val(bytes[i + 1]) as u32) << 12)
                | ((uue_val(bytes[i + 2]) as u32) << 6)
                | uue_val(bytes[i + 3]) as u32;
            if remaining > 0 {
                out.push((n >> 16) as u8);
                remaining -= 1;
            }
            if remaining > 0 {
                out.push((n >> 8) as u8);
                remaining -= 1;
            }
            if remaining > 0 {
                out.push(n as u8);
                remaining -= 1;
            }
            i += 4;
        }
    }
    Ok(out)
}

fn uue_len(n: usize) -> char {
    char::from(32 + n as u8)
}

fn uue_byte(n: u8) -> char {
    char::from(32 + (n & 0x3f))
}

fn uue_decode_len(c: u8) -> usize {
    ((c.wrapping_sub(32)) & 0x3f) as usize
}

fn uue_val(c: u8) -> u8 {
    (c.wrapping_sub(32)) & 0x3f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{FsProviderRegistry, LocalProvider};
    use std::sync::Arc;

    #[tokio::test]
    async fn base64_and_uue_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("x.bin");
        std::fs::write(&src, b"hello orchid").unwrap();
        let reg = FsProviderRegistry::new();
        reg.register(Arc::new(LocalProvider::new())).unwrap();
        let src_p = FsPath::from_local(&src).unwrap();
        let b64 = FsPath::from_local(&dir.path().join("x.b64")).unwrap();
        encode_base64(&reg, &src_p, &b64).await.unwrap();
        let out = FsPath::from_local(&dir.path().join("x.out")).unwrap();
        decode_base64(&reg, &b64, &out).await.unwrap();
        assert_eq!(
            std::fs::read(out.to_local().unwrap()).unwrap(),
            b"hello orchid"
        );

        let uue = FsPath::from_local(&dir.path().join("x.uue")).unwrap();
        encode_uue(&reg, &src_p, &uue).await.unwrap();
        let out2 = FsPath::from_local(&dir.path().join("x2.out")).unwrap();
        decode_uue(&reg, &uue, &out2).await.unwrap();
        assert_eq!(
            std::fs::read(out2.to_local().unwrap()).unwrap(),
            b"hello orchid"
        );
    }
}
