//! Fast thumbnail source: EXIF / embedded JPEG preview + star rating.

use std::io::Cursor;

/// Windows XP / Explorer star rating (1–5).
const EXIF_RATING: u16 = 0x4746;

/// First complete JPEG after the primary SOI, typically the EXIF thumbnail.
///
/// Camera RAW files often embed a larger preview JPEG; those are handled by
/// [`embedded_jpeg_preview`], which also accepts a lone embedded segment.
#[must_use]
pub fn exif_jpeg_thumbnail(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return first_embedded_jpeg(data, 512);
    }
    first_embedded_jpeg(&data[2..], 512)
}

/// Largest JPEG SOI…EOI segment of at least `min_bytes` (preview over thumb).
#[must_use]
pub fn embedded_jpeg_preview(data: &[u8]) -> Option<&[u8]> {
    let mut best: Option<&[u8]> = None;
    let mut i = 0;
    while i + 1 < data.len() {
        if data[i] != 0xFF || data[i + 1] != 0xD8 {
            i += 1;
            continue;
        }
        let start = i;
        i += 2;
        let mut end = None;
        while i + 1 < data.len() {
            if data[i] == 0xFF && data[i + 1] == 0xD9 {
                end = Some(i + 2);
                break;
            }
            i += 1;
        }
        let Some(end) = end else {
            break;
        };
        let slice = &data[start..end];
        if slice.len() >= 512 && best.is_none_or(|b| slice.len() > b.len()) {
            best = Some(slice);
        }
        i = end;
    }
    best
}

fn first_embedded_jpeg(data: &[u8], min_bytes: usize) -> Option<&[u8]> {
    let limit = data.len().min(512 * 1024);
    let mut i = 0;
    while i + 1 < limit {
        if data[i] != 0xFF || data[i + 1] != 0xD8 {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + 2;
        while j + 1 < data.len() {
            if data[j] == 0xFF && data[j + 1] == 0xD9 {
                let slice = &data[start..j + 2];
                if slice.len() >= min_bytes {
                    return Some(slice);
                }
                break;
            }
            j += 1;
        }
        i += 1;
    }
    None
}

/// Star rating 0–5 from EXIF `Rating` (0x4746) when present.
#[must_use]
pub fn rating_from_bytes(bytes: &[u8]) -> u8 {
    let mut reader = Cursor::new(bytes);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) else {
        return 0;
    };
    for field in exif.fields() {
        if field.tag.number() != EXIF_RATING {
            continue;
        }
        return match &field.value {
            exif::Value::Short(v) => v.first().copied().unwrap_or(0).min(5) as u8,
            exif::Value::Long(v) => {
                u8::try_from(v.first().copied().unwrap_or(0).min(5)).unwrap_or(0)
            }
            _ => 0,
        };
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_jpeg() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([10, 20, 30]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    }

    #[test]
    fn finds_jpeg_after_primary_soi() {
        let thumb = tiny_jpeg();
        let mut container = vec![0xFF, 0xD8, 0x00, 0x11, 0x22];
        container.extend_from_slice(&thumb);
        let found = exif_jpeg_thumbnail(&container).unwrap();
        assert_eq!(found, thumb.as_slice());
    }

    #[test]
    fn rating_without_exif_is_zero() {
        assert_eq!(rating_from_bytes(&[0xFF, 0xD8, 0xFF, 0xD9]), 0);
    }
}
