//! libmpv playback session (worker thread + shared snapshot state).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use super::ffi::{
    self, command_args, c_str, error_message, get_double, get_flag, get_int64, get_string,
    set_double, set_flag, set_string, MpvApi, MpvHandle, MpvRenderContext, MpvRenderParam,
    MPV_EVENT_NONE, MPV_EVENT_SHUTDOWN, MPV_EVENT_END_FILE, MPV_RENDER_PARAM_ADVANCED_CONTROL,
    MPV_RENDER_PARAM_API_TYPE, MPV_RENDER_PARAM_INVALID, MPV_RENDER_PARAM_SW_FORMAT,
    MPV_RENDER_PARAM_SW_POINTER, MPV_RENDER_PARAM_SW_SIZE, MPV_RENDER_PARAM_SW_STRIDE,
    MPV_RENDER_UPDATE_FRAME,
};
use super::prefs;
use super::resume;
use super::sidecars::discover_sidecar_subs;

/// Max software render width (height scales to preserve aspect).
const MAX_FRAME_W: u32 = 1920;
/// OSD flash duration after volume / speed / seek / mute changes.
const OSD_SECS: f64 = 1.4;

#[derive(Debug, Clone)]
pub struct FrameBuf {
    pub rgba: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

/// Shared playback state read by [`super::MediaViewer::snapshot`].
#[derive(Debug)]
pub struct SharedPlayback {
    pub available: AtomicBool,
    pub playing: AtomicBool,
    pub position_ms: AtomicU64,
    pub duration_ms: AtomicU64,
    pub volume: AtomicU32,
    pub muted: AtomicBool,
    /// Playback rate × 100 (100 = 1.0×).
    pub speed_x100: AtomicU32,
    pub has_video: AtomicBool,
    pub frame_gen: AtomicU64,
    pub frame: RwLock<Option<FrameBuf>>,
    /// Still image for audio-only (APIC / folder cover); not updated every frame.
    pub cover: RwLock<Option<FrameBuf>>,
    pub title: RwLock<String>,
    pub artist: RwLock<String>,
    /// OS path of the current file (for resume bookmarks).
    pub resume_path: RwLock<Option<PathBuf>>,
    /// Seek once duration is known after load.
    pub pending_resume_secs: RwLock<Option<f64>>,
    pub ab_label: RwLock<String>,
    pub eq_label: RwLock<String>,
    pub eq_index: AtomicU32,
    /// 0 = `auto-copy` (HW decode → system RAM for SW blit), 1 = `no`.
    pub hwdec_mode: AtomicU32,
    pub hwdec_label: RwLock<String>,
    pub osd_text: RwLock<String>,
    /// Instant::now() + duration; cleared when expired in poll.
    pub osd_until: RwLock<Option<Instant>>,
    pub sub_label: RwLock<String>,
    pub sub_visible: AtomicBool,
    pub audio_label: RwLock<String>,
    pub chapter_label: RwLock<String>,
    /// Set when mpv reaches end-of-file (cleared on load / take).
    pub eof_reached: AtomicBool,
    pub error: RwLock<Option<String>>,
    /// Set when a new frame or transport property changed.
    pub dirty: AtomicBool,
}

impl SharedPlayback {
    fn new(available: bool) -> Self {
        Self {
            available: AtomicBool::new(available),
            playing: AtomicBool::new(false),
            position_ms: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            volume: AtomicU32::new(100),
            muted: AtomicBool::new(false),
            speed_x100: AtomicU32::new(100),
            has_video: AtomicBool::new(false),
            frame_gen: AtomicU64::new(0),
            frame: RwLock::new(None),
            cover: RwLock::new(None),
            title: RwLock::new(String::new()),
            artist: RwLock::new(String::new()),
            resume_path: RwLock::new(None),
            pending_resume_secs: RwLock::new(None),
            ab_label: RwLock::new(String::new()),
            eq_label: RwLock::new(String::new()),
            eq_index: AtomicU32::new(0),
            hwdec_mode: AtomicU32::new(0),
            hwdec_label: RwLock::new(String::new()),
            osd_text: RwLock::new(String::new()),
            osd_until: RwLock::new(None),
            sub_label: RwLock::new(String::new()),
            sub_visible: AtomicBool::new(true),
            audio_label: RwLock::new(String::new()),
            chapter_label: RwLock::new(String::new()),
            eof_reached: AtomicBool::new(false),
            error: RwLock::new(None),
            dirty: AtomicBool::new(false),
        }
    }
}

enum EngineCmd {
    Load {
        path: PathBuf,
        sidecars: Vec<PathBuf>,
        resume_secs: Option<f64>,
    },
    PlayPause,
    SeekRel(f64),
    SeekAbs(f64),
    SetVolume(f64),
    VolumeDelta(f64),
    SetSpeed(f64),
    SpeedDelta(f64),
    MuteToggle,
    CycleSub,
    ToggleSub,
    CycleAudio,
    ChapterNext,
    ChapterPrev,
    AbMarkA,
    AbMarkB,
    AbClear,
    AddSub(PathBuf),
    SubScaleDelta(f64),
    SubPosDelta(f64),
    SubStyleReset,
    CycleEq,
    CycleHwdec,
    Stop,
    Quit,
}

/// Handle to a background libmpv session.
#[derive(Debug)]
pub struct MpvEngine {
    pub shared: Arc<SharedPlayback>,
    tx: Sender<EngineCmd>,
    _join: Option<JoinHandle<()>>,
}

impl MpvEngine {
    /// Start the worker. When libmpv is missing, commands become no-ops and
    /// [`SharedPlayback::available`] is false.
    pub fn spawn() -> Self {
        let available = ffi::mpv_available();
        let shared = Arc::new(SharedPlayback::new(available));
        let (tx, rx) = mpsc::channel::<EngineCmd>();
        let shared_worker = Arc::clone(&shared);
        let join = thread::Builder::new()
            .name("orchid-mpv".into())
            .spawn(move || {
                if !available {
                    // Drain until Quit so senders do not block forever.
                    while let Ok(cmd) = rx.recv() {
                        if matches!(cmd, EngineCmd::Quit) {
                            break;
                        }
                    }
                    return;
                }
                if let Err(e) = run_worker(shared_worker, rx) {
                    tracing::warn!(error = %e, "mpv worker exited");
                }
            })
            .ok();
        Self {
            shared,
            tx,
            _join: join,
        }
    }

