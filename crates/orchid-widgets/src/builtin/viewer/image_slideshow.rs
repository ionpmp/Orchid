//! In-viewer slideshow: timer, shuffle, music, and export wrappers.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use orchid_fs::{FsPath, FsProviderRegistry};
use orchid_viewers::{
    export_slideshow_pack, export_slideshow_video, is_slideshow_audio_extension, overlay_text,
    SlideTransition, SlideshowExport,
};

use super::image_nav::ImageFolderNav;

/// Session slideshow controls (not the decoded pixels).
#[derive(Debug, Clone)]
pub struct SlideshowState {
    pub playing: bool,
    pub paused: bool,
    pub interval_ms: u32,
    pub random: bool,
    pub transition: SlideTransition,
    pub transition_ms: u32,
    pub overlay: bool,
    pub music_path: Option<String>,
    pub overlay_text: String,
    pub gen: u32,
    pub prev_rgba: Option<Arc<Vec<u8>>>,
    pub prev_w: u32,
    pub prev_h: u32,
    pub shuffled: Vec<usize>,
    pub shuffle_at: usize,
    /// Milliseconds accumulated in the current slide (not persisted).
    pub elapsed_ms: u32,
}

impl Default for SlideshowState {
    fn default() -> Self {
        Self {
            playing: false,
            paused: false,
            interval_ms: 4000,
            random: false,
            transition: SlideTransition::Fade,
            transition_ms: 500,
            overlay: true,
            music_path: None,
            overlay_text: String::new(),
            gen: 0,
            prev_rgba: None,
            prev_w: 0,
            prev_h: 0,
            shuffled: Vec::new(),
            shuffle_at: 0,
            elapsed_ms: 0,
        }
    }
}

/// How long the slideshow task should sleep before the next useful wake.
///
/// During a transition this is 50 ms (Slint drives `slide-t` from `gen * 50`).
/// Between slides it is the remaining interval. Pause polls every 200 ms.
#[must_use]
pub fn slideshow_wait_ms(
    paused: bool,
    elapsed_ms: u32,
    interval_ms: u32,
    trans_ms: u32,
    slide_gen: u32,
) -> u32 {
    if paused {
        return 200;
    }
    let interval = interval_ms.max(400);
    let remain_slide = interval.saturating_sub(elapsed_ms).max(1);
    if slide_gen.saturating_mul(50) >= trans_ms.max(1) {
        remain_slide
    } else {
        50.min(remain_slide)
    }
}

impl SlideshowState {
    pub fn cycle_interval(&mut self, faster: bool) {
        let step = 1000u32;
        self.interval_ms = if faster {
            self.interval_ms.saturating_sub(step).max(1000)
        } else {
            (self.interval_ms + step).min(30_000)
        };
    }

    pub fn cycle_transition_ms(&mut self) {
        self.transition_ms = match self.transition_ms {
            0..=249 => 250,
            250..=499 => 500,
            500..=749 => 750,
            750..=999 => 1000,
            _ => 250,
        };
    }

    pub fn rebuild_shuffle(&mut self, nav: &ImageFolderNav) {
        let mut idx: Vec<usize> = (0..nav.siblings.len())
            .filter(|i| {
                nav.siblings
                    .get(*i)
                    .is_some_and(|p| !nav.unreadable.contains(p.as_str()))
            })
            .collect();
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        for i in (1..idx.len()).rev() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let j = (seed as usize) % (i + 1);
            idx.swap(i, j);
        }
        self.shuffle_at = idx.iter().position(|i| *i == nav.index).unwrap_or(0);
        self.shuffled = idx;
    }

    pub fn next_shuffled(&mut self, nav: &ImageFolderNav) -> Option<usize> {
        if self.shuffled.is_empty() {
            self.rebuild_shuffle(nav);
        }
        if self.shuffled.is_empty() {
            return None;
        }
        let n = self.shuffled.len();
        let next_at = self.shuffle_at + 1;
        if next_at >= n {
            if !nav.loop_playlist {
                return None;
            }
            let last = self.shuffled.get(self.shuffle_at).copied();
            self.rebuild_shuffle(nav);
            self.shuffle_at = 0;
            if n > 1 && self.shuffled.first().copied() == last {
                self.shuffle_at = 1;
            }
        } else {
            self.shuffle_at = next_at;
        }
        self.shuffled.get(self.shuffle_at).copied()
    }
}

/// Kill a music child started by [`start_music`].
pub fn stop_music(child: &mut Option<Child>) {
    if let Some(mut c) = child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Best-effort background loop of `path` (ffplay, then Windows MediaPlayer).
pub fn start_music(path: &str, child: &mut Option<Child>) {
    stop_music(child);
    let os = match FsPath::new(path).ok().and_then(|p| p.to_local().ok()) {
        Some(p) => p,
        None => PathBuf::from(path),
    };
    if !os.is_file() {
        return;
    }
    if let Ok(c) = Command::new("ffplay")
        .args(["-nodisp", "-loop", "0", "-loglevel", "quiet"])
        .arg(&os)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        *child = Some(c);
        return;
    }
    #[cfg(windows)]
    {
        let uri = os.display().to_string().replace('\\', "/");
        let ps = format!(
            "Add-Type -AssemblyName presentationCore; \
             $p=New-Object System.Windows.Media.MediaPlayer; \
             $p.Open([uri]'file:///{uri}'); $p.Play(); \
             while($true){{ Start-Sleep -Milliseconds 400; \
             if($p.NaturalDuration.HasTimeSpan -and $p.Position -ge $p.NaturalDuration.TimeSpan){{ \
             $p.Position=[TimeSpan]::Zero; $p.Play() }} }}"
        );
        if let Ok(c) = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            *child = Some(c);
        }
    }
}

