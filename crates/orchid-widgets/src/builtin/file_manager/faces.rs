//! Face rectangles for the Photos people view.
//!
//! On Windows this calls `Windows.Media.FaceAnalysis.FaceDetector`. It finds
//! faces in a picture. It does not decide who the person is. A file with a
//! face and no `people/` name is tagged `people/unnamed`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Tag applied when a face was found and the file has no person name.
pub const UNNAMED_PERSON_TAG: &str = "people/unnamed";

/// How many images one folder refresh may scan.
pub(crate) const SCAN_WAVE: usize = 24;

const MAX_EDGE: u32 = 640;
const MAX_FILE_BYTES: u64 = 40 * 1024 * 1024;
const STORE_CAP: usize = 4000;
const STORE_VERSION: u32 = 1;

/// What to do with [`UNNAMED_PERSON_TAG`] after a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnnamedTag {
    /// The file has a face and no person name yet.
    Add,
    /// A person name exists, or the latest scan found no face.
    Remove,
    /// The tag already matches the scan.
    Keep,
}

/// A face box as fractions of the image width and height, origin top-left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FaceBox {
    /// Left edge, `0.0..=1.0`.
    pub x: f32,
    /// Top edge, `0.0..=1.0`.
    pub y: f32,
    /// Width, `0.0..=1.0`.
    pub w: f32,
    /// Height, `0.0..=1.0`.
    pub h: f32,
}

/// Why a file was not scanned.
#[derive(Debug)]
pub(crate) enum FaceDetectError {
    /// This build or this Windows edition has no face detector.
    Unsupported,
    /// The file could not be decoded. The caller may retry later.
    Failed(String),
}

/// Decide whether `people/unnamed` should be added or removed.
#[must_use]
pub(crate) fn unnamed_person_tag(tags: &[String], face_count: usize) -> UnnamedTag {
    let has_unnamed = tags.iter().any(|tag| tag == UNNAMED_PERSON_TAG);
    let named = tags
        .iter()
        .any(|tag| tag == "people" || (tag.starts_with("people/") && tag != UNNAMED_PERSON_TAG));
    if face_count == 0 || named {
        if has_unnamed {
            UnnamedTag::Remove
        } else {
            UnnamedTag::Keep
        }
    } else if has_unnamed {
        UnnamedTag::Keep
    } else {
        UnnamedTag::Add
    }
}

/// Nearest-neighbor gray image whose longest side is at most `max_edge`.
#[must_use]
pub(crate) fn gray_max_edge(
    rgba: &[u8],
    width: u32,
    height: u32,
    max_edge: u32,
) -> Option<(Vec<u8>, u32, u32)> {
    if width == 0 || height == 0 || max_edge == 0 {
        return None;
    }
    let need = (width as u64).checked_mul(height as u64)?.checked_mul(4)?;
    if rgba.len() < need as usize {
        return None;
    }
    let long = width.max(height);
    let (dw, dh) = if long <= max_edge {
        (width, height)
    } else {
        let scale = max_edge as f32 / long as f32;
        let dw = ((width as f32) * scale).round().max(1.0) as u32;
        let dh = ((height as f32) * scale).round().max(1.0) as u32;
        (dw.max(1), dh.max(1))
    };
    let mut out = vec![0u8; (dw as usize).saturating_mul(dh as usize)];
    for y in 0..dh {
        let sy = (y as u64 * height as u64) / dh as u64;
        for x in 0..dw {
            let sx = (x as u64 * width as u64) / dw as u64;
            let i = ((sy * width as u64 + sx) * 4) as usize;
            let luma =
                (rgba[i] as u32 * 54 + rgba[i + 1] as u32 * 183 + rgba[i + 2] as u32 * 19) >> 8;
            out[(y * dw + x) as usize] = luma as u8;
        }
    }
    Some((out, dw, dh))
}

/// Map a detector box in bitmap pixels into [`FaceBox`] fractions.
#[must_use]
pub(crate) fn normalize_face(
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    bitmap_w: u32,
    bitmap_h: u32,
) -> Option<FaceBox> {
    if bitmap_w == 0 || bitmap_h == 0 || x >= bitmap_w || y >= bitmap_h {
        return None;
    }
    let w = w.min(bitmap_w - x);
    let h = h.min(bitmap_h - y);
    if w == 0 || h == 0 {
        return None;
    }
    Some(FaceBox {
        x: x as f32 / bitmap_w as f32,
        y: y as f32 / bitmap_h as f32,
        w: w as f32 / bitmap_w as f32,
        h: h as f32 / bitmap_h as f32,
    })
}

/// Modification time in unix seconds, when the clock can represent it.
#[must_use]
pub(crate) fn file_mtime_secs(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    i64::try_from(secs).ok()
}

/// `true` when this build can call the Windows face detector.
#[must_use]
pub(crate) fn face_detection_compiled() -> bool {
    cfg!(windows)
}