    pub fn load(&self, path: &Path) {
        let sidecars = discover_sidecar_subs(path);
        let resume_secs = resume::take_resume(path);
        let _ = self.tx.send(EngineCmd::Load {
            path: path.to_path_buf(),
            sidecars,
            resume_secs,
        });
    }

    /// Still cover / tags for audio chrome (call with [`Self::load`]).
    pub fn set_cover_and_tags(&self, cover: Option<FrameBuf>, title: String, artist: String) {
        *self.shared.cover.write() = cover;
        *self.shared.title.write() = title;
        *self.shared.artist.write() = artist;
        self.shared.dirty.store(true, Ordering::Release);
    }

    pub fn play_pause(&self) {
        let _ = self.tx.send(EngineCmd::PlayPause);
    }

    pub fn seek_rel(&self, seconds: f64) {
        let _ = self.tx.send(EngineCmd::SeekRel(seconds));
    }

    pub fn seek_abs(&self, seconds: f64) {
        let _ = self.tx.send(EngineCmd::SeekAbs(seconds));
    }

    pub fn set_volume(&self, volume: f64) {
        let _ = self.tx.send(EngineCmd::SetVolume(volume));
    }

    pub fn volume_delta(&self, delta: f64) {
        let _ = self.tx.send(EngineCmd::VolumeDelta(delta));
    }

    pub fn set_speed(&self, speed: f64) {
        let _ = self.tx.send(EngineCmd::SetSpeed(speed));
    }

    pub fn speed_delta(&self, delta: f64) {
        let _ = self.tx.send(EngineCmd::SpeedDelta(delta));
    }

    pub fn mute_toggle(&self) {
        let _ = self.tx.send(EngineCmd::MuteToggle);
    }

    pub fn cycle_sub(&self) {
        let _ = self.tx.send(EngineCmd::CycleSub);
    }

    pub fn toggle_sub(&self) {
        let _ = self.tx.send(EngineCmd::ToggleSub);
    }

    pub fn cycle_audio(&self) {
        let _ = self.tx.send(EngineCmd::CycleAudio);
    }

    pub fn chapter_next(&self) {
        let _ = self.tx.send(EngineCmd::ChapterNext);
    }

