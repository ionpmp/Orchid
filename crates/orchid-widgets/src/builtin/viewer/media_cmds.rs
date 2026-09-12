//! Media viewer commands.

use super::*;

/// Dispatch a media-player command (`play`, `pause`, seek tokens, …).
pub async fn media_command(instance_id: Uuid, command: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    match command {
        "next" => {
            inner.navigate_media(media_nav::MediaNavStep::Next).await?;
            return Ok(());
        }
        "prev" => {
            inner.navigate_media(media_nav::MediaNavStep::Prev).await?;
            return Ok(());
        }
        "first" => {
            inner.navigate_media(media_nav::MediaNavStep::First).await?;
            return Ok(());
        }
        "last" => {
            inner.navigate_media(media_nav::MediaNavStep::Last).await?;
            return Ok(());
        }
        "loop" => {
            let next = !inner.media_nav.read().loop_playlist;
            inner.media_nav.write().loop_playlist = next;
            inner.refresh_snapshot().await;
            return Ok(());
        }
        "shuffle" => {
            let next = !inner.media_nav.read().shuffle;
            inner.media_nav.write().shuffle = next;
            inner.refresh_snapshot().await;
            return Ok(());
        }
        "playlist-toggle" => {
            let next = !inner.playlist_panel_open.load(Ordering::Relaxed);
            inner.playlist_panel_open.store(next, Ordering::Relaxed);
            orchid_viewers::persist_media_playlist_panel(next);
            {
                let (w, h) = *inner.media_viewport.read();
                if w > 0.0 && h > 0.0 {
                    let guard = inner.viewer.lock().await;
                    if let Some(v) = guard.as_ref() {
                        if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                            media.set_viewport(w, h, next);
                        }
                    }
                }
            }
            inner.refresh_snapshot().await;
            return Ok(());
        }
        "random" => {
            inner
                .navigate_media(media_nav::MediaNavStep::Random)
                .await?;
            return Ok(());
        }
        cmd if let Some(raw) = cmd.strip_prefix("goto:") => {
            if let Ok(n) = raw.parse::<usize>() {
                inner
                    .navigate_media(media_nav::MediaNavStep::Goto(n))
                    .await?;
            }
            return Ok(());
        }
        "fullscreen" | "kiosk" | "exit-immersive" | "next-monitor" => {
            // Handled at the UI window layer; still accept no-op here.
            return Ok(());
        }
        _ => {}
    }
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(media) = v.as_any().downcast_ref::<MediaViewer>() else {
            return Ok(());
        };
        if command == "play-pause" && !media.is_playing() {
            crate::builtin::audio_player::pause_all();
            crate::builtin::video_player::pause_all();
        } else if command == "play" {
            crate::builtin::audio_player::pause_all();
            crate::builtin::video_player::pause_all();
        }
        media.apply_command(command);
    }
    inner.schedule_media_ticks();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Seek media to a 0..1 progress fraction.
pub async fn media_seek_frac(instance_id: Uuid, frac: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(media) = v.as_any().downcast_ref::<MediaViewer>() else {
            return Ok(());
        };
        media.seek_fraction(f64::from(frac));
    }
    inner.schedule_media_ticks();
    inner.refresh_snapshot().await;
    Ok(())
}
