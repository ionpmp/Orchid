//! OS shell file/folder icons for the file manager.
//!
//! On Windows this prefers `IShellItemImageFactory` (the same path Explorer
//! uses for sharp 48/256px glyphs), falling back to `SHGetImageList` /
//! `SHGetFileInfoW`. Other platforms return `None` and keep the geometric UI
//! fallback.

use std::sync::Arc;

use crate::path::FsPath;

/// Pixel size bucket for a shell icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellIconSize {
    /// 16×16 — list / details rows.
    Small,
    /// 32×32 — medium tiles / fallback when jumbo is unavailable.
    Large,
    /// 48×48 — icons view (DPI-scaled via `SHIL_EXTRALARGE`).
    ExtraLarge,
    /// 256×256 — gallery / large icon tiles (`SHIL_JUMBO`).
    Jumbo,
}

impl ShellIconSize {
    /// Side length in pixels.
    #[must_use]
    pub const fn pixels(self) -> u32 {
        match self {
            Self::Small => 16,
            Self::Large => 32,
            Self::ExtraLarge => 48,
            Self::Jumbo => 256,
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

/// Tight-crop fully transparent padding, then pad to a square.
///
/// Windows jumbo image lists often leave a 32×32 glyph in the corner of a
/// 256×256 canvas when the association has no high-resolution image. Stretching
/// that padded bitmap looks like a tiny icon; stretching the cropped 32×32
/// bitmap looks melted. Callers should display the cropped size without
/// aggressive upscaling.
fn crop_transparent(icon: ShellIcon) -> ShellIcon {
    let w = icon.width as usize;
    let h = icon.height as usize;
    if w == 0 || h == 0 || icon.rgba.len() < w * h * 4 {
        return icon;
    }
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    for y in 0..h {
        for x in 0..w {
            if icon.rgba[(y * w + x) * 4 + 3] > 8 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x {
        return icon;
    }
    min_x = min_x.saturating_sub(1);
    min_y = min_y.saturating_sub(1);
    max_x = (max_x + 1).min(w.saturating_sub(1));
    max_y = (max_y + 1).min(h.saturating_sub(1));
    let cw = max_x - min_x + 1;
    let ch = max_y - min_y + 1;
    if cw >= w.saturating_sub(2) && ch >= h.saturating_sub(2) {
        return icon;
    }
    let side = cw.max(ch);
    let mut out = vec![0u8; side * side * 4];
    let ox = (side - cw) / 2;
    let oy = (side - ch) / 2;
    for row in 0..ch {
        let src = ((min_y + row) * w + min_x) * 4;
        let dst = ((oy + row) * side + ox) * 4;
        out[dst..dst + cw * 4].copy_from_slice(&icon.rgba[src..src + cw * 4]);
    }
    ShellIcon {
        rgba: Arc::new(out),
        width: side as u32,
        height: side as u32,
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
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetDIBits, GetObjectW,
        ReleaseDC, SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HBITMAP,
    };
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
    };
    use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SHGetFileInfoW, SHGetImageList,
        SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SMALLICON, SHGFI_SYSICONINDEX,
        SHGFI_USEFILEATTRIBUTES, SHIL_EXTRALARGE, SHIL_JUMBO, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
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
        // High-res factory icons can carry per-folder overlays; never share one
        // path's bitmap across every directory for those sizes.
        if is_dir {
            return match size {
                ShellIconSize::ExtraLarge | ShellIconSize::Jumbo => {
                    (format!("dir:{}", path.as_str()), size)
                }
                ShellIconSize::Small | ShellIconSize::Large => ("dir".into(), size),
            };
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
        let name = path
            .file_name()
            .unwrap_or(if is_dir { "folder" } else { "file" });
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
        match size {
            ShellIconSize::ExtraLarge | ShellIconSize::Jumbo => {
                // Image-list jumbo entries are often a 32×32 glyph on a 256×256
                // canvas (looks melted when stretched). IShellItemImageFactory
                // returns the same high-res bitmaps Explorer uses.
                extract_via_item_factory(path, size)
                    .or_else(|| extract_via_image_list(path, is_dir, size))
                    .or_else(|| extract_via_file_info(path, is_dir, ShellIconSize::Large))
            }
            ShellIconSize::Small | ShellIconSize::Large => {
                extract_via_file_info(path, is_dir, size)
            }
        }
    }

    fn path_wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    unsafe fn extract_via_item_factory(path: &Path, size: ShellIconSize) -> Option<ShellIcon> {
        if !path.exists() {
            return None;
        }
        let wide = path_wide(path);
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&windows::Win32::System::Com::IBindCtx>)
                .ok()?;
        let px = size.pixels() as i32;
        let hbmp = factory
            .GetImage(
                SIZE { cx: px, cy: px },
                SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
            )
            .ok()?;
        let icon = hbitmap_to_rgba(hbmp);
        let _ = DeleteObject(hbmp.into());
        // Factory icons are already tightly composed; cropping would only
        // discard intentional padding and shrink a sharp bitmap.
        icon
    }

    unsafe fn hbitmap_to_rgba(hbmp: HBITMAP) -> Option<ShellIcon> {
        let mut bmp = BITMAP::default();
        let got = GetObjectW(
            hbmp.into(),
            mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut c_void),
        );
        if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
            return None;
        }
        let width = bmp.bmWidth as u32;
        let height = bmp.bmHeight.unsigned_abs();
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return None;
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bmp.bmWidth,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as _,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let rows = GetDIBits(
            mem_dc,
            hbmp,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &bmi as *const _ as *mut BITMAPINFO,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        if rows == 0 {
            return None;
        }
        Some(ShellIcon {
            rgba: Arc::new(bgra_to_rgba(&pixels)),
            width,
            height,
        })
    }

    unsafe fn extract_via_image_list(
        path: &Path,
        is_dir: bool,
        size: ShellIconSize,
    ) -> Option<ShellIcon> {
        let wide = path_wide(path);
        let exists = path.exists();
        let mut info = SHFILEINFOW::default();
        let cb = mem::size_of::<SHFILEINFOW>() as u32;
        let flags = if exists {
            SHGFI_SYSICONINDEX
        } else {
            SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES
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
        if ok == 0 {
            return None;
        }

        let shil = match size {
            ShellIconSize::ExtraLarge => SHIL_EXTRALARGE,
            ShellIconSize::Jumbo => SHIL_JUMBO,
            _ => return None,
        };
        let list: IImageList = SHGetImageList(shil as i32).ok()?;
        let hicon = list.GetIcon(info.iIcon, ILD_TRANSPARENT.0).ok()?;
        if hicon.is_invalid() {
            return None;
        }
        let mut cx = 0i32;
        let mut cy = 0i32;
        let _ = list.GetIconSize(&mut cx, &mut cy);
        let px = if cx > 0 { cx as u32 } else { size.pixels() };
        let result = render_icon_rgba(hicon, px);
        let _ = DestroyIcon(hicon);
        result.map(super::crop_transparent)
    }

    unsafe fn extract_via_file_info(
        path: &Path,
        is_dir: bool,
        size: ShellIconSize,
    ) -> Option<ShellIcon> {
        let wide = path_wide(path);
        let size_flag = match size {
            ShellIconSize::Small => SHGFI_SMALLICON,
            _ => SHGFI_LARGEICON,
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
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
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
        let dib = match CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(h) => h,
            Err(_) => {
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return None;
            }
        };
        if bits.is_null() {
            let _ = DeleteObject(dib.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }

        let prev = SelectObject(mem_dc, dib.into());
        // Clear to transparent before drawing (some icons lack full alpha).
        {
            let n = (px * px * 4) as usize;
            slice::from_raw_parts_mut(bits.cast::<u8>(), n).fill(0);
        }

        let drew = DrawIconEx(mem_dc, 0, 0, hicon, size, size, 0, None, DI_NORMAL).is_ok();

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
        let _ = DeleteObject(dib.into());
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
            let icon =
                shell_icon(&path, false, ShellIconSize::Large).expect("pdf association icon");
            assert_eq!(icon.width, 32);
            assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        }

        #[test]
        fn extracts_jumbo_system_icon() {
            let windir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
            let explorer = PathBuf::from(windir).join("explorer.exe");
            if !explorer.exists() {
                return;
            }
            let path = FsPath::from_local(&explorer).expect("local path");
            let jumbo = shell_icon(&path, false, ShellIconSize::Jumbo)
                .expect("explorer should have a jumbo shell icon");
            assert!(jumbo.width >= 48, "jumbo cropped to {}px", jumbo.width);
            assert_eq!(jumbo.rgba.len(), (jumbo.width * jumbo.height * 4) as usize);
            assert!(jumbo.rgba.iter().any(|&b| b != 0));
        }
    }
}

#[cfg(test)]
mod crop_tests {
    use super::{crop_transparent, ShellIcon};
    use std::sync::Arc;

    #[test]
    fn crop_strips_transparent_padding() {
        let mut rgba = vec![0u8; 8 * 8 * 4];
        // 2×2 opaque square at (1,1).
        for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            let i = (y * 8 + x) * 4;
            rgba[i] = 255;
            rgba[i + 1] = 0;
            rgba[i + 2] = 0;
            rgba[i + 3] = 255;
        }
        let cropped = crop_transparent(ShellIcon {
            rgba: Arc::new(rgba),
            width: 8,
            height: 8,
        });
        assert_eq!(cropped.width, 4);
        assert_eq!(cropped.height, 4);
        assert_eq!(cropped.rgba.len(), 4 * 4 * 4);
        assert!(cropped.rgba.iter().any(|&b| b != 0));
    }

    #[test]
    fn crop_keeps_full_canvas_when_filled() {
        let rgba = vec![255u8; 4 * 4 * 4];
        let cropped = crop_transparent(ShellIcon {
            rgba: Arc::new(rgba),
            width: 4,
            height: 4,
        });
        assert_eq!(cropped.width, 4);
        assert_eq!(cropped.height, 4);
    }
}