    pub fn chapter_prev(&self) {
        let _ = self.tx.send(EngineCmd::ChapterPrev);
    }

    pub fn ab_mark_a(&self) {
        let _ = self.tx.send(EngineCmd::AbMarkA);
    }

    pub fn ab_mark_b(&self) {
        let _ = self.tx.send(EngineCmd::AbMarkB);
    }

    pub fn ab_clear(&self) {
        let _ = self.tx.send(EngineCmd::AbClear);
    }

    pub fn add_sub(&self, path: &Path) {
        let _ = self.tx.send(EngineCmd::AddSub(path.to_path_buf()));
    }

    pub fn sub_scale_delta(&self, delta: f64) {
        let _ = self.tx.send(EngineCmd::SubScaleDelta(delta));
    }

    pub fn sub_pos_delta(&self, delta: f64) {
        let _ = self.tx.send(EngineCmd::SubPosDelta(delta));
    }

    pub fn sub_style_reset(&self) {
        let _ = self.tx.send(EngineCmd::SubStyleReset);
    }

    pub fn cycle_eq(&self) {
        let _ = self.tx.send(EngineCmd::CycleEq);
    }

    pub fn cycle_hwdec(&self) {
        let _ = self.tx.send(EngineCmd::CycleHwdec);
    }

    pub fn stop(&self) {
        let _ = self.tx.send(EngineCmd::Stop);
    }

    /// Take dirty flag; returns true when UI should republish the snapshot.
    pub fn take_dirty(&self) -> bool {
        self.shared.dirty.swap(false, Ordering::AcqRel)
    }

