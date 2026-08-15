//! Folder thumbnail strip / grid, preload cache, and contact-sheet write.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use orchid_fs::{FsPath, FsProviderRegistry};
use orchid_viewers::{
    compose_contact_sheet, encode_contact_sheet_png, load_image, rating_from_bytes, ImageThumbItem,
    LoadedImage, Thumbnail, ThumbnailService, ThumbnailSize, DEFAULT_SIZE_LIMIT,
};

use super::image_nav::ImageFolderNav;

/// Contact-sheet file written beside the folder playlist.
pub const CONTACT_SHEET_NAME: &str = "orchid-contact-sheet.png";

/// UI prefs + generated thumbs for one viewer instance.
#[derive(Debug, Clone)]
pub struct ImageThumbState {
    /// 0 hidden, 1 bottom, 2 top.
    pub strip: u8,
    /// Full-folder grid mode.
    pub grid: bool,
    pub size: ThumbnailSize,
    pub show_meta: bool,
    /// How many siblings ahead to decode fully.
    pub preload_n: u8,
    pub items: Vec<ImageThumbItem>,
    /// 0 photo, 1 timeline, 2 map, 3 calendar.
    pub browse: u8,
    /// Hide chrome after idle time.
    pub overlay_autohide: bool,
    pub cal_year: i32,
    pub cal_month: u8,
}

impl Default for ImageThumbState {
    fn default() -> Self {
        Self {
            strip: 1,
            grid: false,
            size: ThumbnailSize::Medium,
            show_meta: true,
            preload_n: 2,
            items: Vec::new(),
            browse: 0,
            overlay_autohide: true,
            cal_year: 0,
            cal_month: 0,
        }
    }
}

impl ImageThumbState {
    /// Cycle strip: hidden → bottom → top → hidden.
    pub fn cycle_strip(&mut self) {
        self.strip = match self.strip {
            0 => 1,
            1 => 2,
            _ => 0,
        };
    }

    pub fn cycle_size(&mut self) {
        self.size = self.size.cycle();
        self.items.clear();
    }

    /// Toggle `mode` on, or return to the single-image view.
    pub fn toggle_browse(&mut self, mode: u8) {
        self.browse = if self.browse == mode { 0 } else { mode };
        if self.browse == 3 && (self.cal_year == 0 || self.cal_month == 0) {
            let date = self
                .items
                .iter()
                .find(|t| t.selected)
                .map(|t| t.date_text.as_str())
                .unwrap_or("");
            let (y, m) = super::image_browse::month_from_date(date);
            self.cal_year = y;
            self.cal_month = m as u8;
        }
    }
}

/// Decoded full images waiting for next/prev.
#[derive(Default)]
pub struct ImagePreloadCache {
    map: HashMap<String, LoadedImage>,
    order: VecDeque<String>,
}

impl ImagePreloadCache {
    pub fn take(&mut self, path: &str) -> Option<LoadedImage> {
        if let Some(img) = self.map.remove(path) {
            self.order.retain(|p| p != path);
            return Some(img);
        }
        None
    }

    pub fn forget(&mut self, path: &str) {
        self.map.remove(path);
        self.order.retain(|p| p != path);
    }

    pub fn contains(&self, path: &str) -> bool {
        self.map.contains_key(path)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    pub fn insert(&mut self, path: String, img: LoadedImage, cap: usize) {
        if self.map.contains_key(&path) {
            self.order.retain(|p| p != &path);
        }
        while self.map.len() >= cap.max(1) {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            } else {
                break;
            }
        }
        self.map.insert(path.clone(), img);
        self.order.push_back(path);
    }
}

/// Paths of the next `n` readable siblings after `nav.index`.
#[must_use]
pub fn preload_paths(nav: &ImageFolderNav, n: usize) -> Vec<FsPath> {
    if n == 0 || nav.siblings.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = nav.index;
    for _ in 0..nav.siblings.len() {
        i = if i + 1 < nav.siblings.len() {
            i + 1
        } else if nav.loop_playlist {
            0
        } else {
            break;
        };
        if i == nav.index {
            break;
        }
        if let Some(p) = nav.siblings.get(i) {
            if !nav.unreadable.contains(p.as_str()) {
                out.push(p.clone());
                if out.len() >= n {
                    break;
                }
            }
        }
    }
    out
}

/// Fetch or generate one thumbnail, using the shared disk cache.
pub async fn load_one_thumb(
    thumbs: &ThumbnailService,
    registry: &FsProviderRegistry,
    path: &FsPath,
    key: [u8; 32],
    size: ThumbnailSize,
) -> Option<Thumbnail> {
    if let Ok(Some(thumb)) = thumbs.get_cached(&key, size).await {
        return Some(thumb);
    }
    if path.scheme() == "local" {
        if let Ok(os_path) = path.to_local() {
            if let Ok(thumb) = thumbs.generate_from_local_path(key, size, os_path).await {
                return Some(thumb);
            }
        }
    }
    let provider = registry.for_path(path)?;
    let bytes = match provider.read(path).await {
        Ok(b) if b.len() <= 16 * 1024 * 1024 => b,
        _ => return None,
    };
    thumbs
        .generate_from_image_bytes(key, size, bytes)
        .await
        .ok()
}

fn rating_from_local(path: &std::path::Path) -> u8 {
    let Ok(mut file) = std::fs::File::open(path) else {
        return 0;
    };
    let mut buf = vec![0u8; 64 * 1024];
    let n = std::io::Read::read(&mut file, &mut buf).unwrap_or(0);
    rating_from_bytes(&buf[..n])
}

