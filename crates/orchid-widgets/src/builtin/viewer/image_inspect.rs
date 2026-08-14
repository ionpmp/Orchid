//! In-viewer metadata panel: EXIF/IPTC/XMP, histogram, and cursor probe.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use orchid_fs::FsPath;
use orchid_viewers::{
    compute_histogram, describe_pixel, format_inspect_panel, inspect_image_file, render_histogram,
    ChannelHistogram, HistMode, ImageInspect, ImageSnapshot,
};

const HIST_W: u32 = 256;
const HIST_H: u32 = 72;
const PROBE_MIN_MS: u64 = 120;

/// Session inspect UI (pixels stay on the image snapshot).
#[derive(Debug, Clone)]
pub struct InspectState {
    pub panel: bool,
    pub overlay: bool,
    pub hist_mode: HistMode,
    pub inspect: Option<ImageInspect>,
    pub report: String,
    pub overlay_text: String,
    pub hist: Option<ChannelHistogram>,
    pub hist_rgba: Option<Arc<Vec<u8>>>,
    pub probe: String,
    pub path: String,
    last_probe_ms: u64,
}

impl Default for InspectState {
    fn default() -> Self {
        Self {
            panel: false,
            overlay: false,
            hist_mode: HistMode::Luma,
            inspect: None,
            report: String::new(),
            overlay_text: String::new(),
            hist: None,
            hist_rgba: None,
            probe: String::new(),
            path: String::new(),
            last_probe_ms: 0,
        }
    }
}

impl InspectState {
    pub fn apply_inspect(
        &mut self,
        path: &str,
        inspect: ImageInspect,
        snap: Option<&ImageSnapshot>,
    ) {
        self.path = path.to_string();
        self.overlay_text = inspect.overlay.clone();
        if let Some(s) = snap {
            self.report = format_inspect_panel(
                s.width_px,
                s.height_px,
                s.size_bytes,
                &s.format_label,
                s.bit_depth,
                &s.color_model,
                &s.color_source,
                &s.color_dest,
                &inspect,
            );
        } else {
            self.report =
                format_inspect_panel(0, 0, 0, "", 8, "", &inspect.icc_label, "", &inspect);
        }
        self.inspect = Some(inspect);
    }

    pub fn set_histogram(&mut self, hist: ChannelHistogram) {
        self.hist_rgba = Some(Arc::new(render_histogram(
            &hist,
            self.hist_mode,
            HIST_W,
            HIST_H,
        )));
        self.hist = Some(hist);
    }

    pub fn cycle_hist_mode(&mut self) {
        self.hist_mode = self.hist_mode.cycle();
        if let Some(h) = self.hist.as_ref() {
            self.hist_rgba = Some(Arc::new(render_histogram(
                h,
                self.hist_mode,
                HIST_W,
                HIST_H,
            )));
        }
    }

    pub fn probe_pixel(&mut self, snap: &ImageSnapshot, x: i32, y: i32) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if now.saturating_sub(self.last_probe_ms) < PROBE_MIN_MS {
            return false;
        }
        self.last_probe_ms = now;
        let next = describe_pixel(&snap.rgba_bytes, snap.width_px, snap.height_px, x, y);
        if next == self.probe {
            return false;
        }
        self.probe = next;
        true
    }

    pub fn gps_url(&self) -> Option<String> {
        self.inspect.as_ref()?.gps.map(|g| g.map_url())
    }
}

/// Blocking file inspect for a local path.
pub fn inspect_local(path: &FsPath) -> Result<ImageInspect, String> {
    let os = path.to_local().map_err(|e| e.to_string())?;
    inspect_image_file(&os).map_err(|e| e.to_string())
}

/// Histogram from the current decoded snapshot.
pub fn histogram_from_snap(snap: &ImageSnapshot) -> ChannelHistogram {
    compute_histogram(&snap.rgba_bytes, snap.width_px, snap.height_px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hist_mode_persists_through_cycle() {
        let mut s = InspectState::default();
        s.set_histogram(ChannelHistogram::default());
        s.cycle_hist_mode();
        assert_eq!(s.hist_mode, HistMode::Rgb);
        assert!(s.hist_rgba.is_some());
    }
}