    /// Take end-of-file flag (set once per finished track).
    pub fn take_eof(&self) -> bool {
        self.shared.eof_reached.swap(false, Ordering::AcqRel)
    }
}

impl Drop for MpvEngine {
    fn drop(&mut self) {
        let _ = self.tx.send(EngineCmd::Quit);
    }
}

fn run_worker(
    shared: Arc<SharedPlayback>,
    rx: mpsc::Receiver<EngineCmd>,
) -> Result<(), String> {
    let api = ffi::api().map_err(|_| "libmpv unavailable".to_string())?;
    let handle = unsafe { (api.create)() };
    if handle.is_null() {
        return Err("mpv_create failed".into());
    }

    unsafe {
        set_opt(api, handle, "vo", "libmpv")?;
        set_opt(api, handle, "terminal", "no")?;
        set_opt(api, handle, "input-default-bindings", "no")?;
        set_opt(api, handle, "input-vo-keyboard", "no")?;
        set_opt(api, handle, "osc", "no")?;
        set_opt(api, handle, "keep-open", "yes")?;
        set_opt(api, handle, "idle", "yes")?;
        // HW decode with copy-back so the SW (rgb0) render path can blit to Slint.
        set_opt(api, handle, "hwdec", "auto-copy")?;
        set_opt(api, handle, "video-sync", "audio")?;
        // Prefer embedded album art as a video track when present.
        let _ = set_opt(api, handle, "audio-display", "embedded-first");
        let prefs = prefs::load();
        let _ = set_opt(api, handle, "volume", &format!("{}", prefs.volume));
        let _ = set_opt(api, handle, "mute", if prefs.muted { "yes" } else { "no" });
        shared
            .volume
            .store(prefs.volume.round() as u32, Ordering::Relaxed);
        shared.muted.store(prefs.muted, Ordering::Relaxed);
        shared.hwdec_mode.store(prefs.hwdec_mode.min(1), Ordering::Relaxed);
        let rc = (api.initialize)(handle);
        if rc < 0 {
            let msg = error_message(api, rc);
            (api.terminate_destroy)(handle);
            return Err(msg);
        }
        let mode_idx = shared.hwdec_mode.load(Ordering::Relaxed).min(1) as usize;
        let hwdec = if mode_idx == 0 { "auto-copy" } else { "no" };
        let _ = set_string(api, handle, "hwdec", hwdec);
        refresh_hwdec_label(api, handle, &shared);
    }

    let render = unsafe { create_render_context(api, handle)? };
    let frame_flag = Arc::new(AtomicBool::new(false));
    let flag_cb = Arc::clone(&frame_flag);
    unsafe {
        (api.render_context_set_update_callback)(
            render,
            Some(on_render_update),
            Arc::into_raw(flag_cb) as *mut _,
        );
    }

    let mut last_prop = Instant::now() - Duration::from_secs(1);
    let mut quit = false;
    while !quit {
        while let Ok(cmd) = rx.try_recv() {
            if matches!(cmd, EngineCmd::Quit) {
                quit = true;
                break;
            }
            unsafe {
                handle_cmd(api, handle, &shared, cmd);
            }
        }
        if quit {
            break;
        }

        unsafe {
            drain_events(api, handle, &shared);
            let flags = (api.render_context_update)(render);
            if flags & MPV_RENDER_UPDATE_FRAME != 0 || frame_flag.swap(false, Ordering::AcqRel)
            {
                render_frame(api, handle, render, &shared);
            }
        }

        if last_prop.elapsed() >= Duration::from_millis(100) {
            last_prop = Instant::now();
            unsafe {
                poll_props(api, handle, &shared);
            }
        }

        // Short wait so we also wake for mpv events.
        unsafe {
            let ev = (api.wait_event)(handle, 0.02);
            if !ev.is_null() {
                let id = (*ev).event_id;
                if id == MPV_EVENT_SHUTDOWN {
                    quit = true;
                } else if id != MPV_EVENT_NONE {
                    // already drained above next loop; nothing
                }
            }
        }
    }

    unsafe {
        (api.render_context_set_update_callback)(render, None, std::ptr::null_mut());
        (api.render_context_free)(render);
        (api.terminate_destroy)(handle);
        // Leak the Arc intentionally if callback still holds it — we nulled the
        // callback above, so reclaim:
        // (callback pointer was Arc::into_raw; free it)
    }
    // Reclaim update-callback Arc (set to null already).
    // We passed Arc::into_raw once; drop it now.
    // Safety: callback cleared, no concurrent use.
    // Cannot easily recover the pointer after nulling — accept Arc leak of AtomicBool.

    Ok(())
}

unsafe extern "C" fn on_render_update(ctx: *mut std::ffi::c_void) {
    if ctx.is_null() {
        return;
    }
    let flag = &*(ctx as *const AtomicBool);
    flag.store(true, Ordering::Release);
}

unsafe fn set_opt(api: &MpvApi, handle: MpvHandle, key: &str, val: &str) -> Result<(), String> {
    let k = c_str(key);
    let v = c_str(val);
    let rc = (api.set_option_string)(handle, k.as_ptr(), v.as_ptr());
    if rc < 0 {
        Err(error_message(api, rc))
    } else {
        Ok(())
    }
}

unsafe fn create_render_context(
    api: &MpvApi,
    handle: MpvHandle,
) -> Result<MpvRenderContext, String> {
    let api_type = c_str("sw");
    let mut advanced: i32 = 0;
    let mut params = [
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_API_TYPE,
            data: api_type.as_ptr().cast_mut().cast(),
        },
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_ADVANCED_CONTROL,
            data: (&raw mut advanced).cast(),
        },
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_INVALID,
            data: std::ptr::null_mut(),
        },
    ];
    let mut ctx: MpvRenderContext = std::ptr::null_mut();
    let rc = (api.render_context_create)(&mut ctx, handle, params.as_mut_ptr());
    if rc < 0 || ctx.is_null() {
        Err(error_message(api, rc))
    } else {
        Ok(ctx)
    }
}