/// Size / mtime / rating / EXIF date / GPS for a playlist path.
pub async fn sibling_meta(registry: &FsProviderRegistry, path: &FsPath) -> SiblingBrowseMeta {
    let name = path.file_name().unwrap_or_default().to_string();
    let empty = SiblingBrowseMeta {
        name,
        ..SiblingBrowseMeta::default()
    };
    let provider = match registry.for_path(path) {
        Some(p) => p,
        None => return empty,
    };
    let meta = match provider.metadata(path).await {
        Ok(m) => m,
        Err(_) => return empty,
    };
    let modified_ms = meta.modified.map(|t| t.timestamp_millis()).unwrap_or(0);
    let mut date_text = meta
        .modified
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default();
    let mut taken_ms = modified_ms;
    let mut has_gps = false;
    let mut gps_lat = 0.0;
    let mut gps_lon = 0.0;
    let rating = if path.scheme() == "local" {
        if let Ok(os) = path.to_local() {
            let tags = orchid_viewers::inspect_image_tags(&os);
            if let Some(g) = tags.gps {
                has_gps = true;
                gps_lat = g.lat as f32;
                gps_lon = g.lon as f32;
            }
            for (k, v) in tags.exif.iter().chain(tags.xmp.iter()) {
                if k.eq_ignore_ascii_case("DateTimeOriginal") || k.eq_ignore_ascii_case("DateTime")
                {
                    if let Some((y, m, d)) = super::image_browse::parse_ymd(v) {
                        date_text = format!("{y:04}-{m:02}-{d:02}");
                        taken_ms = super::image_browse::taken_ms_from_date(&date_text);
                    }
                    break;
                }
            }
            rating_from_local(&os)
        } else {
            0
        }
    } else {
        0
    };
    SiblingBrowseMeta {
        size_bytes: meta.size,
        modified_ms,
        date_text,
        rating,
        taken_ms,
        has_gps,
        gps_lat,
        gps_lon,
        name: empty.name,
    }
}

/// Folder-sibling facts used by the strip and browse views.
#[derive(Debug, Clone, Default)]
pub struct SiblingBrowseMeta {
    /// File size.
    pub size_bytes: u64,
    /// Filesystem mtime (cache key).
    pub modified_ms: i64,
    /// `YYYY-MM-DD` (EXIF when present).
    pub date_text: String,
    pub rating: u8,
    pub taken_ms: i64,
    pub has_gps: bool,
    pub gps_lat: f32,
    pub gps_lon: f32,
    pub name: String,
}

/// Decode `path` for the preload cache (best-effort).
pub async fn preload_one(
    registry: Arc<FsProviderRegistry>,
    path: FsPath,
) -> Option<(String, LoadedImage)> {
    let key = path.as_str().to_string();
    match load_image(&path, registry, DEFAULT_SIZE_LIMIT).await {
        Ok(img) => Some((key, img)),
        Err(_) => None,
    }
}

/// Build a PNG contact sheet for `nav` and write it beside the folder.
pub async fn write_contact_sheet(
    thumbs: &ThumbnailService,
    registry: &FsProviderRegistry,
    nav: &ImageFolderNav,
    size: ThumbnailSize,
) -> Result<FsPath, String> {
    let folder = nav
        .folder
        .clone()
        .ok_or_else(|| "no folder playlist".to_string())?;
    let dest = folder.join(CONTACT_SHEET_NAME);
    let mut cells = Vec::new();
    for path in nav.siblings.iter().take(64) {
        let modified_ms = {
            let provider = registry.for_path(path);
            match provider {
                Some(p) => p
                    .metadata(path)
                    .await
                    .ok()
                    .and_then(|m| m.modified)
                    .map(|t| t.timestamp_millis())
                    .unwrap_or(0),
                None => 0,
            }
        };
        let key = ThumbnailService::cache_key(path, modified_ms);
        if let Some(thumb) = load_one_thumb(thumbs, registry, path, key, size).await {
            cells.push(thumb);
        }
    }
    if cells.is_empty() {
        return Err("no thumbnails for contact sheet".into());
    }
    let cols = (cells.len() as f32).sqrt().ceil().max(1.0) as u32;
    let sheet = compose_contact_sheet(&cells, cols, size.to_pixels(), 8);
    let png = encode_contact_sheet_png(&sheet).map_err(|e| e.to_string())?;
    let provider = registry
        .for_path(&dest)
        .ok_or_else(|| "no provider for contact sheet".to_string())?;
    provider
        .write(&dest, &png)
        .await
        .map_err(|e| e.to_string())?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_fs::FsPath;

    fn p(name: &str) -> FsPath {
        FsPath::new(format!("local:c:/pics/{name}")).unwrap()
    }

    #[test]
    fn preload_paths_takes_next_n() {
        let nav = ImageFolderNav {
            folder: Some(FsPath::new("local:c:/pics").unwrap()),
            siblings: vec![p("a.png"), p("b.png"), p("c.png"), p("d.png")],
            index: 0,
            loop_playlist: true,
            ..ImageFolderNav::default()
        };
        let next = preload_paths(&nav, 2);
        assert_eq!(
            next.iter()
                .map(|x| x.file_name().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["b.png", "c.png"]
        );
    }

    #[test]
    fn strip_cycles_three_states() {
        let mut s = ImageThumbState::default();
        assert_eq!(s.strip, 1);
        s.cycle_strip();
        assert_eq!(s.strip, 2);
        s.cycle_strip();
        assert_eq!(s.strip, 0);
        s.cycle_strip();
        assert_eq!(s.strip, 1);
    }
}
