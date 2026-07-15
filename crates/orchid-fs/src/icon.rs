//! OS shell file/folder icons for the file manager.
//!
//! On Windows this extracts association icons via `SHGetFileInfoW` and renders
//! them to RGBA. Other platforms return `None` and keep the geometric UI fallback.

use std::sync::Arc;

use crate::path::FsPath;

/// Pixel size bucket for a shell icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellIconSize {
    /// 16×16 — list / details rows.
    Small,
    /// 32×32 — icons / gallery tiles without image previews.
    Large,
}

impl ShellIconSize {
    /// Side length in pixels.
    #[must_use]
    pub const fn pixels(self) -> u32 {
        match self {
            Self::Small => 16,
            Self::Large => 32,
        }
    }
}

/// Decoded shell icon ready for the UI (`RGBA8`, top-down).
#[derive(Debug, Clone)]
pub struct ShellIcon {
    /// Tight RGBA buffer (`width * height * 4`).
    pub rgba: Arc<Vec<u8>>,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
}

/// Fetch the OS association icon for `path`.
///
/// Local paths use the real filesystem entry when it exists; otherwise (and for
/// non-local schemes) the file name / extension is used with
/// `SHGFI_USEFILEATTRIBUTES` so virtual and remote listings still get type icons.
///
/// Returns `None` when extraction is unsupported or fails — callers should keep
/// their geometric fallback.
#[must_use]
pub fn shell_icon(path: &FsPath, is_dir: bool, size: ShellIconSize) -> Option<ShellIcon> {
    #[cfg(windows)]
    {
        windows_impl::shell_icon(path, is_dir, size)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, is_dir, size);
        None
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use std::mem;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::slice;
    use std::sync::{Arc, Mutex, OnceLock};

    use dashmap::DashMap;
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBRUSH,
    };
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
    };
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SMALLICON,
        SHGFI_USEFILEATTRIBUTES,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL, HICON};

    use super::{ShellIcon, ShellIconSize};
    use crate::path::FsPath;

    /// Shell icon extraction is not reliably thread-safe across shell extensions.
    static SHELL_LOCK: Mutex<()> = Mutex::new(());

    type CacheKey = (String, ShellIconSize);

    fn cache() -> &'static DashMap<CacheKey, ShellIcon> {
        static CACHE: OnceLock<DashMap<CacheKey, ShellIcon>> = OnceLock::new();
        CACHE.get_or_init(DashMap::new)
    }

    pub(super) fn shell_icon(
        path: &FsPath,
        is_dir: bool,
        size: ShellIconSize,
    ) -> Option<ShellIcon> {
        let key = cache_key(path, is_dir, size);
        if let Some(hit) = cache().get(&key) {
            return Some(hit.clone());
        }

        let lookup = lookup_path(path, is_dir)?;
        let icon = extract_locked(&lookup, is_dir, size)?;
        cache().insert(key, icon.clone());
        Some(icon)
    }

    fn cache_key(path: &FsPath, is_dir: bool, size: ShellIconSize) -> CacheKey {
        if is_dir {
            return ("dir".into(), size);
        }
        let ext = path.extension().unwrap_or("").to_ascii_lowercase();
        let key = match ext.as_str() {
            "exe" | "lnk" | "dll" | "ico" | "cur" | "scr" | "msi" | "cpl" | "ocx" => {
                format!("path:{}", path.as_str())
            }
            "" => format!("name:{}", path.file_name().unwrap_or("file")),
            other => format!("ext:{other}"),
        };
        (key, size)
    }

    fn lookup_path(path: &FsPath, is_dir: bool) -> Option<PathBuf> {
        if path.is_local() {
            if let Ok(os) = path.to_local() {
                if os.exists() {
                    // SHGetFileInfoW is more reliable with native separators.
                    return Some(normalize_os_path(os));
                }
            }
        }
        // Association-by-name for missing / remote / virtual entries.
        let name = path.file_name().unwrap_or(if is_dir { "folder" } else { "file" });
        Some(PathBuf::from(name))
    }

    fn normalize_os_path(path: PathBuf) -> PathBuf {
        PathBuf::from(path.to_string_lossy().replace('/', "\\"))
    }

    fn extract_locked(path: &Path, is_dir: bool, size: ShellIconSize) -> Option<ShellIcon> {
        let _guard = SHELL_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: Win32 shell + GDI calls under process-wide serialisation.
        unsafe { extract_icon(path, is_dir, size) }
    }

    unsafe fn extract_icon(path: &Path, is_dir: bool, size: ShellIconSize) -> Option<ShellIcon> {
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let size_flag = match size {
            ShellIconSize::Small => SHGFI_SMALLICON,
            ShellIconSize::Large => SHGFI_LARGEICON,
        };

        let exists = path.exists();
        let mut info = SHFILEINFOW::default();
        let cb = mem::size_of::<SHFILEINFOW>() as u32;

        let flags = if exists {
            SHGFI_ICON | size_flag
        } else {
            SHGFI_ICON | size_flag | SHGFI_USEFILEATTRIBUTES
        };
        let attrs = if is_dir {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        };

        let ok = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            if exists {
                FILE_FLAGS_AND_ATTRIBUTES(0)
            } else {
                attrs
            },
            Some(&mut info),
            cb,
            flags,
        );
        if ok == 0 || info.hIcon.is_invalid() {
            return None;
        }

        let hicon = info.hIcon;
        let result = render_icon_rgba(hicon, size.pixels());
        let _ = DestroyIcon(hicon);
        result
    }

    unsafe fn render_icon_rgba(hicon: HICON, px: u32) -> Option<ShellIcon> {
        let size = px as i32;
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return None;
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as _,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut c_void = std::ptr::null_mut();
        let dib = match CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(h) => h,
            Err(_) => {
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return None;
            }
        };
        if bits.is_null() {
            let _ = DeleteObject(dib);
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }

        let prev = SelectObject(mem_dc, dib);
        // Clear to transparent before drawing (some icons lack full alpha).
        {
            let n = (px * px * 4) as usize;
            slice::from_raw_parts_mut(bits.cast::<u8>(), n).fill(0);
        }

        let drew = DrawIconEx(
            mem_dc,
            0,
            0,
            hicon,
            size,
            size,
            0,
            HBRUSH::default(),
            DI_NORMAL,
        )
        .is_ok();

        let icon = if drew {
            let n = (px * px) as usize;
            let bgra = slice::from_raw_parts(bits.cast::<u8>(), n * 4);
            Some(ShellIcon {
                rgba: Arc::new(bgra_to_rgba(bgra)),
                width: px,
                height: px,
            })
        } else {
            None
        };

        let _ = SelectObject(mem_dc, prev);
        let _ = DeleteObject(dib);
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        icon
    }

    /// Convert BGRA DIB pixels to RGBA, repairing missing alpha from mask draws.
    fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(bgra.len());
        for chunk in bgra.chunks_exact(4) {
            let (b, g, r, mut a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
            if a == 0 && (r != 0 || g != 0 || b != 0) {
                a = 255;
            }
            rgba.extend_from_slice(&[r, g, b, a]);
        }
        rgba
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::path::FsPath;

        #[test]
        fn extracts_exe_icon() {
            let windir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
            let notepad = PathBuf::from(windir).join("System32").join("notepad.exe");
            if !notepad.exists() {
                return;
            }
            let path = FsPath::from_local(&notepad).expect("local path");
            let icon = shell_icon(&path, false, ShellIconSize::Small)
                .expect("notepad should have a shell icon");
            assert_eq!(icon.width, 16);
            assert_eq!(icon.height, 16);
            assert_eq!(icon.rgba.len(), 16 * 16 * 4);
            // At least some non-transparent / non-zero pixels.
            assert!(icon.rgba.iter().any(|&b| b != 0));
        }

        #[test]
        fn extracts_extension_icon_without_path() {
            let path = FsPath::new("local:c:/does-not-exist/report.pdf").expect("path");
            let icon = shell_icon(&path, false, ShellIconSize::Large)
                .expect("pdf association icon");
            assert_eq!(icon.width, 32);
            assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        }
    }
}
