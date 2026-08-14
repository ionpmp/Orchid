//! Slideshow playlist helpers: transitions, overlay text, and exports.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use orchid_fs::FsPath;

use crate::error::{Result, ViewerError};
use crate::image::exif::read_exif_fields;
use crate::media::is_media_file_extension;

/// Visual transition between slides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SlideTransition {
    /// Instant cut.
    None = 0,
    /// Cross-fade opacity.
    #[default]
    Fade = 1,
    /// Incoming slide from the right.
    Slide = 2,
    /// Soft cross-fade (same as fade in the live viewer).
    Dissolve = 3,
    /// Horizontal wipe reveal.
    Wipe = 4,
}

impl SlideTransition {
    /// Persist / snapshot encoding.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Self::as_u8`]; unknown values become fade.
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::None,
            2 => Self::Slide,
            3 => Self::Dissolve,
            4 => Self::Wipe,
            _ => Self::Fade,
        }
    }

    /// Cycle none → fade → slide → dissolve → wipe → none.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::None => Self::Fade,
            Self::Fade => Self::Slide,
            Self::Slide => Self::Dissolve,
            Self::Dissolve => Self::Wipe,
            Self::Wipe => Self::None,
        }
    }

    /// Token used in the HTML/JS player.
    #[must_use]
    pub fn js_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fade => "fade",
            Self::Slide => "slide",
            Self::Dissolve => "dissolve",
            Self::Wipe => "wipe",
        }
    }
}

/// Name, date, and a short EXIF line for the slideshow overlay.
#[must_use]
pub fn overlay_text(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let date = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| {
            let dt = chrono::DateTime::<chrono::Local>::from(t);
            dt.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_default();
    let exif = short_exif_line(path);
    let mut out = name.to_string();
    if !date.is_empty() {
        out.push('\n');
        out.push_str(&date);
    }
    if !exif.is_empty() {
        out.push('\n');
        out.push_str(&exif);
    }
    out
}

fn short_exif_line(path: &Path) -> String {
    let Ok(fields) = read_exif_fields(path) else {
        return String::new();
    };
    let mut picked = Vec::new();
    for (tag, value) in fields {
        if matches!(
            tag.as_str(),
            "DateTimeOriginal"
                | "Model"
                | "FNumber"
                | "ExposureTime"
                | "PhotographicSensitivity"
                | "ISOSpeedRatings"
                | "FocalLength"
        ) {
            picked.push(format!("{tag} {value}"));
        }
        if picked.len() >= 4 {
            break;
        }
    }
    picked.join(" · ")
}

/// True when `ext` is a still-image audio bed (not video).
#[must_use]
pub fn is_slideshow_audio_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" | "wma" | "opus" | "aiff"
    ) && is_media_file_extension(ext)
}

/// Settings written into an exported player.
#[derive(Debug, Clone)]
pub struct SlideshowExport {
    /// Local image files in play order.
    pub images: Vec<PathBuf>,
    /// Dwell time per slide.
    pub interval_ms: u32,
    /// Visual transition kind.
    pub transition: SlideTransition,
    /// How long the transition itself lasts.
    pub transition_ms: u32,
    /// Shuffle play order.
    pub random: bool,
    /// Repeat the playlist at the end.
    pub r#loop: bool,
    /// Show filename / date overlay.
    pub overlay: bool,
    /// Optional looping audio bed.
    pub music: Option<PathBuf>,
}

/// Write a portable HTML player (plus HTA / CMD / optional EXE+SCR) under `dest`.
///
/// # Errors
///
/// I/O or a missing image list.
pub fn export_slideshow_pack(dest: &Path, spec: &SlideshowExport) -> Result<PathBuf> {
    if spec.images.is_empty() {
        return Err(ViewerError::ThumbnailFailed("no images to export".into()));
    }
    std::fs::create_dir_all(dest)?;
    let html = dest.join("index.html");
    std::fs::write(&html, render_html_player(spec))?;
    write_launchers(dest, spec)?;
    if let Some(exe) = try_compile_launcher(dest) {
        let scr = dest.join("OrchidSlideshow.scr");
        let _ = std::fs::copy(&exe, &scr);
    }
    Ok(html)
}

