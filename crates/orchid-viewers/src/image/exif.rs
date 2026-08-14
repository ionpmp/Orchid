//! EXIF / TIFF metadata for still images.

use std::io::BufReader;
use std::path::Path;

use crate::error::{Result, ViewerError};

/// Extensions that may carry EXIF.
const EXIF_EXTENSIONS: &[&str] = &["jpg", "jpeg", "tif", "tiff", "webp", "heic", "heif"];

/// Whether `ext` (lowercase, no dot) is an EXIF-capable image.
#[must_use]
pub fn is_exif_extension(ext: &str) -> bool {
    EXIF_EXTENSIONS.contains(&ext)
}

/// Read EXIF fields from a local image file.
///
/// # Errors
///
/// I/O or an unreadable EXIF container.
pub fn read_exif_fields(path: &Path) -> Result<Vec<(String, String)>> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let exif = exif::Reader::new()
        .read_from_container(&mut reader)
        .map_err(|e| ViewerError::Metadata(e.to_string()))?;
    let mut out = Vec::new();
    for field in exif.fields() {
        let tag = field.tag.to_string();
        if tag.starts_with("Unknown") || tag.contains("MakerNote") {
            continue;
        }
        let value = field.display_value().with_unit(&exif).to_string();
        if value.is_empty() {
            continue;
        }
        out.push((tag, value));
    }
    Ok(out)
}

/// Format EXIF as a report body.
///
/// # Errors
///
/// See [`read_exif_fields`].
pub fn format_exif_report(path: &Path) -> Result<String> {
    let fields = read_exif_fields(path)?;
    if fields.is_empty() {
        return Ok(String::new());
    }
    let mut body = String::new();
    for (tag, value) in fields {
        body.push_str(&tag);
        body.push_str(": ");
        body.push_str(&value);
        body.push('\n');
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_without_exif_errors_or_empty() {
        // Minimal SOF-only JPEG (no APP1).
        let jpeg: &[u8] = &[
            0xFF, 0xD8, 0xFF, 0xD9, // SOI + EOI
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jpg");
        std::fs::write(&path, jpeg).unwrap();
        assert!(read_exif_fields(&path).is_err() || read_exif_fields(&path).unwrap().is_empty());
    }
}