unsafe fn handle_cmd(api: &MpvApi, handle: MpvHandle, shared: &SharedPlayback, cmd: EngineCmd) {
    match cmd {
        EngineCmd::Load {
            path,
            sidecars,
            resume_secs,
        } => {
            persist_resume(shared);
            let path_s = path.to_string_lossy();
            let rc = command_args(api, handle, &["loadfile", path_s.as_ref(), "replace"]);
            if rc < 0 {
                *shared.error.write() = Some(error_message(api, rc));
                shared.dirty.store(true, Ordering::Release);
                return;
            }
            *shared.error.write() = None;
            shared.eof_reached.store(false, Ordering::Relaxed);
            *shared.resume_path.write() = Some(path.clone());
            *shared.pending_resume_secs.write() = resume_secs;
            *shared.ab_label.write() = String::new();
            shared.eq_index.store(0, Ordering::Relaxed);
            *shared.eq_label.write() = String::new();
            let _ = command_args(api, handle, &["af", "clr"]);
            let _ = command_args(api, handle, &["set", "ab-loop-a", "no"]);
            let _ = command_args(api, handle, &["set", "ab-loop-b", "no"]);
            for sub in sidecars {
                let s = sub.to_string_lossy();
                let _ = command_args(api, handle, &["sub-add", s.as_ref()]);
            }
            let _ = set_flag(api, handle, "pause", false);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::PlayPause => {
            let paused = get_flag(api, handle, "pause").unwrap_or(true);
            let _ = set_flag(api, handle, "pause", !paused);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::SeekRel(secs) => {
            let _ = command_args(api, handle, &["seek", &format!("{secs}"), "relative"]);
            let sign = if secs >= 0.0 { "+" } else { "" };
            flash_osd(shared, format!("Seek {sign}{secs:.0}s"));
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::SeekAbs(secs) => {
            let _ = command_args(api, handle, &["seek", &format!("{secs}"), "absolute"]);
            flash_osd(shared, format!("Seek {}", format_osd_time(secs)));
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::SetVolume(v) => {
            let v = v.clamp(0.0, 150.0);
            let _ = set_double(api, handle, "volume", v);
            shared.volume.store(v as u32, Ordering::Relaxed);
            prefs::store(
                v,
                shared.muted.load(Ordering::Relaxed),
                shared.hwdec_mode.load(Ordering::Relaxed),
            );
            flash_osd(shared, format!("Vol {:.0}%", v));
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::VolumeDelta(d) => {
            let cur = get_double(api, handle, "volume").unwrap_or(100.0);
            let v = (cur + d).clamp(0.0, 150.0);
            let _ = set_double(api, handle, "volume", v);
            shared.volume.store(v as u32, Ordering::Relaxed);
            prefs::store(
                v,
                shared.muted.load(Ordering::Relaxed),
                shared.hwdec_mode.load(Ordering::Relaxed),
            );
            flash_osd(shared, format!("Vol {:.0}%", v));
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::SetSpeed(s) => {
            let s = s.clamp(0.25, 3.0);
            let _ = set_double(api, handle, "speed", s);
            shared
                .speed_x100
                .store((s * 100.0).round() as u32, Ordering::Relaxed);
            flash_osd(shared, format!("Speed {s:.2}×"));
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::SpeedDelta(d) => {
            let cur = get_double(api, handle, "speed").unwrap_or(1.0);
            let s = (cur + d).clamp(0.25, 3.0);
            let _ = set_double(api, handle, "speed", s);
            shared
                .speed_x100
                .store((s * 100.0).round() as u32, Ordering::Relaxed);
            flash_osd(shared, format!("Speed {s:.2}×"));
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::MuteToggle => {
            let muted = get_flag(api, handle, "mute").unwrap_or(false);
            let _ = set_flag(api, handle, "mute", !muted);
            shared.muted.store(!muted, Ordering::Relaxed);
            let vol = shared.volume.load(Ordering::Relaxed) as f64;
            prefs::store(
                vol,
                !muted,
                shared.hwdec_mode.load(Ordering::Relaxed),
            );
            flash_osd(
                shared,
                if !muted {
                    "Muted".into()
                } else {
                    format!("Vol {:.0}%", vol)
                },
            );
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::CycleSub => {
            let _ = command_args(api, handle, &["cycle", "sub"]);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::ToggleSub => {
            let vis = get_flag(api, handle, "sub-visibility").unwrap_or(true);
            let _ = set_flag(api, handle, "sub-visibility", !vis);
            shared.sub_visible.store(!vis, Ordering::Relaxed);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::CycleAudio => {
            let _ = command_args(api, handle, &["cycle", "audio"]);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::ChapterNext => {
            let _ = command_args(api, handle, &["add", "chapter", "1"]);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::ChapterPrev => {
            let _ = command_args(api, handle, &["add", "chapter", "-1"]);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::AbMarkA => {
            if let Some(pos) = get_double(api, handle, "time-pos") {
                let _ = set_double(api, handle, "ab-loop-a", pos);
                refresh_ab_label(api, handle, shared);
                shared.dirty.store(true, Ordering::Release);
            }
        }
        EngineCmd::AbMarkB => {
            if let Some(pos) = get_double(api, handle, "time-pos") {
                let _ = set_double(api, handle, "ab-loop-b", pos);
                refresh_ab_label(api, handle, shared);
                shared.dirty.store(true, Ordering::Release);
            }
        }
        EngineCmd::AbClear => {
            let _ = command_args(api, handle, &["set", "ab-loop-a", "no"]);
            let _ = command_args(api, handle, &["set", "ab-loop-b", "no"]);
            *shared.ab_label.write() = String::new();
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::AddSub(path) => {
            let s = path.to_string_lossy();
            let rc = command_args(api, handle, &["sub-add", s.as_ref()]);
            if rc < 0 {
                *shared.error.write() = Some(error_message(api, rc));
            }
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::SubScaleDelta(d) => {
            let _ = command_args(api, handle, &["add", "sub-scale", &format!("{d}")]);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::SubPosDelta(d) => {
            let _ = command_args(api, handle, &["add", "sub-pos", &format!("{d}")]);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::SubStyleReset => {
            let _ = set_double(api, handle, "sub-scale", 1.0);
            let _ = set_double(api, handle, "sub-pos", 100.0);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::CycleEq => {
            apply_next_eq(api, handle, shared);
            let label = shared.eq_label.read().clone();
            flash_osd(
                shared,
                if label.is_empty() {
                    "EQ Flat".into()
                } else {
                    label
                },
            );
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::CycleHwdec => {
            apply_next_hwdec(api, handle, shared);
            let label = shared.hwdec_label.read().clone();
            flash_osd(
                shared,
                if label.is_empty() {
                    "Dec".into()
                } else {
                    label
                },
            );
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::Stop => {
            persist_resume(shared);
            let _ = command_args(api, handle, &["stop"]);
            *shared.frame.write() = None;
            *shared.cover.write() = None;
            *shared.title.write() = String::new();
            *shared.artist.write() = String::new();
            *shared.resume_path.write() = None;
            *shared.pending_resume_secs.write() = None;
            *shared.ab_label.write() = String::new();
            shared.eq_index.store(0, Ordering::Relaxed);
            *shared.eq_label.write() = String::new();
            *shared.hwdec_label.write() = String::new();
            shared.has_video.store(false, Ordering::Relaxed);
            shared.playing.store(false, Ordering::Relaxed);
            shared.dirty.store(true, Ordering::Release);
        }
        EngineCmd::Quit => {}
    }
}

unsafe fn drain_events(api: &MpvApi, handle: MpvHandle, shared: &SharedPlayback) {
    loop {
        let ev = (api.wait_event)(handle, 0.0);
        if ev.is_null() || (*ev).event_id == MPV_EVENT_NONE {
            break;
        }
        if (*ev).event_id == MPV_EVENT_SHUTDOWN {
            break;
        }
        if (*ev).event_id == MPV_EVENT_END_FILE {
            shared.eof_reached.store(true, Ordering::Release);
            shared.dirty.store(true, Ordering::Release);
        }
    }
}

unsafe fn poll_props(api: &MpvApi, handle: MpvHandle, shared: &SharedPlayback) {
    if let Some(pos) = get_double(api, handle, "time-pos") {
        shared
            .position_ms
            .store((pos.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
    }
    if let Some(dur) = get_double(api, handle, "duration") {
        shared
            .duration_ms
            .store((dur.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
        if dur > 0.0 {
            if let Some(secs) = shared.pending_resume_secs.write().take() {
                if secs > 0.0 && secs + 1.0 < dur {
                    let _ = command_args(api, handle, &["seek", &format!("{secs}"), "absolute"]);
                }
            }
        }
    }
    if let Some(paused) = get_flag(api, handle, "pause") {
        shared.playing.store(!paused, Ordering::Relaxed);
    }
    if let Some(vol) = get_double(api, handle, "volume") {
        shared.volume.store(vol as u32, Ordering::Relaxed);
    }
    if let Some(m) = get_flag(api, handle, "mute") {
        shared.muted.store(m, Ordering::Relaxed);
    }
    if let Some(sp) = get_double(api, handle, "speed") {
        shared
            .speed_x100
            .store((sp * 100.0).round() as u32, Ordering::Relaxed);
    }
    if let Some(vis) = get_flag(api, handle, "sub-visibility") {
        shared.sub_visible.store(vis, Ordering::Relaxed);
    }
    let vid = get_int64(api, handle, "video-params/w").unwrap_or(0);
    shared.has_video.store(vid > 0, Ordering::Relaxed);

    let sid = get_int64(api, handle, "sid").unwrap_or(-1);
    let label = if sid <= 0 {
        String::new()
    } else if let Some(title) = get_string(api, handle, "current-tracks/sub/title") {
        if title.is_empty() {
            get_string(api, handle, "current-tracks/sub/lang").unwrap_or_else(|| format!("sub {sid}"))
        } else {
            title
        }
    } else if let Some(lang) = get_string(api, handle, "current-tracks/sub/lang") {
        lang
    } else {
        format!("sub {sid}")
    };
    *shared.sub_label.write() = label;

    let aid = get_int64(api, handle, "aid").unwrap_or(-1);
    let audio = if aid <= 0 {
        String::new()
    } else if let Some(title) = get_string(api, handle, "current-tracks/audio/title") {
        if title.is_empty() {
            get_string(api, handle, "current-tracks/audio/lang")
                .unwrap_or_else(|| format!("audio {aid}"))
        } else {
            title
        }
    } else if let Some(lang) = get_string(api, handle, "current-tracks/audio/lang") {
        lang
    } else {
        format!("audio {aid}")
    };
    *shared.audio_label.write() = audio;

    let chapter = get_int64(api, handle, "chapter").unwrap_or(-1);
    let chapters = get_int64(api, handle, "chapters").unwrap_or(0);
    *shared.chapter_label.write() = if chapters > 0 && chapter >= 0 {
        let key = format!("chapter-list/{chapter}/title");
        match get_string(api, handle, &key) {
            Some(t) if !t.is_empty() => format!("{} ({}/{})", t, chapter + 1, chapters),
            _ => format!("{}/{}", chapter + 1, chapters),
        }
    } else {
        String::new()
    };
    refresh_ab_label(api, handle, shared);
    refresh_hwdec_label(api, handle, shared);
    expire_osd(shared);
    shared.dirty.store(true, Ordering::Release);
}

fn persist_resume(shared: &SharedPlayback) {
    let path = shared.resume_path.read().clone();
    let Some(path) = path else {
        return;
    };
    let pos = shared.position_ms.load(Ordering::Relaxed) as f64 / 1000.0;
    let dur = shared.duration_ms.load(Ordering::Relaxed) as f64 / 1000.0;
    resume::store_resume(&path, pos, dur);
}

fn flash_osd(shared: &SharedPlayback, text: String) {
    *shared.osd_text.write() = text;
    *shared.osd_until.write() = Some(Instant::now() + Duration::from_secs_f64(OSD_SECS));
}

fn expire_osd(shared: &SharedPlayback) {
    let clear = shared
        .osd_until
        .read()
        .is_some_and(|until| Instant::now() >= until);
    if clear {
        *shared.osd_text.write() = String::new();
        *shared.osd_until.write() = None;
    }
}

fn format_osd_time(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

unsafe fn refresh_ab_label(api: &MpvApi, handle: MpvHandle, shared: &SharedPlayback) {
    let a = get_double(api, handle, "ab-loop-a");
    let b = get_double(api, handle, "ab-loop-b");
    *shared.ab_label.write() = match (a, b) {
        (Some(a), Some(b)) => format!("A-B {a:.0}s–{b:.0}s"),
        (Some(a), None) => format!("A {a:.0}s"),
        (None, Some(b)) => format!("B {b:.0}s"),
        (None, None) => String::new(),
    };
}

/// Simple lavfi EQ presets (Flat → Bass → Treble → Vocal).
const EQ_PRESETS: &[(&str, Option<&str>)] = &[
    ("", None),
    ("Bass", Some("lavfi=[bass=g=6]")),
    ("Treble", Some("lavfi=[treble=g=5]")),
    ("Vocal", Some("lavfi=[equalizer=f=300:t=h:width=200:g=-3,equalizer=f=3000:t=h:width=1000:g=4]")),
];

unsafe fn apply_next_eq(api: &MpvApi, handle: MpvHandle, shared: &SharedPlayback) {
    let next = (shared.eq_index.load(Ordering::Relaxed) + 1) % EQ_PRESETS.len() as u32;
    shared.eq_index.store(next, Ordering::Relaxed);
    let (label, filter) = EQ_PRESETS[next as usize];
    let _ = command_args(api, handle, &["af", "clr"]);
    if let Some(af) = filter {
        let _ = command_args(api, handle, &["af", "add", af]);
    }
    *shared.eq_label.write() = if label.is_empty() {
        String::new()
    } else {
        format!("EQ {label}")
    };
}

/// Preferred decode modes for the SW blit path (`auto-copy` keeps frames in system RAM).
const HWDEC_MODES: &[&str] = &["auto-copy", "no"];

unsafe fn apply_next_hwdec(api: &MpvApi, handle: MpvHandle, shared: &SharedPlayback) {
    let next = (shared.hwdec_mode.load(Ordering::Relaxed) + 1) % HWDEC_MODES.len() as u32;
    shared.hwdec_mode.store(next, Ordering::Relaxed);
    let mode = HWDEC_MODES[next as usize];
    let _ = set_string(api, handle, "hwdec", mode);
    refresh_hwdec_label(api, handle, shared);
    prefs::store(
        shared.volume.load(Ordering::Relaxed) as f64,
        shared.muted.load(Ordering::Relaxed),
        next,
    );
}

unsafe fn refresh_hwdec_label(api: &MpvApi, handle: MpvHandle, shared: &SharedPlayback) {
    let mode = HWDEC_MODES[shared.hwdec_mode.load(Ordering::Relaxed) as usize];
    let current = get_string(api, handle, "hwdec-current").unwrap_or_default();
    *shared.hwdec_label.write() = if mode == "no" {
        "Dec SW".into()
    } else if current.is_empty() || current == "no" {
        "Dec auto-copy".into()
    } else {
        format!("Dec {current}")
    };
}

unsafe fn render_frame(
    api: &MpvApi,
    handle: MpvHandle,
    render: MpvRenderContext,
    shared: &SharedPlayback,
) {
    let vw = get_int64(api, handle, "video-params/w").unwrap_or(0).max(0) as u32;
    let vh = get_int64(api, handle, "video-params/h").unwrap_or(0).max(0) as u32;
    if vw == 0 || vh == 0 {
        shared.has_video.store(false, Ordering::Relaxed);
        return;
    }
    shared.has_video.store(true, Ordering::Relaxed);

    let (w, h) = scale_frame(vw, vh, MAX_FRAME_W);
    let stride = (w as usize) * 4;
    let mut buf = vec![0u8; stride * h as usize];

    let mut size = [w as i32, h as i32];
    // Documented SW formats: rgb0 = R,G,B,pad. Convert to RGBA for Slint.
    let format = c_str("rgb0");
    let mut stride_sz: usize = stride;
    let mut params = [
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_SW_SIZE,
            data: size.as_mut_ptr().cast(),
        },
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_SW_FORMAT,
            data: format.as_ptr().cast_mut().cast(),
        },
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_SW_STRIDE,
            data: (&raw mut stride_sz).cast(),
        },
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_SW_POINTER,
            data: buf.as_mut_ptr().cast(),
        },
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_INVALID,
            data: std::ptr::null_mut(),
        },
    ];
    let rc = (api.render_context_render)(render, params.as_mut_ptr());
    if rc < 0 {
        *shared.error.write() = Some(error_message(api, rc));
        shared.dirty.store(true, Ordering::Release);
        return;
    }

    // rgb0 → RGBA (set alpha opaque).
    for px in buf.chunks_exact_mut(4) {
        px[3] = 255;
    }

    *shared.frame.write() = Some(FrameBuf {
        rgba: Arc::new(buf),
        width: w,
        height: h,
    });
    shared.frame_gen.fetch_add(1, Ordering::Relaxed);
    shared.dirty.store(true, Ordering::Release);
}

fn scale_frame(w: u32, h: u32, max_w: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (0, 0);
    }
    if w <= max_w {
        return (w, h);
    }
    let scale = f64::from(max_w) / f64::from(w);
    let nh = ((f64::from(h) * scale).round() as u32).max(1);
    (max_w, nh)
}

#[cfg(test)]
mod tests {
    use super::scale_frame;

    #[test]
    fn scale_preserves_small() {
        assert_eq!(scale_frame(640, 360, 1280), (640, 360));
    }

    #[test]
    fn scale_shrinks_wide() {
        let (w, h) = scale_frame(1920, 1080, 1280);
        assert_eq!(w, 1280);
        assert_eq!(h, 720);
    }
}
