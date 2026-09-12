//! Image viewer commands.

use super::*;

/// Multiply the current image zoom by `factor`.
pub async fn image_zoom_by(instance_id: Uuid, factor: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.zoom_by(factor);
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: zoom in (~10%).
pub async fn image_zoom_in(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.zoom_by(1.1);
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: zoom out (~10%).
pub async fn image_zoom_out(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.zoom_by(1.0 / 1.1);
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: fit to viewport.
pub async fn image_fit(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.fit_to_viewport();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: 1:1.
pub async fn image_actual_size(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.actual_size();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: rotate clockwise.
pub async fn image_rotate_cw(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.rotate_cw();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: rotate counter-clockwise.
pub async fn image_rotate_ccw(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.rotate_ccw();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: toggle horizontal flip.
pub async fn image_flip_h(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.flip_horizontal();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: toggle vertical flip.
pub async fn image_flip_v(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.flip_vertical();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

pub(crate) fn print_uses_folder(raw: &str) -> bool {
    raw.split(" | ").any(|p| {
        matches!(
            p.trim().to_ascii_lowercase().as_str(),
            "folder" | "sheet" | "index" | "contact"
        )
    })
}

/// Image toolbar / keyboard command (`fit-width`, `fit-height`, `fit-shrink`,
/// `bg-next`, `fullscreen`, `kiosk`, …).
pub async fn image_command(instance_id: Uuid, command: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
            return Ok(());
        };
        match command {
            "fit-window" | "fit" => img.fit_to_viewport(),
            "fit-width" => img.fit_to_width(),
            "fit-height" => img.fit_to_height(),
            "fit-shrink" => img.fit_shrink(),
            "actual" => img.actual_size(),
            "bg-next" => img.cycle_background(),
            "fullscreen" => img.toggle_chrome_hidden(),
            "kiosk" => img.toggle_kiosk(),
            "exit-immersive" => img.exit_immersive(),
            "lens" => img.toggle_lens(),
            "rotate-180" => img.rotate_180(),
            "reset-transform" => img.reset_transforms(),
            "next"
            | "prev"
            | "first"
            | "last"
            | "random"
            | "loop"
            | "thumbs"
            | "thumb-grid"
            | "thumb-size"
            | "thumb-meta"
            | "thumb-refresh"
            | "contact-sheet"
            | "browse-timeline"
            | "browse-map"
            | "browse-calendar"
            | "browse-off"
            | "cal-prev"
            | "cal-next"
            | "overlay-autohide"
            | "slideshow"
            | "slideshow-stop"
            | "slideshow-pause"
            | "slideshow-faster"
            | "slideshow-slower"
            | "slideshow-random"
            | "slideshow-transition"
            | "slideshow-trans-ms"
            | "slideshow-overlay"
            | "slideshow-music"
            | "slideshow-export-html"
            | "slideshow-export-video"
            | "slideshow-export-exe"
            | "slideshow-export-scr"
            | "meta-panel"
            | "meta-overlay"
            | "hist-mode"
            | "gps-map"
            | "meta-strip"
            | "meta-strip-gps"
            | "meta-export-csv"
            | "meta-export-xml"
            | "edit-auto-straighten"
            | "copy-image"
            | "paste-image"
            | "wallpaper"
            | "screenshot"
            | "anim-export"
            | "anim-extract" => {}
            "anim-play" => img.set_anim_playing(true),
            "anim-pause" => img.set_anim_playing(false),
            "anim-toggle" => img.toggle_anim(),
            "anim-next" => img.anim_step(1),
            "anim-prev" => img.anim_step(-1),
            "anim-first" => img.anim_goto(1),
            "anim-last" => img.anim_goto(img.anim_count()),
            cmd if let Some(raw) = cmd.strip_prefix("anim-goto:") => {
                if let Ok(n) = raw.parse::<usize>() {
                    img.anim_goto(n);
                }
            }
            cmd if cmd.starts_with("goto:")
                || cmd.starts_with("recent:")
                || cmd.starts_with("lossless")
                || cmd.starts_with("preload:")
                || cmd.starts_with("open-thumb:")
                || cmd.starts_with("slideshow-interval:")
                || cmd.starts_with("slideshow-music:")
                || cmd.starts_with("probe:")
                || cmd.starts_with("meta-save:")
                || cmd.starts_with("edit-")
                || cmd.starts_with("adjust:")
                || cmd.starts_with("filter:")
                || cmd.starts_with("annotate")
                || cmd.starts_with("print")
                || cmd.starts_with("export")
                || cmd.starts_with("screenshot")
                || cmd.starts_with("email")
                || cmd.starts_with("share")
                || cmd.starts_with("save-as")
                || cmd.starts_with("anim-") => {}
            cmd if let Some(raw) = cmd.strip_prefix("rotate:") => {
                if let Ok(deg) = raw.parse::<f32>() {
                    img.set_rotation(deg);
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("rotate-by:") => {
                if let Ok(delta) = raw.parse::<f32>() {
                    img.rotate_by(delta);
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("zoom:") => {
                let raw = raw.trim().trim_end_matches('%');
                if let Ok(p) = raw.parse::<f32>() {
                    img.zoom_to_percent(p);
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("zoom-at:") => {
                let parts: Vec<&str> = raw.split(':').collect();
                if parts.len() == 3 {
                    if let (Ok(f), Ok(x), Ok(y)) = (
                        parts[0].parse::<f32>(),
                        parts[1].parse::<f32>(),
                        parts[2].parse::<f32>(),
                    ) {
                        img.zoom_at(f, x, y);
                    }
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("zoom-rect:") => {
                let parts: Vec<&str> = raw.split(':').collect();
                if parts.len() == 4 {
                    if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                        parts[0].parse::<f32>(),
                        parts[1].parse::<f32>(),
                        parts[2].parse::<f32>(),
                        parts[3].parse::<f32>(),
                    ) {
                        img.zoom_to_rect(x0, y0, x1, y1);
                    }
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("nav-pan:") => {
                let parts: Vec<&str> = raw.split(':').collect();
                if parts.len() == 2 {
                    if let (Ok(nx), Ok(ny)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                        img.pan_to_image_fraction(nx, ny);
                    }
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("pinch:") => {
                if let Ok(f) = raw.parse::<f32>() {
                    img.zoom_by(f);
                }
            }
            _ => {}
        }
    }
    if matches!(command, "anim-play" | "anim-toggle") {
        inner.schedule_anim_ticks();
    }
    match command {
        "next" => inner.navigate_images(image_nav::NavStep::Next).await?,
        "prev" => inner.navigate_images(image_nav::NavStep::Prev).await?,
        "first" => inner.navigate_images(image_nav::NavStep::First).await?,
        "last" => inner.navigate_images(image_nav::NavStep::Last).await?,
        "random" => inner.navigate_images(image_nav::NavStep::Random).await?,
        "loop" => {
            let next = !inner.image_nav.read().loop_playlist;
            inner.image_nav.write().loop_playlist = next;
            inner.refresh_snapshot().await;
        }
        cmd if let Some(raw) = cmd.strip_prefix("goto:") => {
            if let Ok(n) = raw.parse::<usize>() {
                inner.navigate_images(image_nav::NavStep::Goto(n)).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("recent:") => {
            if let Ok(path) = orchid_fs::FsPath::new(raw) {
                inner.open_path(path).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("lossless-folder:") => {
            if let Some(op) = LosslessOp::from_token(raw) {
                inner.apply_lossless_folder(op).await?;
            }
        }
        "edit-auto-straighten" => inner.apply_edit_current(EditOp::AutoStraighten).await?,
        cmd if let Some(raw) = cmd.strip_prefix("adjust:") => {
            if let Some(op) = parse_adjust_line(raw) {
                inner.apply_adjust_current(op).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("filter:") => {
            let dir = inner.path.read().as_ref().and_then(|p| {
                p.to_local()
                    .ok()
                    .and_then(|os| os.parent().map(std::path::Path::to_path_buf))
            });
            if let Some(op) = parse_filter_line_in(raw, dir.as_deref()) {
                inner.apply_filter_current(op).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("annotate-view:") => {
            inner.annotate_from_view(raw).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("annotate:") => {
            if let Some(op) = parse_annotate_line(raw) {
                inner.apply_annotate_current(op).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("print-preview:") => {
            let folder = print_uses_folder(raw);
            inner.print_job(raw, true, folder).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("print-sheet:") => {
            let mut line = raw.to_string();
            if !line.contains("sheet") {
                line = format!("sheet | {line}");
            }
            inner.print_job(&line, false, true).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("print:") => {
            inner.print_job(raw, false, print_uses_folder(raw)).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("export:") => {
            inner.export_current(raw).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("save-as:") => {
            inner.export_current(raw).await?;
        }
        "copy-image" => inner.copy_image().await?,
        "paste-image" => inner.paste_image().await?,
        "wallpaper" => inner.set_current_wallpaper().await?,
        cmd if let Some(raw) = cmd.strip_prefix("email:") => {
            inner.email_current(raw).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("share:") => {
            inner.share_current(raw).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("screenshot:") => {
            inner.screenshot_current(raw).await?;
        }
        "screenshot" => inner.screenshot_current("").await?,
        cmd if let Some(raw) = cmd.strip_prefix("edit-resize:") => {
            if let Some((spec, filter)) = parse_resize_line(raw) {
                inner
                    .apply_edit_current(EditOp::Resize { spec, filter })
                    .await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("edit-canvas:") => {
            let size = {
                let guard = inner.viewer.lock().await;
                guard.as_ref().and_then(|v| {
                    v.as_any()
                        .downcast_ref::<ImageViewer>()
                        .and_then(|img| img.clone_loaded())
                        .map(|i| (i.width, i.height))
                })
            };
            if let Some((sw, sh)) = size {
                if let Some((w, h)) = parse_canvas_line(raw, sw, sh) {
                    inner
                        .apply_edit_current(EditOp::Canvas {
                            width: w,
                            height: h,
                            fill: [0, 0, 0, 255],
                        })
                        .await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("edit-crop:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() >= 4 {
                if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                    parts[0].parse::<f32>(),
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    let aspect = parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    let keep = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0u8);
                    inner
                        .edit_crop_from_view(x0, y0, x1, y1, aspect, keep)
                        .await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("edit-straighten:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() == 4 {
                if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                    parts[0].parse::<f32>(),
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    inner
                        .edit_line_from_view(x0, y0, x1, y1, false, &[])
                        .await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("edit-perspective:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() == 8 {
                let nums: Result<Vec<f32>, _> = parts.iter().map(|s| s.parse()).collect();
                if let Ok(n) = nums {
                    inner
                        .edit_line_from_view(
                            n[0],
                            n[1],
                            n[2],
                            n[3],
                            true,
                            &[(n[4], n[5]), (n[6], n[7])],
                        )
                        .await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("lossless-crop:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() == 4 {
                if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                    parts[0].parse::<f32>(),
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    inner.apply_lossless_crop(x0, y0, x1, y1).await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("lossless-") => {
            if let Some(op) = LosslessOp::from_token(raw) {
                inner.apply_lossless_current(op).await?;
            }
        }
        "thumbs" => {
            inner.image_thumbs.write().cycle_strip();
            inner.refresh_snapshot().await;
        }
        "thumb-grid" => {
            let next = !inner.image_thumbs.read().grid;
            inner.image_thumbs.write().grid = next;
            inner.refresh_snapshot().await;
        }
        "thumb-size" => {
            inner.image_thumbs.write().cycle_size();
            inner.schedule_thumbs_and_preload();
            inner.refresh_snapshot().await;
        }
        "thumb-meta" => {
            let next = !inner.image_thumbs.read().show_meta;
            inner.image_thumbs.write().show_meta = next;
            inner.refresh_snapshot().await;
        }
        "thumb-refresh" => {
            inner.image_thumbs.write().items.clear();
            inner.schedule_thumbs_and_preload();
            inner.refresh_snapshot().await;
        }
        "contact-sheet" => {
            inner.write_contact_sheet().await?;
        }
        "browse-timeline" => {
            inner
                .image_thumbs
                .write()
                .toggle_browse(image_browse::BROWSE_TIMELINE);
            inner.refresh_snapshot().await;
        }
        "browse-map" => {
            inner
                .image_thumbs
                .write()
                .toggle_browse(image_browse::BROWSE_MAP);
            inner.refresh_snapshot().await;
        }
        "browse-calendar" => {
            inner
                .image_thumbs
                .write()
                .toggle_browse(image_browse::BROWSE_CALENDAR);
            inner.refresh_snapshot().await;
        }
        "browse-off" => {
            inner.image_thumbs.write().browse = image_browse::BROWSE_PHOTO;
            inner.refresh_snapshot().await;
        }
        "cal-prev" => {
            {
                let mut th = inner.image_thumbs.write();
                let (y, m) =
                    image_browse::shift_month(th.cal_year, u32::from(th.cal_month.max(1)), -1);
                th.cal_year = y;
                th.cal_month = m as u8;
            }
            inner.refresh_snapshot().await;
        }
        "cal-next" => {
            {
                let mut th = inner.image_thumbs.write();
                let (y, m) =
                    image_browse::shift_month(th.cal_year, u32::from(th.cal_month.max(1)), 1);
                th.cal_year = y;
                th.cal_month = m as u8;
            }
            inner.refresh_snapshot().await;
        }
        "overlay-autohide" => {
            let next = !inner.image_thumbs.read().overlay_autohide;
            inner.image_thumbs.write().overlay_autohide = next;
            inner.refresh_snapshot().await;
        }
        cmd if let Some(raw) = cmd.strip_prefix("preload:") => {
            if let Ok(n) = raw.parse::<u8>() {
                inner.image_thumbs.write().preload_n = n.min(8);
                inner.schedule_thumbs_and_preload();
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("open-thumb:") => {
            if let Ok(path) = orchid_fs::FsPath::new(raw) {
                inner.image_thumbs.write().grid = false;
                inner.image_thumbs.write().browse = image_browse::BROWSE_PHOTO;
                inner.open_path(path).await?;
            }
        }
        "anim-export" => inner.export_anim_frames().await?,
        "anim-extract" => inner.extract_anim_frame().await?,
        "slideshow" => inner.toggle_slideshow().await?,
        "slideshow-stop" => {
            inner.stop_slideshow();
            inner.refresh_snapshot().await;
        }
        "slideshow-pause" => {
            if inner.slideshow.read().playing {
                let next = !inner.slideshow.read().paused;
                inner.slideshow.write().paused = next;
                inner.reschedule_slideshow_ticks();
                inner.refresh_snapshot().await;
            }
        }
        "slideshow-faster" => {
            inner.slideshow.write().cycle_interval(true);
            inner.reschedule_slideshow_ticks();
            inner.refresh_snapshot().await;
        }
        "slideshow-slower" => {
            inner.slideshow.write().cycle_interval(false);
            inner.reschedule_slideshow_ticks();
            inner.refresh_snapshot().await;
        }
        "slideshow-random" => {
            let next = !inner.slideshow.read().random;
            {
                let nav = inner.image_nav.read().clone();
                let mut sl = inner.slideshow.write();
                sl.random = next;
                if next {
                    sl.rebuild_shuffle(&nav);
                }
            }
            inner.refresh_snapshot().await;
        }
        "slideshow-transition" => {
            let next = inner.slideshow.read().transition.cycle();
            inner.slideshow.write().transition = next;
            inner.refresh_snapshot().await;
        }
        "slideshow-trans-ms" => {
            inner.slideshow.write().cycle_transition_ms();
            inner.reschedule_slideshow_ticks();
            inner.refresh_snapshot().await;
        }
        "slideshow-overlay" => {
            let next = !inner.slideshow.read().overlay;
            inner.slideshow.write().overlay = next;
            if next {
                if let Some(path) = inner.path.read().as_ref() {
                    inner.slideshow.write().overlay_text = image_slideshow::overlay_for_path(path);
                }
            }
            inner.refresh_snapshot().await;
        }
        "slideshow-music" => {
            let current = inner.slideshow.read().music_path.clone();
            let path = inner.path.read().clone();
            let next = if let Some(path) = path {
                image_slideshow::next_folder_audio(&inner.deps.registry, &path, current.as_deref())
                    .await
            } else {
                None
            };
            inner.slideshow.write().music_path = next.as_ref().map(|p| p.as_str().to_string());
            if inner.slideshow.read().playing {
                match &next {
                    Some(p) => {
                        image_slideshow::start_music(p.as_str(), &mut inner.music_child.lock())
                    }
                    None => image_slideshow::stop_music(&mut inner.music_child.lock()),
                }
            }
            inner.refresh_snapshot().await;
        }
        "slideshow-export-html" | "slideshow-export-exe" | "slideshow-export-scr" => {
            inner.export_slideshow("pack").await?;
        }
        "slideshow-export-video" => inner.export_slideshow("video").await?,
        cmd if let Some(raw) = cmd.strip_prefix("slideshow-interval:") => {
            if let Ok(sec) = raw.parse::<u32>() {
                inner.slideshow.write().interval_ms = (sec * 1000).clamp(1000, 30_000);
                inner.refresh_snapshot().await;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("slideshow-music:") => {
            inner.slideshow.write().music_path = if raw.is_empty() {
                None
            } else {
                Some(raw.to_string())
            };
            if inner.slideshow.read().playing {
                if let Some(m) = inner.slideshow.read().music_path.clone() {
                    image_slideshow::start_music(&m, &mut inner.music_child.lock());
                }
            }
            inner.refresh_snapshot().await;
        }
        "meta-panel" => {
            let next = !inner.inspect.read().panel;
            inner.inspect.write().panel = next;
            inner.refresh_snapshot().await;
        }
        "meta-overlay" => {
            let next = !inner.inspect.read().overlay;
            inner.inspect.write().overlay = next;
            inner.refresh_snapshot().await;
        }
        "hist-mode" => {
            inner.inspect.write().cycle_hist_mode();
            inner.refresh_snapshot().await;
        }
        "gps-map" => {
            if let Some(url) = inner.inspect.read().gps_url() {
                let _ = opener::open(url);
            }
        }
        "meta-strip" => apply_viewer_meta(
            &inner,
            &orchid_viewers::EditableMeta {
                strip_all: true,
                ..orchid_viewers::EditableMeta::default()
            },
        )?,
        "meta-strip-gps" => apply_viewer_meta(
            &inner,
            &orchid_viewers::EditableMeta {
                strip_gps: true,
                ..orchid_viewers::EditableMeta::default()
            },
        )?,
        "meta-export-csv" => export_viewer_meta(&inner, false)?,
        "meta-export-xml" => export_viewer_meta(&inner, true)?,
        cmd if let Some(raw) = cmd.strip_prefix("meta-save:") => {
            apply_viewer_meta(&inner, &orchid_viewers::unpack_editable_meta(raw))?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("probe:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                    let changed = {
                        let Some(ViewerSnapshot::Image(s)) = inner.snapshot.read().clone() else {
                            return Ok(());
                        };
                        inner.inspect.write().probe_pixel(&s, x, y)
                    };
                    if changed {
                        if let Some(ViewerSnapshot::Image(s)) = inner.snapshot.write().as_mut() {
                            s.probe_text = inner.inspect.read().probe.clone();
                        }
                        inner.publish_refresh();
                    }
                }
            }
        }
        _ => inner.refresh_snapshot().await,
    }
    Ok(())
}

pub(crate) fn apply_viewer_meta(
    inner: &ViewerWidgetInner,
    edit: &orchid_viewers::EditableMeta,
) -> WidgetResult<()> {
    let Some(path) = inner.path.read().clone() else {
        return Ok(());
    };
    let os = path
        .to_local()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    orchid_viewers::apply_editable_meta(&os, edit)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    inner.schedule_inspect(&path);
    Ok(())
}

pub(crate) fn export_viewer_meta(inner: &ViewerWidgetInner, xml: bool) -> WidgetResult<()> {
    let Some(path) = inner.path.read().clone() else {
        return Ok(());
    };
    let os = path
        .to_local()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    let body = if xml {
        orchid_viewers::export_metadata_xml(std::slice::from_ref(&os))
    } else {
        orchid_viewers::export_metadata_csv(std::slice::from_ref(&os))
    }
    .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let dest = if xml {
        os.with_extension("metadata.xml")
    } else {
        os.with_extension("metadata.csv")
    };
    std::fs::write(&dest, body)
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    let _ = opener::open(&dest);
    Ok(())
}

/// Image: pan by logical pixels.
pub async fn image_pan(instance_id: Uuid, dx: f32, dy: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.pan(dx, dy);
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}
