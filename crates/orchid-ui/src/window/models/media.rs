use orchid_i18n::LocaleManager;
use slint::{Image, SharedString};

use crate::slint_generated::MediaModel;

fn base64_decode(input: &str) -> std::result::Result<Vec<u8>, ()> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        .collect();

    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes.len() % 4 != 0 {
        return Err(());
    }

    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let a = val(chunk[0]).ok_or(())?;
        let b = val(chunk[1]).ok_or(())?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            val(chunk[2]).ok_or(())?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            val(chunk[3]).ok_or(())?
        };

        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
        out.push((n >> 16) as u8);
        if chunk[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if chunk[3] != b'=' {
            out.push(n as u8);
        }
    }

    Ok(out)
}

pub(crate) fn empty_media_model(locale: &LocaleManager) -> MediaModel {
    MediaModel {
        has_session: false,
        empty_state_text: locale.tr("media-loading").into(),
        title: SharedString::new(),
        artist: SharedString::new(),
        album: SharedString::new(),
        source_app: SharedString::new(),
        position: SharedString::new(),
        duration: SharedString::new(),
        progress: 0.0,
        is_playing: false,
        has_thumbnail: false,
        thumbnail: Image::default(),
    }
}

pub(crate) fn build_media_model(p: &orchid_widgets::MediaPlayerPayload, locale: &LocaleManager) -> MediaModel {
    let (has_thumb, thumb_img) = p
        .thumbnail_base64
        .as_ref()
        .and_then(|b64| {
            let bytes = base64_decode(b64).ok()?;
            let dyn_img = image::load_from_memory(&bytes).ok()?;
            let rgba = dyn_img.to_rgba8();
            let (w, h) = rgba.dimensions();
            if w == 0 || h == 0 {
                return None;
            }
            let buf =
                slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h);
            Some((true, Image::from_rgba8(buf)))
        })
        .unwrap_or((false, Image::default()));
    let empty_state_text = if p.is_loading {
        locale.tr("media-loading").into()
    } else if p.is_unsupported {
        locale.tr("media-unsupported").into()
    } else {
        locale.tr("media-no-session").into()
    };
    MediaModel {
        // Keep the empty/loading layout until a real session is ready.
        has_session: p.has_session && !p.is_loading,
        empty_state_text,
        title: p.title.clone().into(),
        artist: p.artist.clone().into(),
        album: p.album.clone().into(),
        source_app: p.source_app.clone().into(),
        position: format_media_duration(p.position_secs).into(),
        duration: format_media_duration(p.duration_secs).into(),
        progress: p.progress_fraction.clamp(0.0, 1.0),
        is_playing: p.is_playing,
        has_thumbnail: has_thumb,
        thumbnail: thumb_img,
    }
}

fn format_media_duration(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_i18n::{LocaleManager, default_language};

    fn test_locale() -> LocaleManager {
        LocaleManager::new(default_language(), None).expect("locale")
    }

    fn sample_media_payload() -> orchid_widgets::MediaPlayerPayload {
        orchid_widgets::MediaPlayerPayload {
            has_session: true,
            title: "t".into(),
            artist: "a".into(),
            album: "al".into(),
            source_app: "app".into(),
            position_secs: 0,
            duration_secs: 60,
            progress_fraction: 0.5,
            is_playing: true,
            thumbnail_base64: None,
            ..Default::default()
        }
    }

    #[test]
    fn media_empty_state_text() {
        let locale = test_locale();
        let loading = build_media_model(
            &orchid_widgets::MediaPlayerPayload {
                is_loading: true,
                ..Default::default()
            },
            &locale,
        );
        assert_eq!(loading.empty_state_text.as_str(), "Loading media…");
        assert!(!loading.has_session);

        let loading_with_session = build_media_model(
            &orchid_widgets::MediaPlayerPayload {
                has_session: true,
                is_loading: true,
                ..Default::default()
            },
            &locale,
        );
        assert!(!loading_with_session.has_session);

        let unsupported = build_media_model(
            &orchid_widgets::MediaPlayerPayload {
                is_unsupported: true,
                ..Default::default()
            },
            &locale,
        );
        assert_eq!(
            unsupported.empty_state_text.as_str(),
            "Media controls are not available on this platform"
        );
    }

    #[test]
    fn media_progress_clamps() {
        let mut p = sample_media_payload();
        p.progress_fraction = 1.5;
        let m = build_media_model(&p, &test_locale());
        assert!(m.progress <= 1.0);

        p.progress_fraction = -0.3;
        let m = build_media_model(&p, &test_locale());
        assert!(m.progress >= 0.0);
    }
}
