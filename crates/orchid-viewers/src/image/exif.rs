//! EXIF / TIFF metadata for still images.

use std::io::{BufReader, Cursor};
use std::path::Path;

use image::DynamicImage;

use crate::error::{Result, ViewerError};

/// EXIF orientation tag (1–8). `1` is identity.
#[must_use]
pub fn orientation_from_bytes(bytes: &[u8]) -> u32 {
    let mut reader = Cursor::new(bytes);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) else {
        return 1;
    };
    let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) else {
        return 1;
    };
    match field.value {
        exif::Value::Short(ref v) => u32::from(v.first().copied().unwrap_or(1)),
        _ => 1,
    }
}

/// Apply EXIF orientation so the pixel buffer matches “how it should look”.
#[must_use]
pub fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    use image::imageops::{flip_horizontal, flip_vertical, rotate180, rotate270, rotate90};
    match orientation {
        2 => DynamicImage::ImageRgba8(flip_horizontal(&img)),
        3 => DynamicImage::ImageRgba8(rotate180(&img)),
        4 => DynamicImage::ImageRgba8(flip_vertical(&img)),
        5 => {
            let rotated = rotate90(&img);
            DynamicImage::ImageRgba8(flip_horizontal(&rotated))
        }
        6 => DynamicImage::ImageRgba8(rotate90(&img)),
        7 => {
            let rotated = rotate90(&img);
            DynamicImage::ImageRgba8(flip_vertical(&rotated))
        }
        8 => DynamicImage::ImageRgba8(rotate270(&img)),
        _ => img,
    }
}

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
    use image::GenericImageView;

    #[test]
    fn orientation_6_rotates_to_upright() {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(2, 1, |x, _y| {
            if x == 0 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 255, 0, 255])
            }
        }));
        let out = apply_orientation(img, 6);
        assert_eq!(out.width(), 1);
        assert_eq!(out.height(), 2);
        assert_eq!(out.get_pixel(0, 0), image::Rgba([255, 0, 0, 255]));
        assert_eq!(out.get_pixel(0, 1), image::Rgba([0, 255, 0, 255]));
    }

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