/// Detect faces in a local image file.
///
/// An empty `Ok` means the file was readable and contained no face, or it
/// was larger than 40 MiB and was skipped. [`FaceDetectError::Unsupported`]
/// means the detector itself is unavailable.
pub(crate) fn detect_file_faces(path: &Path) -> Result<Vec<FaceBox>, FaceDetectError> {
    let len = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if len > MAX_FILE_BYTES {
        return Ok(Vec::new());
    }
    detect_decoded(path)
}

fn detect_decoded(path: &Path) -> Result<Vec<FaceBox>, FaceDetectError> {
    let image = orchid_viewers::load_image_file(path)
        .map_err(|err| FaceDetectError::Failed(err.to_string()))?;
    let Some((gray, width, height)) =
        gray_max_edge(&image.rgba, image.width, image.height, MAX_EDGE)
    else {
        return Ok(Vec::new());
    };
    detect_gray(&gray, width, height)
}

#[cfg(not(windows))]
fn detect_gray(_gray: &[u8], _width: u32, _height: u32) -> Result<Vec<FaceBox>, FaceDetectError> {
    Err(FaceDetectError::Unsupported)
}

#[cfg(windows)]
fn detect_gray(gray: &[u8], width: u32, height: u32) -> Result<Vec<FaceBox>, FaceDetectError> {
    let detector = shared_detector()?;
    let bitmap = gray_bitmap(gray, width, height).map_err(FaceDetectError::Failed)?;
    let faces = detector
        .DetectFacesAsync(&bitmap)
        .map_err(|err| FaceDetectError::Failed(err.to_string()))?
        .join()
        .map_err(|err| FaceDetectError::Failed(err.to_string()))?;
    let _ = bitmap.Close();
    let count = faces
        .Size()
        .map_err(|err| FaceDetectError::Failed(err.to_string()))?;
    let mut out = Vec::new();
    for index in 0..count {
        let face = faces
            .GetAt(index)
            .map_err(|err| FaceDetectError::Failed(err.to_string()))?;
        let bounds = face
            .FaceBox()
            .map_err(|err| FaceDetectError::Failed(err.to_string()))?;
        if let Some(norm) = normalize_face(
            bounds.X,
            bounds.Y,
            bounds.Width,
            bounds.Height,
            width,
            height,
        ) {
            out.push(norm);
        }
    }
    Ok(out)
}

#[cfg(windows)]
fn shared_detector() -> Result<windows::Media::FaceAnalysis::FaceDetector, FaceDetectError> {
    use std::sync::OnceLock;
    use windows::Media::FaceAnalysis::FaceDetector;

    static CELL: OnceLock<Option<FaceDetector>> = OnceLock::new();
    let slot = CELL.get_or_init(|| {
        match FaceDetector::IsSupported() {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!("Windows face detector is not supported");
                return None;
            }
            Err(err) => {
                tracing::warn!(error = %err, "face detector support check failed");
                return None;
            }
        }
        match FaceDetector::CreateAsync().and_then(|op| op.join()) {
            Ok(detector) => Some(detector),
            Err(err) => {
                tracing::warn!(error = %err, "face detector create failed");
                None
            }
        }
    });
    slot.clone().ok_or(FaceDetectError::Unsupported)
}

#[cfg(windows)]
fn gray_bitmap(
    gray: &[u8],
    width: u32,
    height: u32,
) -> Result<windows::Graphics::Imaging::SoftwareBitmap, String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Storage::Streams::DataWriter;

    let writer = DataWriter::new().map_err(|err| err.to_string())?;
    writer.WriteBytes(gray).map_err(|err| err.to_string())?;
    let buffer = writer.DetachBuffer().map_err(|err| err.to_string())?;
    let created = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Gray8,
        width as i32,
        height as i32,
    );
    if let Ok(bitmap) = created {
        return Ok(bitmap);
    }
    let mut bgra = Vec::with_capacity(gray.len().saturating_mul(4));
    for pixel in gray {
        bgra.extend_from_slice(&[*pixel, *pixel, *pixel, 255]);
    }
    let writer = DataWriter::new().map_err(|err| err.to_string())?;
    writer.WriteBytes(&bgra).map_err(|err| err.to_string())?;
    let buffer = writer.DetachBuffer().map_err(|err| err.to_string())?;
    let bgra_bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        width as i32,
        height as i32,
    )
    .map_err(|err| err.to_string())?;
    SoftwareBitmap::Convert(&bgra_bitmap, BitmapPixelFormat::Gray8)
        .or_else(|_| SoftwareBitmap::Convert(&bgra_bitmap, BitmapPixelFormat::Nv12))
        .map_err(|err| err.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FaceFile {
    mtime_secs: i64,
    faces: Vec<FaceBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FaceDb {
    version: u32,
    files: BTreeMap<String, FaceFile>,
}

impl Default for FaceDb {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            files: BTreeMap::new(),
        }
    }
}

/// On-disk face rectangles (`data/photo-faces.json`).
#[derive(Debug)]
pub struct FaceStore {
    path: PathBuf,
    db: Mutex<FaceDb>,
    scanning: AtomicBool,
    unavailable: AtomicBool,
}

