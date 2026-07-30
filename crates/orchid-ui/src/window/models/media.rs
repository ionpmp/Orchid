use std::cell::RefCell;
use std::sync::Arc;

use orchid_i18n::LocaleManager;
use slint::{Image, SharedString};

use crate::slint_generated::MediaModel;

thread_local! {
    /// UI-thread cache of the last decoded media thumbnail by `Arc` pointer
    /// identity so a 500ms poll that reuses the same bytes does not re-decode.
    static THUMB_CACHE: RefCell<Option<(usize, Image)>> = const { RefCell::new(None) };
}

fn thumb_image_cached(bytes: Option<&Arc<[u8]>>) -> (bool, Image) {
    let Some(bytes) = bytes else {
        THUMB_CACHE.with(|c| *c.borrow_mut() = None);
        return (false, Image::default());
    };
    let ptr = Arc::as_ptr(bytes) as *const u8 as usize;
    if let Some(img) = THUMB_CACHE.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|(p, img)| (*p == ptr).then(|| img.clone()))
    }) {
        return (true, img);
    }
    let Some(img) = decode_thumb_image(bytes.as_ref()) else {
        THUMB_CACHE.with(|c| *c.borrow_mut() = None);
        return (false, Image::default());
    };
    THUMB_CACHE.with(|c| *c.borrow_mut() = Some((ptr, img.clone())));
    (true, img)
}

fn decode_thumb_image(bytes: &[u8]) -> Option<Image> {
    let dyn_img = image::load_from_memory(bytes).ok()?;
    let rgba = dyn_img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h);
    Some(Image::from_rgba8(buf))
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

pub(crate) fn build_media_model(
    p: &orchid_widgets::MediaPlayerPayload,
    locale: &LocaleManager,
) -> MediaModel {
    let (has_thumb, thumb_img) = thumb_image_cached(p.thumbnail_bytes.as_ref());
    fill_media_model(p, locale, has_thumb, thumb_img)
}

/// Patch an existing [`MediaModel`] in place (no nested list models to preserve).
pub(crate) fn patch_media_model(
    model: &mut MediaModel,
    p: &orchid_widgets::MediaPlayerPayload,
    locale: &LocaleManager,
) {
    let (has_thumb, thumb_img) = thumb_image_cached(p.thumbnail_bytes.as_ref());
    *model = fill_media_model(p, locale, has_thumb, thumb_img);
}

fn fill_media_model(
    p: &orchid_widgets::MediaPlayerPayload,
    locale: &LocaleManager,
    has_thumb: bool,
    thumb_img: Image,
) -> MediaModel {
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
    use orchid_i18n::{default_language, LocaleManager};

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
            thumbnail_bytes: None,
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
