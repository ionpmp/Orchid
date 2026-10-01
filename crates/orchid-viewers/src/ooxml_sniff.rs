//! Sniff Office Open XML family from a ZIP prefix (no full archive needed).
//!
//! OOXML packages usually put `[Content_Types].xml` among the first local
//! entries, so a few KiB of head bytes are enough to tell Word / Excel /
//! PowerPoint apart from a generic ZIP.

use std::io::Read;

/// OOXML package family inferred from `[Content_Types].xml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OoxmlFamily {
    /// WordprocessingML (`.docx` / `.docm`).
    Word,
    /// SpreadsheetML (`.xlsx` / `.xlsm`).
    Excel,
    /// PresentationML (`.pptx` / `.pptm`).
    PowerPoint,
}

/// Classify an OOXML ZIP from a leading sample of the file.
///
/// Returns [`None`] when the sample is not a ZIP, `[Content_Types].xml` is
/// missing from the scanned local entries, or the types XML has no known
/// Office Override.
#[must_use]
pub fn sniff_ooxml_family(sample: &[u8]) -> Option<OoxmlFamily> {
    let xml = read_zip_local_entry(sample, "[Content_Types].xml")?;
    classify_content_types(&xml)
}

fn classify_content_types(xml: &[u8]) -> Option<OoxmlFamily> {
    // Content types are ASCII; lowercase once for substring checks.
    let lower = ascii_lowercase(xml);
    // Prefer the most specific Override markers over generic `officedocument`.
    if contains_slice(&lower, b"wordprocessingml") {
        return Some(OoxmlFamily::Word);
    }
    if contains_slice(&lower, b"spreadsheetml") {
        return Some(OoxmlFamily::Excel);
    }
    if contains_slice(&lower, b"presentationml") {
        return Some(OoxmlFamily::PowerPoint);
    }
    None
}

fn contains_slice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn ascii_lowercase(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|b| if b.is_ascii_uppercase() { b + 32 } else { *b })
        .collect()
}

/// Walk ZIP local file headers in `sample` and return the decompressed
/// payload for `wanted_name` when present and fully contained in the sample.
fn read_zip_local_entry(sample: &[u8], wanted_name: &str) -> Option<Vec<u8>> {
    let wanted = wanted_name.as_bytes();
    let mut offset = 0usize;
    // Cap walks so a corrupt stream cannot spin forever in a 4 KiB buffer.
    for _ in 0..64 {
        if offset + 30 > sample.len() {
            return None;
        }
        if &sample[offset..offset + 4] != b"PK\x03\x04" {
            return None;
        }
        let method = u16::from_le_bytes([sample[offset + 8], sample[offset + 9]]);
        let flags = u16::from_le_bytes([sample[offset + 6], sample[offset + 7]]);
        let comp_size = u32::from_le_bytes(sample[offset + 18..offset + 22].try_into().ok()?)
            as usize;
        let name_len = u16::from_le_bytes([sample[offset + 26], sample[offset + 27]]) as usize;
        let extra_len = u16::from_le_bytes([sample[offset + 28], sample[offset + 29]]) as usize;
        let name_start = offset + 30;
        let name_end = name_start.checked_add(name_len)?;
        if name_end > sample.len() {
            return None;
        }
        let name = &sample[name_start..name_end];
        let data_start = name_end.checked_add(extra_len)?;
        // Data descriptor (bit 3): sizes may be zero; skip classification.
        let has_data_descriptor = (flags & 0x0008) != 0;
        if name == wanted {
            if has_data_descriptor && comp_size == 0 {
                return None;
            }
            let data_end = data_start.checked_add(comp_size)?;
            if data_end > sample.len() {
                return None;
            }
            let compressed = &sample[data_start..data_end];
            return decompress_zip_payload(method, compressed);
        }
        if has_data_descriptor && comp_size == 0 {
            // Cannot skip without scanning for the descriptor — stop.
            return None;
        }
        offset = data_start.checked_add(comp_size)?;
    }
    None
}

fn decompress_zip_payload(method: u16, compressed: &[u8]) -> Option<Vec<u8>> {
    match method {
        0 => Some(compressed.to_vec()),
        8 => {
            let mut dec = flate2::read::DeflateDecoder::new(compressed);
            let mut out = Vec::new();
            dec.read_to_end(&mut out).ok()?;
            Some(out)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn zip_with_content_types(xml: &str, method: CompressionMethod) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default().compression_method(method);
            zip.start_file("[Content_Types].xml", opts).unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
            // Pad with another entry so the central directory is after the types blob.
            zip.start_file("dummy.txt", opts).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    const WORD_TYPES: &str = r#"<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

    const EXCEL_TYPES: &str = r#"<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
</Types>"#;

    const PPT_TYPES: &str = r#"<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
</Types>"#;

    #[test]
    fn sniffs_word_stored() {
        let bytes = zip_with_content_types(WORD_TYPES, CompressionMethod::Stored);
        assert_eq!(sniff_ooxml_family(&bytes), Some(OoxmlFamily::Word));
    }

    #[test]
    fn sniffs_excel_deflated() {
        let bytes = zip_with_content_types(EXCEL_TYPES, CompressionMethod::Deflated);
        assert_eq!(sniff_ooxml_family(&bytes), Some(OoxmlFamily::Excel));
    }

    #[test]
    fn sniffs_powerpoint_deflated() {
        let bytes = zip_with_content_types(PPT_TYPES, CompressionMethod::Deflated);
        assert_eq!(sniff_ooxml_family(&bytes), Some(OoxmlFamily::PowerPoint));
    }

    #[test]
    fn plain_zip_is_none() {
        let bytes = zip_with_content_types("<Types/>", CompressionMethod::Stored);
        assert_eq!(sniff_ooxml_family(&bytes), None);
    }

    #[test]
    fn prefix_only_still_works() {
        let bytes = zip_with_content_types(WORD_TYPES, CompressionMethod::Deflated);
        let prefix = &bytes[..bytes.len().min(4096)];
        assert_eq!(sniff_ooxml_family(prefix), Some(OoxmlFamily::Word));
    }
}