/// Build an MP4 via `ffmpeg` when the binary is on PATH.
///
/// # Errors
///
/// Missing ffmpeg, empty playlist, or a non-zero ffmpeg exit.
pub fn export_slideshow_video(dest_mp4: &Path, spec: &SlideshowExport) -> Result<PathBuf> {
    if spec.images.is_empty() {
        return Err(ViewerError::ThumbnailFailed("no images to export".into()));
    }
    let ffmpeg = find_ffmpeg().ok_or_else(|| {
        ViewerError::ThumbnailFailed("ffmpeg not found on PATH (needed for video export)".into())
    })?;
    if let Some(parent) = dest_mp4.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let list_path = dest_mp4.with_extension("ffconcat");
    {
        let mut list = std::fs::File::create(&list_path)?;
        writeln!(list, "ffconcat version 1.0")
            .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
        let secs = (spec.interval_ms as f64 / 1000.0).max(0.2);
        for img in &spec.images {
            let p = img.display().to_string().replace('\\', "/");
            writeln!(list, "file '{p}'")
                .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
            writeln!(list, "duration {secs}")
                .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
        }
        if let Some(last) = spec.images.last() {
            let p = last.display().to_string().replace('\\', "/");
            writeln!(list, "file '{p}'")
                .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
        }
    }
    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-y")
        .args(["-f", "concat", "-safe", "0", "-i"])
        .arg(&list_path);
    if let Some(music) = &spec.music {
        cmd.arg("-i").arg(music).args(["-shortest"]);
    }
    cmd.args([
        "-vf",
        "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,format=yuv420p",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
    ])
    .arg(dest_mp4);
    let status = cmd
        .status()
        .map_err(|e| ViewerError::ThumbnailFailed(format!("ffmpeg: {e}")))?;
    let _ = std::fs::remove_file(&list_path);
    if !status.success() {
        return Err(ViewerError::ThumbnailFailed(format!(
            "ffmpeg exited with {status}"
        )));
    }
    Ok(dest_mp4.to_path_buf())
}

fn find_ffmpeg() -> Option<PathBuf> {
    if Command::new("ffmpeg").arg("-version").output().is_ok() {
        return Some(PathBuf::from("ffmpeg"));
    }
    None
}

fn write_launchers(dest: &Path, spec: &SlideshowExport) -> Result<()> {
    let _ = spec;
    let cmd = dest.join("Play.cmd");
    std::fs::write(
        &cmd,
        "@echo off\r\n\
         set HERE=%~dp0\r\n\
         if exist \"%HERE%OrchidSlideshow.exe\" (\r\n\
         start \"\" \"%HERE%OrchidSlideshow.exe\"\r\n\
         exit /b 0\r\n\
         )\r\n\
         start \"\" mshta \"%HERE%Play.hta\"\r\n",
    )?;
    let hta = dest.join("Play.hta");
    std::fs::write(
        &hta,
        "<html><head><title>Orchid Slideshow</title>\r\n\
         <HTA:APPLICATION ID=\"orchidSlide\" BORDER=\"none\" CAPTION=\"no\" SHOWINTASKBAR=\"yes\" SCROLL=\"no\" />\r\n\
         <script>window.location.replace('index.html');</script></head><body></body></html>\r\n",
    )?;
    Ok(())
}

fn try_compile_launcher(dest: &Path) -> Option<PathBuf> {
    let src = dest.join("orchid_slideshow_launcher.rs");
    let exe = dest.join("OrchidSlideshow.exe");
    let code = r#"
fn main() {
    let exe = match std::env::current_exe() { Ok(p) => p, Err(_) => return };
    let Some(dir) = exe.parent() else { return };
    let html = dir.join("index.html");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a.eq_ignore_ascii_case("/p") || a.eq_ignore_ascii_case("/c")) {
        return;
    }
    let path = html.to_string_lossy().replace('\\', "/");
    let url = format!("file:///{path}");
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", "msedge", "--kiosk", "--edge-kiosk-type=fullscreen", &url])
        .spawn();
}
"#;
    std::fs::write(&src, code).ok()?;
    let status = Command::new("rustc")
        .args(["-O", "-C", "opt-level=s", "-o"])
        .arg(&exe)
        .arg(&src)
        .status()
        .ok()?;
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(dest.join("orchid_slideshow_launcher.pdb"));
    if status.success() && exe.is_file() {
        Some(exe)
    } else {
        None
    }
}