/// First audio file in the same folder as `current`, if any.
pub async fn first_folder_audio(registry: &FsProviderRegistry, current: &FsPath) -> Option<FsPath> {
    let folder = current.parent()?;
    let provider = registry.for_path(&folder)?;
    let entries = provider.list(&folder).await.ok()?;
    entries.into_iter().find_map(|e| {
        let ext = e.path.extension()?;
        is_slideshow_audio_extension(ext).then_some(e.path)
    })
}

/// Cycle to the next audio sibling, or `None` to clear.
pub async fn next_folder_audio(
    registry: &FsProviderRegistry,
    current_image: &FsPath,
    current_music: Option<&str>,
) -> Option<FsPath> {
    let folder = current_image.parent()?;
    let provider = registry.for_path(&folder)?;
    let entries = provider.list(&folder).await.ok()?;
    let mut audios: Vec<FsPath> = entries
        .into_iter()
        .filter_map(|e| {
            let ext = e.path.extension()?;
            is_slideshow_audio_extension(ext).then_some(e.path)
        })
        .collect();
    if audios.is_empty() {
        return None;
    }
    audios.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let Some(cur) = current_music else {
        return audios.into_iter().next();
    };
    if let Some(i) = audios.iter().position(|p| p.as_str() == cur) {
        return audios.get(i + 1).cloned();
    }
    audios.into_iter().next()
}

/// Build an export spec from the live playlist.
pub fn export_spec(nav: &ImageFolderNav, slide: &SlideshowState) -> Option<SlideshowExport> {
    let images: Vec<PathBuf> = nav
        .siblings
        .iter()
        .filter_map(|p| p.to_local().ok())
        .filter(|p| p.is_file())
        .collect();
    if images.is_empty() {
        return None;
    }
    let music = slide
        .music_path
        .as_ref()
        .and_then(|s| FsPath::new(s).ok())
        .and_then(|p| p.to_local().ok())
        .filter(|p| p.is_file());
    Some(SlideshowExport {
        images,
        interval_ms: slide.interval_ms,
        transition: slide.transition,
        transition_ms: slide.transition_ms,
        random: slide.random,
        r#loop: nav.loop_playlist,
        overlay: slide.overlay,
        music,
    })
}

/// Write HTML/HTA/CMD/(EXE/SCR) under `folder/orchid-slideshow`.
pub fn write_pack(nav: &ImageFolderNav, slide: &SlideshowState) -> Result<PathBuf, String> {
    let spec = export_spec(nav, slide).ok_or_else(|| "no local images".to_string())?;
    let dest = nav
        .folder
        .as_ref()
        .and_then(|f| f.to_local().ok())
        .ok_or_else(|| "folder is not local".to_string())?
        .join("orchid-slideshow");
    export_slideshow_pack(&dest, &spec).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Write `folder/orchid-slideshow/slideshow.mp4` via ffmpeg.
pub fn write_video(nav: &ImageFolderNav, slide: &SlideshowState) -> Result<PathBuf, String> {
    let spec = export_spec(nav, slide).ok_or_else(|| "no local images".to_string())?;
    let dest = nav
        .folder
        .as_ref()
        .and_then(|f| f.to_local().ok())
        .ok_or_else(|| "folder is not local".to_string())?
        .join("orchid-slideshow");
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let mp4 = dest.join("slideshow.mp4");
    export_slideshow_video(&mp4, &spec).map_err(|e| e.to_string())
}

/// Refresh overlay text for a local image path.
#[must_use]
pub fn overlay_for_path(path: &FsPath) -> String {
    path.to_local()
        .ok()
        .map(|os| overlay_text(&os))
        .unwrap_or_else(|| path.file_name().unwrap_or_default().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_fs::FsPath;

    fn p(name: &str) -> FsPath {
        FsPath::new(format!("local:c:/pics/{name}")).unwrap()
    }

    #[test]
    fn shuffle_visits_every_readable() {
        let nav = ImageFolderNav {
            folder: Some(FsPath::new("local:c:/pics").unwrap()),
            siblings: vec![p("a.png"), p("b.png"), p("c.png")],
            index: 0,
            loop_playlist: true,
            ..ImageFolderNav::default()
        };
        let mut s = SlideshowState::default();
        s.rebuild_shuffle(&nav);
        let unique: std::collections::HashSet<_> = s.shuffled.iter().copied().collect();
        assert_eq!(unique.len(), 3);
        assert!(s.shuffled.contains(&nav.index));
        assert!(s.next_shuffled(&nav).is_some());

        let mut finite = nav.clone();
        finite.loop_playlist = false;
        s.rebuild_shuffle(&finite);
        s.shuffle_at = s.shuffled.len().saturating_sub(1);
        assert!(s.next_shuffled(&finite).is_none());
    }

    #[test]
    fn interval_clamps() {
        let mut s = SlideshowState {
            interval_ms: 1000,
            ..SlideshowState::default()
        };
        s.cycle_interval(true);
        assert_eq!(s.interval_ms, 1000);
        s.cycle_interval(false);
        assert_eq!(s.interval_ms, 2000);
    }

    #[test]
    fn wait_sleeps_remaining_interval_after_transition() {
        assert_eq!(slideshow_wait_ms(false, 500, 4000, 500, 10), 3500);
        assert_eq!(slideshow_wait_ms(false, 0, 4000, 500, 0), 50);
        assert_eq!(slideshow_wait_ms(true, 0, 4000, 500, 0), 200);
    }
}
