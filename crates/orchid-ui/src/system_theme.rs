//! OS light/dark preference and theme resolution for [`AppearanceConfig`].

use orchid_storage::AppearanceConfig;

/// Resolve the theme id to apply from appearance settings.
///
/// When `follow_system_theme` is enabled on Windows, reads the user's
/// `AppsUseLightTheme` registry value and picks `dark_theme` or `light_theme`.
/// On other platforms (or when the registry value is unavailable), falls back to
/// [`AppearanceConfig::theme`].
#[must_use]
pub fn resolve_theme_id(cfg: &AppearanceConfig) -> String {
    if cfg.follow_system_theme {
        #[cfg(windows)]
        if let Some(dark) = windows_prefers_dark_apps() {
            return if dark {
                cfg.dark_theme.clone()
            } else {
                cfg.light_theme.clone()
            };
        }
    }
    cfg.theme.clone()
}

/// Returns `Some(true)` when Windows reports a dark app theme, `Some(false)` for
/// light, or `None` if the preference could not be read.
#[cfg(windows)]
fn windows_prefers_dark_apps() -> Option<bool> {
    use std::mem::MaybeUninit;

    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegGetValueW, RRF_RT_REG_DWORD, HKEY_CURRENT_USER,
    };

    let mut data = MaybeUninit::<u32>::uninit();
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    // 0 = dark, 1 = light
    Some(unsafe { data.assume_init() } == 0)
}