fn render_html_player(spec: &SlideshowExport) -> String {
    let images: Vec<String> = spec
        .images
        .iter()
        .filter_map(|p| p.to_str())
        .map(|s| format!("\"{}\"", js_escape(&s.replace('\\', "/"))))
        .collect();
    let music = spec
        .music
        .as_ref()
        .and_then(|p| p.to_str())
        .map(|s| s.replace('\\', "/"))
        .unwrap_or_default();
    format!(
        r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>Orchid Slideshow</title>
<style>
html,body{{margin:0;height:100%;background:#111;color:#eee;font:14px/1.4 sans-serif;overflow:hidden}}
#stage{{position:fixed;inset:0}}
#stage img{{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;transition:opacity {trans}ms ease,transform {trans}ms ease,clip-path {trans}ms ease}}
#stage img.out{{opacity:0}}
#stage.fade img.out{{opacity:0}}
#stage.slide img.out{{transform:translateX(-40%);opacity:0}}
#stage.dissolve img.out{{opacity:0;filter:blur(12px)}}
#stage.wipe img.out{{clip-path:inset(0 100% 0 0)}}
#ov{{position:fixed;left:16px;bottom:16px;padding:8px 12px;background:#000a;border-radius:6px;white-space:pre-line;max-width:70vw}}
#bar{{position:fixed;right:16px;bottom:16px;opacity:.7}}
</style></head><body>
<div id="stage" class="{kind}"></div>
<div id="ov" {ovhide}></div>
<div id="bar"></div>
{audio}
<script>
const IMAGES=[{images}];
const INTERVAL={interval};
const RANDOM={random};
const LOOP={loop_on};
const OVERLAY={overlay};
let i=0, paused=false, order=IMAGES.map((_,n)=>n);
if(RANDOM) order.sort(()=>Math.random()-0.5);
const stage=document.getElementById('stage');
const ov=document.getElementById('ov');
function show(idx){{
  const src=IMAGES[order[idx]];
  const img=new Image();
  img.src=src.startsWith('file:')?src:('file:///'+src);
  img.onload=()=>{{
    const prev=stage.querySelector('img');
    stage.appendChild(img);
    if(prev){{ prev.classList.add('out'); setTimeout(()=>prev.remove(), {trans}+40); }}
    if(OVERLAY) ov.textContent=src.split('/').pop();
  }};
}}
function next(){{
  if(paused) return;
  i+=1;
  if(i>=order.length){{ if(!LOOP){{ paused=true; return; }} i=0; if(RANDOM) order.sort(()=>Math.random()-0.5); }}
  show(i);
}}
document.addEventListener('keydown',e=>{{
  if(e.code==='Space'){{ paused=!paused; e.preventDefault(); }}
  if(e.code==='ArrowRight') next();
  if(e.code==='Escape') window.close();
}});
show(0);
setInterval(next, INTERVAL);
</script></body></html>
"#,
        trans = spec.transition_ms.max(80),
        kind = spec.transition.js_name(),
        ovhide = if spec.overlay { "" } else { "hidden" },
        audio = if music.is_empty() {
            String::new()
        } else {
            format!(
                "<audio src=\"file:///{} \" autoplay loop></audio>",
                js_escape(&music)
            )
        },
        images = images.join(","),
        interval = spec.interval_ms.max(400),
        random = if spec.random { "true" } else { "false" },
        loop_on = if spec.r#loop { "true" } else { "false" },
        overlay = if spec.overlay { "true" } else { "false" },
    )
}

fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

/// Local filesystem path for an `FsPath`, when the scheme is `local`.
#[must_use]
pub fn local_os_path(path: &FsPath) -> Option<PathBuf> {
    path.to_local().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_cycles_all_five() {
        let mut t = SlideTransition::None;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..5 {
            seen.insert(t.as_u8());
            t = t.cycle();
        }
        assert_eq!(seen.len(), 5);
        assert_eq!(t, SlideTransition::None);
    }

    #[test]
    fn html_player_contains_interval_and_images() {
        let spec = SlideshowExport {
            images: vec![
                PathBuf::from("C:/pics/a.jpg"),
                PathBuf::from("C:/pics/b.jpg"),
            ],
            interval_ms: 3500,
            transition: SlideTransition::Wipe,
            transition_ms: 400,
            random: true,
            r#loop: true,
            overlay: true,
            music: None,
        };
        let html = render_html_player(&spec);
        assert!(html.contains("3500"));
        assert!(html.contains("wipe"));
        assert!(html.contains("a.jpg"));
    }

    #[test]
    fn audio_ext_rejects_video() {
        assert!(is_slideshow_audio_extension("mp3"));
        assert!(!is_slideshow_audio_extension("mp4"));
    }

    #[test]
    fn overlay_includes_file_name() {
        let dir = std::env::temp_dir();
        let path = dir.join("orchid-slide-overlay-test.png");
        let _ = std::fs::write(&path, b"not-a-real-png");
        let text = overlay_text(&path);
        let _ = std::fs::remove_file(&path);
        assert!(text.contains("orchid-slide-overlay-test.png"));
    }
}