impl FaceStore {
    /// Load an existing file, or start empty when it is missing or unreadable.
    #[must_use]
    pub fn open(path: PathBuf) -> Self {
        let db = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .filter(|db: &FaceDb| db.version == STORE_VERSION)
            .unwrap_or_default();
        Self {
            path,
            db: Mutex::new(db),
            scanning: AtomicBool::new(false),
            unavailable: AtomicBool::new(false),
        }
    }

    /// `true` when a previous scan learned that the detector cannot run.
    #[must_use]
    pub fn detector_unavailable(&self) -> bool {
        self.unavailable.load(Ordering::Acquire)
    }

    /// Remember that this process cannot detect faces.
    pub fn mark_unavailable(&self) {
        self.unavailable.store(true, Ordering::Release);
    }

    /// Begin one scan wave. `false` when a wave is already running.
    pub fn try_begin_scan(&self) -> bool {
        if self.unavailable.load(Ordering::Acquire) {
            return false;
        }
        !self.scanning.swap(true, Ordering::AcqRel)
    }

    /// Allow another scan wave.
    pub fn end_scan(&self) {
        self.scanning.store(false, Ordering::Release);
    }

    /// `true` when `path` was scanned at this modification time.
    #[must_use]
    pub fn is_current(&self, path: &str, mtime_secs: i64) -> bool {
        self.db
            .lock()
            .files
            .get(path)
            .is_some_and(|file| file.mtime_secs == mtime_secs)
    }

    /// How many faces were stored for `path`, if it has been scanned.
    #[must_use]
    pub fn face_count(&self, path: &str) -> Option<usize> {
        self.db.lock().files.get(path).map(|file| file.faces.len())
    }

    /// Replace the stored scan for `path` and write the file.
    pub fn put(&self, path: &str, mtime_secs: i64, faces: Vec<FaceBox>) {
        let mut db = self.db.lock();
        if db.files.len() >= STORE_CAP && !db.files.contains_key(path) {
            if let Some(oldest) = db.files.keys().next().cloned() {
                db.files.remove(&oldest);
            }
        }
        db.files
            .insert(path.to_string(), FaceFile { mtime_secs, faces });
        db.version = STORE_VERSION;
        if let Ok(bytes) = serde_json::to_vec_pretty(&*db) {
            if let Some(parent) = self.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(err) = std::fs::write(&self.path, bytes) {
                tracing::warn!(error = %err, path = %self.path.display(), "photo face store write failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray_keeps_small_images_and_shrinks_wide_ones() {
        let red = [255u8, 0, 0, 255];
        let (gray, w, h) = gray_max_edge(&red, 1, 1, 640).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(gray, vec![((255u32 * 54) >> 8) as u8]);
        let wide = vec![0u8; 4 * 4 * 2];
        let (_gray, w, h) = gray_max_edge(&wide, 4, 2, 2).unwrap();
        assert_eq!((w, h), (2, 1));
    }

    #[test]
    fn normalize_clamps_boxes_inside_the_bitmap() {
        let face = normalize_face(10, 20, 30, 40, 100, 200).unwrap();
        assert!((face.x - 0.1).abs() < 0.001);
        assert!((face.y - 0.1).abs() < 0.001);
        assert!((face.w - 0.3).abs() < 0.001);
        assert!((face.h - 0.2).abs() < 0.001);
        assert!(normalize_face(5, 5, 0, 10, 20, 20).is_none());
    }

    #[test]
    fn unnamed_tag_follows_faces_and_person_names() {
        assert_eq!(unnamed_person_tag(&[], 1), UnnamedTag::Add);
        assert_eq!(
            unnamed_person_tag(&[UNNAMED_PERSON_TAG.into()], 1),
            UnnamedTag::Keep
        );
        assert_eq!(
            unnamed_person_tag(&["people/ada".into()], 2),
            UnnamedTag::Keep
        );
        assert_eq!(
            unnamed_person_tag(&[UNNAMED_PERSON_TAG.into(), "people/ada".into()], 1),
            UnnamedTag::Remove
        );
        assert_eq!(
            unnamed_person_tag(&[UNNAMED_PERSON_TAG.into()], 0),
            UnnamedTag::Remove
        );
        assert_eq!(unnamed_person_tag(&["work".into()], 0), UnnamedTag::Keep);
    }

    #[test]
    fn store_roundtrip_remembers_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo-faces.json");
        let store = FaceStore::open(path.clone());
        assert!(!store.is_current("local:c:/a.jpg", 10));
        store.put(
            "local:c:/a.jpg",
            10,
            vec![FaceBox {
                x: 0.1,
                y: 0.2,
                w: 0.3,
                h: 0.4,
            }],
        );
        let loaded = FaceStore::open(path);
        assert!(loaded.is_current("local:c:/a.jpg", 10));
        assert!(!loaded.is_current("local:c:/a.jpg", 11));
        assert_eq!(loaded.face_count("local:c:/a.jpg"), Some(1));
    }

    #[cfg(windows)]
    #[test]
    fn windows_face_detector_initializes() {
        match shared_detector() {
            Ok(_) | Err(FaceDetectError::Unsupported) => {}
            Err(FaceDetectError::Failed(err)) => panic!("{err}"),
        }
    }
}
