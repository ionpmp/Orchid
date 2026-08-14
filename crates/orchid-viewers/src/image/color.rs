//! ICC / monitor color management for the image viewer.
//!
//! Pixels stay 8-bit RGBA (Slint cannot present an HDR framebuffer). When an
//! embedded profile is present we convert into the current monitor profile
//! (Windows ICM) or sRGB. That is the honest wide-gamut path on an SDR window.

use std::io::Cursor;

/// Result of applying color management to a decoded buffer.
#[derive(Debug, Clone, Default)]
pub struct ColorManageInfo {
    /// Embedded or assumed source profile description.
    pub source_profile: String,
    /// Destination profile description (monitor or sRGB).
    pub dest_profile: String,
    /// `true` when pixels were transformed.
    pub transformed: bool,
}

/// Apply embedded ICC (if any) toward the monitor profile or sRGB.
#[must_use]
pub fn apply_embedded_icc(rgba: &mut [u8], file_bytes: &[u8]) -> ColorManageInfo {
    let Some(src_icc) = extract_icc(file_bytes) else {
        return ColorManageInfo {
            source_profile: "sRGB".into(),
            dest_profile: dest_profile_label(),
            transformed: false,
        };
    };
    let src_name = profile_description(&src_icc).unwrap_or_else(|| "embedded ICC".into());
    let dest_icc = monitor_icc_profile();
    let dest_name = dest_icc
        .as_ref()
        .and_then(|p| profile_description(p))
        .unwrap_or_else(dest_profile_label);
    let mut dest_profile = dest_icc
        .as_deref()
        .and_then(|bytes| qcms::Profile::new_from_slice(bytes, false))
        .unwrap_or_else(qcms::Profile::new_sRGB);
    dest_profile.precache_output_transform();
    let Some(src_profile) = qcms::Profile::new_from_slice(&src_icc, false) else {
        return ColorManageInfo {
            source_profile: src_name,
            dest_profile: dest_name,
            transformed: false,
        };
    };
    let Some(xform) = qcms::Transform::new(
        src_profile.as_ref(),
        dest_profile.as_ref(),
        qcms::DataType::RGBA8,
        qcms::Intent::default(),
    ) else {
        return ColorManageInfo {
            source_profile: src_name,
            dest_profile: dest_name,
            transformed: false,
        };
    };
    xform.apply(rgba);
    ColorManageInfo {
        source_profile: src_name,
        dest_profile: dest_name,
        transformed: true,
    }
}

fn extract_icc(bytes: &[u8]) -> Option<Vec<u8>> {
    let cursor = Cursor::new(bytes);
    let reader = image::ImageReader::new(cursor).with_guessed_format().ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    use image::ImageDecoder;
    decoder.icc_profile().ok().flatten()
}

fn profile_description(icc: &[u8]) -> Option<String> {
    // ICC header desc tag is not always at a fixed offset; use the ASCII
    // copyright/desc scan in the first kilobyte as a best-effort label.
    if icc.len() < 128 {
        return None;
    }
    let text: String = icc
        .iter()
        .take(512)
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|b| *b as char)
        .collect();
    let trimmed = text.trim();
    if trimmed.len() < 4 {
        return None;
    }
    Some(trimmed.chars().take(48).collect())
}

fn dest_profile_label() -> String {
    #[cfg(windows)]
    {
        if monitor_icc_path().is_some() {
            return "monitor ICC".into();
        }
    }
    "sRGB".into()
}

fn monitor_icc_profile() -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        let path = monitor_icc_path()?;
        std::fs::read(path).ok().filter(|b| b.len() > 128)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn monitor_icc_path() -> Option<std::path::PathBuf> {
    use windows::core::{w, PWSTR};
    use windows::Win32::Graphics::Gdi::{CreateDCW, DeleteDC};
    use windows::Win32::UI::ColorSystem::GetICMProfileW;
    unsafe {
        let hdc = CreateDCW(w!("DISPLAY"), w!("DISPLAY"), None, None);
        if hdc.is_invalid() {
            return None;
        }
        let mut chars = 0u32;
        let _ = GetICMProfileW(hdc, &mut chars, None);
        if chars == 0 {
            let _ = DeleteDC(hdc);
            return None;
        }
        let mut buf = vec![0u16; chars as usize];
        let mut len = chars;
        let ok = GetICMProfileW(hdc, &mut len, Some(PWSTR(buf.as_mut_ptr())));
        let _ = DeleteDC(hdc);
        if !ok.as_bool() {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = String::from_utf16_lossy(&buf[..end]);
        if path.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_without_icc_stays_srgb() {
        // 1×1 opaque red PNG.
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE,
            0xD4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let mut px = [255u8, 0, 0, 255];
        let info = apply_embedded_icc(&mut px, png);
        assert!(!info.transformed);
        assert_eq!(info.source_profile, "sRGB");
    }
}
