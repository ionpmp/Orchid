//! [`ViewerWidgetInner`] open/refresh/snapshot internals.

use super::*;

impl ViewerWidgetInner {
    pub(super) fn publish_refresh(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    /// Prefer the user-facing path (`.orchid`) over a temp Raw unwrap path.
    pub(super) fn stamp_user_path(&self, snap: ViewerSnapshot) -> ViewerSnapshot {
        match self.path.read().as_ref() {
            Some(p) => snap.with_path_display(p.as_str()),
            None => snap,
        }
    }

    pub(super) fn take_unwrap_temp(&self) -> Option<std::path::PathBuf> {
        self.unwrap_temp.lock().take()
    }

    pub(super) fn drop_unwrap_temp(&self) {
        if let Some(p) = self.take_unwrap_temp() {
            let _ = std::fs::remove_file(&p);
        }
    }

    pub(super) async fn remember_current_image_view(&self) {
        let path = match self.path.read().clone() {
            Some(p) if is_image_path(&p) => p,
            _ => return,
        };
        let guard = self.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return;
        };
        let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
            return;
        };
        let (fit, transform) = img.capture_view();
        drop(guard);
        self.image_views
            .write()
            .insert(path.as_str().to_string(), SavedImageView { fit, transform });
    }

    pub(super) async fn restore_image_view(&self, path: &orchid_fs::FsPath) {
        let (saved, exact) = self.image_views.read().lookup(path.as_str());
        let Some(saved) = saved else {
            return;
        };
        let guard = self.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return;
        };
        let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
            return;
        };
        if exact {
            img.restore_view(saved.fit, saved.transform);
        } else if saved.fit.tracks_viewport() {
            img.set_fit_mode(saved.fit);
        } else {
            img.restore_zoom_only(saved.transform.zoom);
        }
    }

    /// Open a path: picks the right viewer kind, opens it, and caches the
    /// first snapshot.
    pub(super) async fn open_path(&self, path: orchid_fs::FsPath) -> WidgetResult<()> {
        self.anim_tick.fetch_add(1, Ordering::Relaxed);
        self.media_tick.fetch_add(1, Ordering::Relaxed);
        self.doc_autosave_gen.fetch_add(1, Ordering::Relaxed);
        self.remember_current_image_view().await;
        self.drop_unwrap_temp();
        let apply_passphrase = self.pending_decrypt_passphrase.lock().take();
        if apply_passphrase.is_none() {
            *self.pending_orchid_unlock.lock() = None;
            *self.orchid_passphrase_error.lock() = String::new();
        }
        let registry = self.deps.registry.clone();
        let highlighter = self.deps.highlighter.clone();
        *self.snapshot.write() = Some(ViewerSnapshot::Loading {
            path_display: path.as_str().to_string(),
        });
        *self.path.write() = Some(path.clone());
        self.publish_refresh();

        let select_res = orchid_viewers::select_viewer(
            &path,
            registry.clone(),
            highlighter,
            self.deps.chunk_store.as_deref(),
        )
        .await;
        let orchid_viewers::SelectedViewer {
            mut viewer,
            open_path,
            temp_cleanup,
        } = match select_res {
            Ok(v) => v,
            Err(e) => {
                let path_display = path.as_str().to_string();
                warn!(path = %path_display, error = %e, "viewer dispatch failed");
                *self.snapshot.write() = Some(ViewerSnapshot::Error {
                    path_display,
                    message: e.to_string(),
                });
                self.publish_refresh();
                return Ok(());
            }
        };
        *self.unwrap_temp.lock() = temp_cleanup;
        if let Some(store) = self.deps.chunk_store.clone() {
            if let Some(doc) = viewer
                .as_any_mut()
                .downcast_mut::<orchid_viewers::DocumentViewer>()
            {
                doc.set_chunk_store(store);
            }
        }
        if let Some(pw) = apply_passphrase {
            if let Some(doc) = viewer
                .as_any_mut()
                .downcast_mut::<orchid_viewers::DocumentViewer>()
            {
                doc.set_decrypt_identity(Some(orchid_crypto::Identity::passphrase(pw)));
            }
        }
        if is_image_path(&open_path) {
            let preloaded = self.image_preload.write().take(path.as_str());
            if let Some(loaded) = preloaded {
                if let Some(img) = viewer.as_any_mut().downcast_mut::<ImageViewer>() {
                    img.open_loaded(open_path.clone(), loaded);
                }
            } else if let Err(e) = viewer.open(open_path.clone(), registry).await {
                warn!(error = %e, "viewer open failed");
                self.drop_unwrap_temp();
                *self.snapshot.write() = Some(ViewerSnapshot::Error {
                    path_display: path.as_str().to_string(),
                    message: e.to_string(),
                });
                self.publish_refresh();
                return Ok(());
            }
        } else if let Err(e) = viewer.open(open_path.clone(), registry).await {
            let msg = e.to_string();
            let orchid_doc = open_path
                .to_local()
                .ok()
                .is_some_and(|p| orchid_viewers::is_orchid_path(std::path::Path::new(&p)))
                || path
                    .to_local()
                    .ok()
                    .is_some_and(|p| orchid_viewers::is_orchid_path(std::path::Path::new(&p)));
            if orchid_doc && orchid_viewers::is_orchid_identity_error(&msg) {
                warn!(error = %msg, "encrypted .orchid needs passphrase");
                self.drop_unwrap_temp();
                *self.viewer.lock().await = None;
                *self.pending_orchid_unlock.lock() = Some(path.clone());
                *self.orchid_passphrase_error.lock() =
                    if msg.to_ascii_lowercase().contains("invalid")
                        || msg.to_ascii_lowercase().contains("decryption failed")
                    {
                        "fm-passphrase-invalid".into()
                    } else {
                        String::new()
                    };
                *self.snapshot.write() = Some(ViewerSnapshot::Error {
                    path_display: path.as_str().to_string(),
                    message: "viewer-document-passphrase-required".into(),
                });
                self.publish_refresh();
                return Ok(());
            }
            warn!(error = %msg, "viewer open failed");
            self.drop_unwrap_temp();
            *self.snapshot.write() = Some(ViewerSnapshot::Error {
                path_display: path.as_str().to_string(),
                message: msg,
            });
            self.publish_refresh();
            return Ok(());
        }
        if self.pending_edit.swap(false, Ordering::Relaxed) {
            if let Some(tv) = viewer.as_any().downcast_ref::<TextViewer>() {
                tv.set_mode(orchid_viewers::TextViewerMode::Edit);
            }
        }
        let snap = self.stamp_user_path(viewer.snapshot());
        *self.snapshot.write() = Some(snap);
        *self.viewer.lock().await = Some(viewer);
        // Image/media chrome keys off the opened payload path (may be a temp
        // unwrap of a `.orchid` wrap). Folder nav stays on the user path only
        // when they are the same.
        if is_image_path(&open_path) {
            if open_path == path {
                self.after_image_opened(&path).await;
                self.restore_image_view(&path).await;
                self.attach_animation_if_needed(&path).await;
                self.schedule_thumbs_and_preload();
            } else {
                self.restore_image_view(&path).await;
            }
            let guard = self.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                *self.snapshot.write() = Some(self.stamp_user_path(v.snapshot()));
            }
        }
        if is_media_path(&open_path) {
            crate::builtin::audio_player::pause_all();
            crate::builtin::video_player::pause_all();
            self.after_media_opened(&open_path).await;
            self.schedule_media_ticks();
            #[cfg(windows)]
            smtc_publisher::set_active(self.instance_id);
        } else {
            #[cfg(windows)]
            smtc_publisher::clear_active(self.instance_id);
        }
        self.overlay_image_nav();
        *self.pending_orchid_unlock.lock() = None;
        *self.orchid_passphrase_error.lock() = String::new();
        self.publish_refresh();
        Ok(())
    }

    pub(super) async fn commit_orchid_passphrase(&self, passphrase: &str) -> WidgetResult<()> {
        let path = self.pending_orchid_unlock.lock().clone().ok_or_else(|| {
            WidgetError::InvalidStateForOperation("no pending orchid unlock".into())
        })?;
        *self.pending_decrypt_passphrase.lock() = Some(passphrase.to_string());
        *self.orchid_passphrase_error.lock() = String::new();
        self.open_path(path).await
    }

    pub(super) fn cancel_orchid_passphrase(&self) {
        *self.pending_orchid_unlock.lock() = None;
        *self.pending_decrypt_passphrase.lock() = None;
        *self.orchid_passphrase_error.lock() = String::new();
        self.publish_refresh();
    }

    pub(super) fn overlay_image_nav(&self) {
        let Some(snap) = self.snapshot.write().take() else {
            return;
        };
        *self.snapshot.write() = Some(apply_image_overlay(
            snap,
            &self.image_nav.read(),
            Some(&self.image_thumbs.read()),
            Some(&self.slideshow.read()),
            Some(&self.inspect.read()),
            Some(&self.media_nav.read()),
            self.playlist_panel_open.load(Ordering::Relaxed),
        ));
    }

    pub(super) async fn after_image_opened(&self, path: &orchid_fs::FsPath) {
        let parent = path.parent();
        let need_list = {
            let nav = self.image_nav.read();
            nav.folder.as_ref() != parent.as_ref() || !nav.siblings.iter().any(|p| p == path)
        };
        if need_list {
            if let Some(folder) = parent {
                if let Some(list) =
                    image_nav::list_image_siblings(&self.deps.registry, &folder).await
                {
                    self.image_nav.write().set_folder(folder, list, path);
                }
            }
        } else {
            self.image_nav.write().set_current(path);
        }
        self.image_nav.write().push_history(path);
        if self.slideshow.read().overlay {
            self.slideshow.write().overlay_text = image_slideshow::overlay_for_path(path);
        }
        self.schedule_inspect(path);
    }

    pub(super) async fn after_media_opened(&self, path: &orchid_fs::FsPath) {
        let parent = path.parent();
        let need_list = {
            let nav = self.media_nav.read();
            nav.folder.as_ref() != parent.as_ref() || !nav.siblings.iter().any(|p| p == path)
        };
        if need_list {
            if let Some(folder) = parent {
                if let Some(list) =
                    media_nav::list_media_siblings(&self.deps.registry, &folder).await
                {
                    self.media_nav.write().set_folder(folder, list, path);
                }
            }
        } else {
            self.media_nav.write().set_current(path);
        }
        self.media_nav.write().push_history(path);
        self.apply_media_playlist_overlay().await;
    }

    pub(super) async fn apply_media_playlist_overlay(&self) {
        let (idx, count, shuffle, loop_playlist) = {
            let nav = self.media_nav.read();
            (
                nav.index as u32,
                nav.siblings.len() as u32,
                nav.shuffle,
                nav.loop_playlist,
            )
        };
        let guard = self.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                media.set_playlist_info(idx, count, shuffle, loop_playlist);
            }
        }
    }

    pub(super) async fn navigate_media(&self, step: media_nav::MediaNavStep) -> WidgetResult<()> {
        {
            let path = self.path.read().clone();
            if let Some(path) = path.as_ref() {
                if is_media_path(path) {
                    let need = self.media_nav.read().siblings.is_empty();
                    if need {
                        self.after_media_opened(path).await;
                    }
                }
            }
        }
        let idx = self.media_nav.read().pick(step);
        let Some(idx) = idx else {
            return Ok(());
        };
        let Some(next) = self.media_nav.read().siblings.get(idx).cloned() else {
            return Ok(());
        };
        self.media_nav.write().index = idx;
        self.open_path(next).await
    }

    pub(super) fn schedule_media_ticks(&self) {
        let gen = self.media_tick.fetch_add(1, Ordering::Relaxed) + 1;
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        tokio::spawn(async move {
            loop {
                if inner.media_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                let tick = {
                    let guard = inner.viewer.lock().await;
                    if let Some(v) = guard.as_ref() {
                        if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                            Some((media.take_dirty(), media.take_eof(), media.is_playing()))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                let Some((dirty, eof, playing)) = tick else {
                    return;
                };
                if eof {
                    let _ = inner.navigate_media(media_nav::MediaNavStep::Next).await;
                    continue;
                }
                if dirty {
                    // Re-apply playlist chrome then publish.
                    let (idx, count, shuffle, loop_playlist) = {
                        let nav = inner.media_nav.read();
                        (
                            nav.index as u32,
                            nav.siblings.len() as u32,
                            nav.shuffle,
                            nav.loop_playlist,
                        )
                    };
                    {
                        let guard = inner.viewer.lock().await;
                        if let Some(v) = guard.as_ref() {
                            if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                                media.set_playlist_info(idx, count, shuffle, loop_playlist);
                            }
                            let snap = apply_image_overlay(
                                v.snapshot(),
                                &inner.image_nav.read(),
                                Some(&inner.image_thumbs.read()),
                                Some(&inner.slideshow.read()),
                                Some(&inner.inspect.read()),
                                Some(&inner.media_nav.read()),
                                inner.playlist_panel_open.load(Ordering::Relaxed),
                            );
                            #[cfg(windows)]
                            if let ViewerSnapshot::Media(ref m) = snap {
                                smtc_publisher::publish(inner.instance_id, m);
                            }
                            *inner.snapshot.write() = Some(snap);
                        }
                    }
                    inner.publish_refresh();
                }
                // ~30 Hz while playing (frames + progress); idle slower when paused.
                let wait_ms = if playing { 33 } else { 200 };
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                if inner.media_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
            }
        });
    }

    pub(super) fn schedule_inspect(&self, path: &orchid_fs::FsPath) {
        let gen = self.inspect_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let path = path.clone();
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        let snap = match self.snapshot.read().clone() {
            Some(ViewerSnapshot::Image(s)) => Some(s),
            _ => None,
        };
        tokio::task::spawn_blocking(move || {
            if inner.inspect_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let inspect = match image_inspect::inspect_local(&path) {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "image inspect failed");
                    return;
                }
            };
            if inner.inspect_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let hist = snap.as_ref().map(image_inspect::histogram_from_snap);
            {
                let mut st = inner.inspect.write();
                st.apply_inspect(path.as_str(), inspect, snap.as_ref());
                if let Some(h) = hist {
                    st.set_histogram(h);
                }
            }
            inner.publish_refresh();
        });
    }

    pub(super) async fn navigate_images(&self, step: image_nav::NavStep) -> WidgetResult<()> {
        {
            let path = self.path.read().clone();
            if let Some(path) = path.as_ref() {
                if is_image_path(path) {
                    let need = {
                        let nav = self.image_nav.read();
                        nav.siblings.is_empty()
                    };
                    if need {
                        self.after_image_opened(path).await;
                    }
                }
            }
        }
        self.capture_slide_prev();
        let use_shuffle = {
            let sl = self.slideshow.read();
            sl.playing && sl.random && matches!(step, image_nav::NavStep::Next)
        };
        let n = self.image_nav.read().siblings.len().max(1);
        for _ in 0..n {
            let idx = if use_shuffle {
                let nav = self.image_nav.read().clone();
                self.slideshow.write().next_shuffled(&nav)
            } else {
                self.image_nav.read().pick(step)
            };
            let Some(idx) = idx else {
                if self.slideshow.read().playing && !self.image_nav.read().loop_playlist {
                    self.stop_slideshow();
                }
                return Ok(());
            };
            let Some(next) = self.image_nav.read().siblings.get(idx).cloned() else {
                return Ok(());
            };
            self.open_path(next.clone()).await?;
            let failed = matches!(*self.snapshot.read(), Some(ViewerSnapshot::Error { .. }));
            if failed {
                let mut nav = self.image_nav.write();
                nav.index = idx;
                nav.mark_unreadable(&next);
                continue;
            }
            return Ok(());
        }
        Ok(())
    }

    pub(super) async fn close_viewer(&self) {
        self.stop_slideshow();
        #[cfg(windows)]
        smtc_publisher::clear_active(self.instance_id);
        let taken = self.viewer.lock().await.take();
        if let Some(mut v) = taken {
            let _ = v.close().await;
        }
        self.drop_unwrap_temp();
        *self.snapshot.write() = None;
        *self.path.write() = None;
        self.publish_refresh();
    }

    pub(super) async fn refresh_snapshot(&self) {
        let guard = self.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            *self.snapshot.write() = Some(apply_image_overlay(
                self.stamp_user_path(v.snapshot()),
                &self.image_nav.read(),
                Some(&self.image_thumbs.read()),
                Some(&self.slideshow.read()),
                Some(&self.inspect.read()),
                Some(&self.media_nav.read()),
                self.playlist_panel_open.load(Ordering::Relaxed),
            ));
        }
        drop(guard);
        self.publish_refresh();
    }

    pub(super) fn schedule_thumbs_and_preload(&self) {
        let gen = self.thumb_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        tokio::spawn(async move {
            inner.refresh_thumbs(gen).await;
            inner.preload_ahead(gen).await;
        });
    }

    pub(super) async fn refresh_thumbs(&self, gen: u64) {
        let Some(service) = self.deps.thumbnails.clone() else {
            return;
        };
        if self.thumb_gen.load(Ordering::Relaxed) != gen {
            return;
        }
        let (siblings, current, size) = {
            let nav = self.image_nav.read();
            let thumbs = self.image_thumbs.read();
            (
                nav.siblings.clone(),
                nav.siblings.get(nav.index).cloned(),
                thumbs.size,
            )
        };
        let current_key = current.as_ref().map(|p| p.as_str().to_string());
        let mut items = Vec::with_capacity(siblings.len().min(256));
        for (i, path) in siblings.iter().take(256).enumerate() {
            if self.thumb_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let meta = image_thumbs::sibling_meta(&self.deps.registry, path).await;
            let key = ThumbnailService::cache_key(path, meta.modified_ms);
            let thumb =
                image_thumbs::load_one_thumb(&service, &self.deps.registry, path, key, size).await;
            items.push(ImageThumbItem {
                path: path.as_str().to_string(),
                name: path.file_name().unwrap_or_default().to_string(),
                size_bytes: meta.size_bytes,
                date_text: meta.date_text,
                rating: meta.rating,
                rgba: thumb.as_ref().map(|t| Arc::clone(&t.rgba)),
                width: thumb.as_ref().map(|t| t.width).unwrap_or(0),
                height: thumb.as_ref().map(|t| t.height).unwrap_or(0),
                selected: current_key.as_deref() == Some(path.as_str()),
                index: (i + 1) as u32,
                taken_ms: meta.taken_ms,
                has_gps: meta.has_gps,
                gps_lat: meta.gps_lat,
                gps_lon: meta.gps_lon,
            });
            if items.len() % 8 == 0 {
                if self.thumb_gen.load(Ordering::Relaxed) != gen {
                    return;
                }
                self.image_thumbs.write().items = items.clone();
                self.overlay_image_nav();
                self.publish_refresh();
            }
        }
        if self.thumb_gen.load(Ordering::Relaxed) != gen {
            return;
        }
        self.image_thumbs.write().items = items;
        self.overlay_image_nav();
        self.publish_refresh();
    }

    pub(super) async fn preload_ahead(&self, gen: u64) {
        if self.thumb_gen.load(Ordering::Relaxed) != gen {
            return;
        }
        let (n, paths) = {
            let thumbs = self.image_thumbs.read();
            let nav = self.image_nav.read();
            (
                thumbs.preload_n as usize,
                image_thumbs::preload_paths(&nav, thumbs.preload_n as usize),
            )
        };
        if n == 0 {
            return;
        }
        let cap = n.saturating_mul(2).saturating_add(2);
        for path in paths {
            if self.thumb_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            if self.image_preload.read().contains(path.as_str()) {
                continue;
            }
            let registry = Arc::clone(&self.deps.registry);
            if let Some((key, img)) = image_thumbs::preload_one(registry, path).await {
                if self.thumb_gen.load(Ordering::Relaxed) != gen {
                    return;
                }
                self.image_preload.write().insert(key, img, cap);
            }
        }
    }

    pub(super) async fn write_contact_sheet(&self) -> WidgetResult<()> {
        let Some(service) = self.deps.thumbnails.clone() else {
            return Err(WidgetError::InvalidStateForOperation(
                "thumbnail cache unavailable".into(),
            ));
        };
        let nav = self.image_nav.read().clone();
        let size = self.image_thumbs.read().size;
        let dest = image_thumbs::write_contact_sheet(&service, &self.deps.registry, &nav, size)
            .await
            .map_err(WidgetError::InvalidStateForOperation)?;
        self.open_path(dest).await
    }

    pub(super) fn forget_thumb_memory(&self, path: &orchid_fs::FsPath) {
        self.image_preload.write().forget(path.as_str());
        self.image_thumbs
            .write()
            .items
            .retain(|t| t.path != path.as_str());
    }

    pub(super) fn capture_slide_prev(&self) {
        if !self.slideshow.read().playing {
            return;
        }
        let Some(ViewerSnapshot::Image(s)) = self.snapshot.read().clone() else {
            return;
        };
        let mut sl = self.slideshow.write();
        sl.prev_rgba = Some(s.rgba_bytes);
        sl.prev_w = s.width_px;
        sl.prev_h = s.height_px;
        sl.gen = 0;
        sl.elapsed_ms = 0;
    }

    pub(super) fn patch_slide_clock(&self) {
        let gen = self.slideshow.read().gen;
        if let Some(ViewerSnapshot::Image(s)) = self.snapshot.write().as_mut() {
            s.slideshow_gen = gen;
        }
        self.publish_refresh();
    }

    pub(super) fn stop_slideshow(&self) {
        self.slide_tick.fetch_add(1, Ordering::Relaxed);
        {
            let mut sl = self.slideshow.write();
            sl.playing = false;
            sl.paused = false;
            sl.prev_rgba = None;
        }
        image_slideshow::stop_music(&mut self.music_child.lock());
    }

    pub(super) fn schedule_slideshow_ticks(&self) {
        let gen = self.slide_tick.fetch_add(1, Ordering::Relaxed) + 1;
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        tokio::spawn(async move {
            loop {
                if inner.slide_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                let (playing, paused, interval, trans_ms, slide_gen, elapsed) = {
                    let sl = inner.slideshow.read();
                    (
                        sl.playing,
                        sl.paused,
                        sl.interval_ms,
                        sl.transition_ms,
                        sl.gen,
                        sl.elapsed_ms,
                    )
                };
                if !playing {
                    return;
                }
                let wait = image_slideshow::slideshow_wait_ms(
                    paused, elapsed, interval, trans_ms, slide_gen,
                );
                tokio::time::sleep(std::time::Duration::from_millis(u64::from(wait))).await;
                if inner.slide_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                if paused {
                    continue;
                }
                let (elapsed, bumped) = {
                    let mut sl = inner.slideshow.write();
                    sl.elapsed_ms = sl.elapsed_ms.saturating_add(wait);
                    let bumped = if sl.gen.saturating_mul(50) < trans_ms.max(1) {
                        sl.gen = sl.gen.saturating_add((wait / 50).max(1));
                        true
                    } else {
                        false
                    };
                    (sl.elapsed_ms, bumped)
                };
                if bumped {
                    inner.patch_slide_clock();
                }
                if elapsed >= interval.max(400) {
                    inner.slideshow.write().elapsed_ms = 0;
                    if let Err(e) = inner.navigate_images(image_nav::NavStep::Next).await {
                        warn!(error = %e, "slideshow advance failed");
                        return;
                    }
                }
            }
        });
    }

    pub(super) fn reschedule_slideshow_ticks(&self) {
        if self.slideshow.read().playing {
            self.schedule_slideshow_ticks();
        }
    }

    pub(super) async fn attach_animation_if_needed(&self, path: &orchid_fs::FsPath) {
        let has_anim = {
            let guard = self.viewer.lock().await;
            guard
                .as_ref()
                .and_then(|v| v.as_any().downcast_ref::<ImageViewer>())
                .is_some_and(|img| img.anim_count() >= 2)
        };
        if !has_anim {
            let ext = path.extension().unwrap_or_default();
            if is_animation_extension(ext) {
                let os = path.to_local().ok();
                let seq = if let Some(os) = os {
                    tokio::task::spawn_blocking(move || load_animation_file(&os))
                        .await
                        .ok()
                        .flatten()
                } else {
                    None
                };
                let guard = self.viewer.lock().await;
                if let Some(v) = guard.as_ref() {
                    if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
                        img.attach_anim(seq);
                    }
                }
            }
        }
        self.schedule_anim_ticks();
    }

    pub(super) fn schedule_anim_ticks(&self) {
        let gen = self.anim_tick.fetch_add(1, Ordering::Relaxed) + 1;
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        tokio::spawn(async move {
            loop {
                if inner.anim_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                let delay = {
                    if inner.slideshow.read().playing {
                        None
                    } else {
                        let guard = inner.viewer.lock().await;
                        guard.as_ref().and_then(|v| {
                            let img = v.as_any().downcast_ref::<ImageViewer>()?;
                            if img.anim_count() < 2 || !img.anim_playing() {
                                None
                            } else {
                                Some(img.anim_delay_ms())
                            }
                        })
                    }
                };
                match delay {
                    None => {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    Some(ms) => {
                        tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms.max(20))))
                            .await;
                    }
                }
                if inner.anim_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                if inner.slideshow.read().playing {
                    continue;
                }
                let advanced = {
                    let guard = inner.viewer.lock().await;
                    if let Some(v) = guard.as_ref() {
                        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
                            if img.anim_playing() {
                                img.anim_advance();
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if advanced {
                    inner.refresh_snapshot().await;
                }
            }
        });
    }

    /// Debounced save for dirty linked `.orchid` documents (ChunkStore present).
    pub(super) fn schedule_doc_autosave(&self) {
        if self.deps.chunk_store.is_none() {
            return;
        }
        let gen = self.doc_autosave_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
            return;
        };
        let inner = Arc::clone(entry.value());
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(2_000)).await;
            if inner.doc_autosave_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let save_res = {
                let mut guard = inner.viewer.lock().await;
                let Some(v) = guard.as_mut() else {
                    return;
                };
                let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
                    return;
                };
                if !doc.is_dirty() {
                    return;
                }
                let Some(path) = doc.path_clone() else {
                    return;
                };
                let Ok(os_path) = path.to_local() else {
                    return;
                };
                if !orchid_viewers::is_orchid_path(std::path::Path::new(&os_path)) {
                    return;
                }
                v.save().await
            };
            if let Err(e) = save_res {
                warn!(error = %e, "linked .orchid autosave failed");
                return;
            }
            if inner.doc_autosave_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            inner.refresh_snapshot().await;
        });
    }

    pub(super) async fn start_slideshow(&self) -> WidgetResult<()> {
        {
            let path = self.path.read().clone();
            if let Some(path) = path.as_ref() {
                if is_image_path(path) {
                    let need = self.image_nav.read().siblings.is_empty();
                    if need {
                        self.after_image_opened(path).await;
                    }
                }
            }
        }
        let music = {
            let existing = self.slideshow.read().music_path.clone();
            let current = self.path.read().clone();
            if existing.is_some() {
                existing
            } else if let Some(path) = current {
                image_slideshow::first_folder_audio(&self.deps.registry, &path)
                    .await
                    .map(|p| p.as_str().to_string())
            } else {
                None
            }
        };
        {
            let guard = self.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
                    img.set_anim_playing(false);
                }
            }
        }
        {
            let nav = self.image_nav.read().clone();
            let mut sl = self.slideshow.write();
            sl.playing = true;
            sl.paused = false;
            sl.elapsed_ms = 0;
            sl.gen = 0;
            sl.music_path = music.clone();
            if sl.random {
                sl.rebuild_shuffle(&nav);
            }
            if sl.overlay {
                if let Some(path) = self.path.read().as_ref() {
                    sl.overlay_text = image_slideshow::overlay_for_path(path);
                }
            }
        }
        if let Some(m) = music {
            image_slideshow::start_music(&m, &mut self.music_child.lock());
        }
        self.schedule_slideshow_ticks();
        self.refresh_snapshot().await;
        Ok(())
    }

    pub(super) async fn toggle_slideshow(&self) -> WidgetResult<()> {
        if self.slideshow.read().playing {
            self.stop_slideshow();
            self.refresh_snapshot().await;
            Ok(())
        } else {
            self.start_slideshow().await
        }
    }

    pub(super) async fn export_slideshow(&self, kind: &str) -> WidgetResult<()> {
        let nav = self.image_nav.read().clone();
        let slide = self.slideshow.read().clone();
        let dest = match kind {
            "video" => {
                tokio::task::spawn_blocking(move || image_slideshow::write_video(&nav, &slide))
                    .await
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            }
            _ => tokio::task::spawn_blocking(move || image_slideshow::write_pack(&nav, &slide))
                .await
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?,
        }
        .map_err(WidgetError::InvalidStateForOperation)?;
        let _ = opener::open(&dest);
        Ok(())
    }

    pub(super) async fn apply_lossless_path(
        &self,
        path: &orchid_fs::FsPath,
        op: LosslessOp,
    ) -> WidgetResult<()> {
        let provider = self.deps.registry.for_path(path).ok_or_else(|| {
            WidgetError::InvalidStateForOperation(format!("no provider for {}", path.as_str()))
        })?;
        let bytes = provider
            .read(path)
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let fmt = format_from_extension(path.extension());
        let out = tokio::task::spawn_blocking(move || apply_lossless(&bytes, fmt, op))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .map_err(map_viewer_err)?;
        provider
            .write(path, &out)
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        Ok(())
    }

    pub(super) async fn reopen_after_lossless(&self, path: orchid_fs::FsPath) -> WidgetResult<()> {
        self.image_views.write().forget(path.as_str());
        self.forget_thumb_memory(&path);
        *self.path.write() = None;
        self.open_path(path).await
    }

    pub(super) async fn apply_lossless_current(&self, op: LosslessOp) -> WidgetResult<()> {
        let Some(path) = self.path.read().clone() else {
            return Ok(());
        };
        self.apply_lossless_path(&path, op).await?;
        self.reopen_after_lossless(path).await
    }

    pub(super) async fn apply_lossless_folder(&self, op: LosslessOp) -> WidgetResult<()> {
        let siblings = self.image_nav.read().siblings.clone();
        let current = self.path.read().clone();
        self.image_preload.write().clear();
        self.image_thumbs.write().items.clear();
        let mut ok = 0usize;
        let mut last_err: Option<WidgetError> = None;
        for path in &siblings {
            match self.apply_lossless_path(path, op).await {
                Ok(()) => ok += 1,
                Err(e) => last_err = Some(e),
            }
        }
        if let Some(path) = current {
            self.reopen_after_lossless(path).await?;
        }
        if ok == 0 {
            if let Some(e) = last_err {
                return Err(e);
            }
        }
        Ok(())
    }

    pub(super) async fn apply_lossless_crop(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    ) -> WidgetResult<()> {
        let crop = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            img.viewport_rect_to_image_crop(x0, y0, x1, y1)
        };
        let Some((x, y, w, h)) = crop else {
            return Ok(());
        };
        self.apply_lossless_current(LosslessOp::Crop { x, y, w, h })
            .await
    }

    pub(super) async fn apply_edit_current(&self, op: EditOp) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let img = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(viewer) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            viewer.clone_loaded()
        };
        let Some(img) = img else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let suffix = match &op {
            EditOp::Crop { .. } => "crop",
            EditOp::Resize { .. } => "resize",
            EditOp::Canvas { .. } => "canvas",
            EditOp::Perspective { .. } => "perspective",
            EditOp::Straighten { .. } | EditOp::AutoStraighten => "straighten",
        };
        let dest = tokio::task::spawn_blocking(move || {
            let out = apply_edit(&img, &op)?;
            orchid_viewers::save_sibling(&os, &out, suffix)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    pub(super) async fn export_anim_frames(&self) -> WidgetResult<()> {
        let plan = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            img.anim_export_plan()
        };
        let Some((os, seq)) = plan else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || orchid_viewers::export_anim_frames(&os, &seq.frames))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .map_err(map_viewer_err)?;
        self.refresh_snapshot().await;
        Ok(())
    }

    pub(super) async fn extract_anim_frame(&self) -> WidgetResult<()> {
        let plan = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            img.anim_extract_plan()
        };
        let Some((os, frame, suffix)) = plan else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || {
            orchid_viewers::export_anim_frame(&os, &frame, &suffix)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        self.refresh_snapshot().await;
        Ok(())
    }

    pub(super) async fn apply_adjust_current(&self, op: AdjustOp) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let img = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(viewer) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            viewer.clone_loaded()
        };
        let Some(img) = img else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let suffix = op.suffix();
        let dest = tokio::task::spawn_blocking(move || {
            if os
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(orchid_viewers::is_raw_file_extension)
            {
                let _ = img;
                orchid_viewers::apply_adjust_file(&os, &op)
            } else {
                let out = apply_adjust(&img, &op)?;
                orchid_viewers::save_sibling(&os, &out, suffix)
            }
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    pub(super) async fn apply_filter_current(&self, op: FilterOp) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let img = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(viewer) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            viewer.clone_loaded()
        };
        let Some(img) = img else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let suffix = op.suffix();
        let dest = tokio::task::spawn_blocking(move || {
            let out = apply_filter(&img, &op)?;
            orchid_viewers::save_sibling(&os, &out, suffix)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    pub(super) async fn apply_annotate_current(&self, op: AnnotateOp) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let dest =
            tokio::task::spawn_blocking(move || orchid_viewers::apply_annotate_file(&os, &op))
                .await
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
                .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    pub(super) async fn print_job(
        &self,
        raw: &str,
        preview: bool,
        folder: bool,
    ) -> WidgetResult<()> {
        let spec = if raw.trim().is_empty() {
            orchid_viewers::PrintSpec::default()
        } else {
            parse_print_line(raw).ok_or_else(|| {
                WidgetError::InvalidStateForOperation("could not parse print spec".into())
            })?
        };
        let paths = if folder {
            self.image_nav.read().siblings.clone()
        } else {
            self.path.read().clone().into_iter().collect()
        };
        let os: Vec<std::path::PathBuf> = paths.iter().filter_map(|p| p.to_local().ok()).collect();
        if os.is_empty() {
            return Ok(());
        }
        let hint = os[0].clone();
        let dests = tokio::task::spawn_blocking(move || {
            let refs: Vec<&std::path::Path> = os.iter().map(std::path::PathBuf::as_path).collect();
            if preview {
                orchid_viewers::write_print_preview(&refs, &spec, &hint).map(|p| vec![p])
            } else {
                orchid_viewers::write_print_temps(&refs, &spec)
            }
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        if preview {
            if let Some(dest) = dests.first() {
                let next = orchid_fs::FsPath::from_local(dest)
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                return self.open_path(next).await;
            }
            return Ok(());
        }
        for dest in dests {
            orchid_viewers::send_to_printer(&dest).map_err(map_viewer_err)?;
        }
        Ok(())
    }

    pub(super) async fn export_current(&self, raw: &str) -> WidgetResult<()> {
        let spec = if raw.trim().is_empty() {
            ExportSpec::default()
        } else {
            parse_export_line(raw).ok_or_else(|| {
                WidgetError::InvalidStateForOperation("could not parse export spec".into())
            })?
        };
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let dest = tokio::task::spawn_blocking(move || export_file(&os, &spec))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    pub(super) async fn copy_image(&self) -> WidgetResult<()> {
        let img = {
            let guard = self.viewer.lock().await;
            guard.as_ref().and_then(|v| {
                v.as_any()
                    .downcast_ref::<ImageViewer>()
                    .and_then(|img| img.clone_loaded())
            })
        };
        let Some(img) = img else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || crate::builtin::file_manager::copy_loaded(&img))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))??;
        Ok(())
    }

    pub(super) async fn paste_image(&self) -> WidgetResult<()> {
        let hint = self
            .path
            .read()
            .as_ref()
            .and_then(|p| p.to_local().ok())
            .unwrap_or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|h| {
                        std::path::PathBuf::from(h)
                            .join("Pictures")
                            .join("clipboard.png")
                    })
                    .unwrap_or_else(|| std::env::temp_dir().join("clipboard.png"))
            });
        let dest = tokio::task::spawn_blocking(move || {
            let img = crate::builtin::file_manager::paste_loaded()?;
            let dest = unique_export_dest(&hint, "paste", "png");
            let bytes = encode_png(&img)
                .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
            std::fs::write(&dest, bytes)
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            Ok::<_, WidgetError>(dest)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))??;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    pub(super) async fn set_current_wallpaper(&self) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        tokio::task::spawn_blocking(move || {
            let wall = {
                let ext = os
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(ext.as_str(), "jpg" | "jpeg" | "bmp") {
                    os
                } else {
                    export_file(
                        &os,
                        &ExportSpec {
                            format: ExportFormat::Jpeg,
                            quality: 92,
                            ..ExportSpec::default()
                        },
                    )
                    .map_err(map_viewer_err)?
                }
            };
            set_wallpaper(&wall).map_err(map_viewer_err)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))??;
        Ok(())
    }

    pub(super) async fn email_current(&self, raw: &str) -> WidgetResult<()> {
        let max = raw
            .split('|')
            .find_map(|p| p.trim().strip_prefix("max=")?.trim().parse::<u32>().ok())
            .or_else(|| raw.trim().parse().ok())
            .unwrap_or(1920);
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let eml = tokio::task::spawn_blocking(move || {
            let jpeg = prepare_mail_attachment(&os, max)?;
            write_mail_eml(&jpeg)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        let _ = opener::open(&eml);
        Ok(())
    }

    pub(super) async fn share_current(&self, raw: &str) -> WidgetResult<()> {
        let network = raw
            .split('|')
            .next()
            .unwrap_or(raw)
            .trim()
            .to_ascii_lowercase();
        self.copy_image().await?;
        let label = self
            .path
            .read()
            .as_ref()
            .and_then(|p| p.file_name())
            .unwrap_or("image")
            .to_string();
        if let Some(url) = share_intent_url(&network, &label) {
            let _ = opener::open(url);
        } else if let Some(src) = self.path.read().clone() {
            if let Ok(os) = src.to_local() {
                let _ = opener::open(os);
            }
        }
        Ok(())
    }

    pub(super) async fn screenshot_current(&self, raw: &str) -> WidgetResult<()> {
        let spec = parse_screenshot_line(raw).ok_or_else(|| {
            WidgetError::InvalidStateForOperation("could not parse screenshot spec".into())
        })?;
        let dir = self
            .path
            .read()
            .as_ref()
            .and_then(|p| p.to_local().ok())
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|h| std::path::PathBuf::from(h).join("Pictures"))
            })
            .unwrap_or_else(std::env::temp_dir);
        let dest = tokio::task::spawn_blocking(move || write_screenshot(&dir, &spec))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    pub(super) async fn annotate_from_view(&self, raw: &str) -> WidgetResult<()> {
        let Some((kind, rest)) = raw.split_once(':') else {
            return Ok(());
        };
        let (coords, tail) = rest.split_once(" | ").unwrap_or((rest, ""));
        let img_pts = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            let mut out = Vec::new();
            if kind == "pen" || kind == "poly" {
                for pair in coords.split([';', ' ']) {
                    let Some((x, y)) = pair.split_once(',') else {
                        continue;
                    };
                    if let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) {
                        if let Some(p) = img.viewport_to_image(x, y) {
                            out.push(p);
                        }
                    }
                }
            } else {
                let nums: Vec<f32> = coords.split(':').filter_map(|s| s.parse().ok()).collect();
                for pair in nums.chunks(2) {
                    if pair.len() == 2 {
                        if let Some(p) = img.viewport_to_image(pair[0], pair[1]) {
                            out.push(p);
                        }
                    }
                }
            }
            out
        };
        let packed = match kind {
            "line" | "arrow" if img_pts.len() >= 2 => format!(
                "{kind}={},{},{},{}{tail_part}",
                img_pts[0].0,
                img_pts[0].1,
                img_pts[1].0,
                img_pts[1].1,
                tail_part = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" | {tail}")
                }
            ),
            "rect" | "ellipse" | "highlight" | "privacy" if img_pts.len() >= 2 => {
                let x = img_pts[0].0.min(img_pts[1].0);
                let y = img_pts[0].1.min(img_pts[1].1);
                let w = (img_pts[0].0 - img_pts[1].0).abs();
                let h = (img_pts[0].1 - img_pts[1].1).abs();
                let extra = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" | {tail}")
                };
                format!("{kind}={x},{y},{w},{h}{extra}")
            }
            "text" | "callout" if !img_pts.is_empty() => {
                let extra = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" | {tail}")
                };
                format!("x={} | y={}{extra}", img_pts[0].0, img_pts[0].1)
            }
            "pen" | "poly" if img_pts.len() >= 2 => {
                let pts = img_pts
                    .iter()
                    .map(|(x, y)| format!("{x},{y}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let extra = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" | {tail}")
                };
                format!("{kind}={pts}{extra}")
            }
            _ => return Ok(()),
        };
        if let Some(op) = parse_annotate_line(&packed) {
            self.apply_annotate_current(op).await?;
        }
        Ok(())
    }

    pub(super) async fn edit_crop_from_view(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        aspect: f32,
        keep: u8,
    ) -> WidgetResult<()> {
        let crop = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            img.viewport_rect_to_image_crop(x0, y0, x1, y1)
        };
        let Some((x, y, w, h)) = crop else {
            return Ok(());
        };
        self.apply_edit_current(EditOp::Crop {
            x,
            y,
            w,
            h,
            aspect: (aspect > 0.05).then_some(aspect),
            keep: match keep {
                1 => CropKeep::Width,
                2 => CropKeep::Height,
                _ => CropKeep::None,
            },
        })
        .await
    }

    pub(super) async fn edit_line_from_view(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        perspective: bool,
        extra: &[(f32, f32)],
    ) -> WidgetResult<()> {
        let pts = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            let mut out = Vec::new();
            for (x, y) in [(x0, y0), (x1, y1)]
                .into_iter()
                .chain(extra.iter().copied())
            {
                if let Some(p) = img.viewport_to_image(x, y) {
                    out.push(p);
                }
            }
            out
        };
        if perspective {
            if pts.len() < 4 {
                return Ok(());
            }
            return self
                .apply_edit_current(EditOp::Perspective {
                    quad: [pts[0], pts[1], pts[2], pts[3]],
                })
                .await;
        }
        if pts.len() < 2 {
            return Ok(());
        }
        self.apply_edit_current(EditOp::Straighten {
            x0: pts[0].0,
            y0: pts[0].1,
            x1: pts[1].0,
            y1: pts[1].1,
        })
        .await
    }
}
