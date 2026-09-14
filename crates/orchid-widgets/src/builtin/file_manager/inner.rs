//! [`FileManagerInner`] listing, watches, decorations, and transfers.

use super::*;

impl FileManagerInner {
    pub(super) fn publish_refresh(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    /// Coalesce icon/thumb snapshot publishes so scroll/hover are not fighting
    /// a frame rebuild on every batch of shell icons.
    pub(super) fn schedule_decoration_publish(self: &Arc<Self>) {
        let gen = self
            .decoration_publish_gen
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let this = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(64)).await;
            if this.decoration_publish_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            this.publish_refresh();
        });
    }

    pub(super) fn reset_pane_viewport(&self, pane: u8) {
        self.viewport_by_pane.write().remove(&pane);
    }

    pub(super) fn bump_listing_epoch(&self, tab_id: Uuid) -> u64 {
        let mut map = self.listing_epoch.write();
        let slot = map.entry(tab_id).or_insert(0);
        *slot = slot.saturating_add(1);
        *slot
    }

    pub(super) fn listing_epoch(&self, tab_id: Uuid) -> u64 {
        self.listing_epoch.read().get(&tab_id).copied().unwrap_or(0)
    }

    pub(super) fn record_visit(&self, path: &orchid_fs::FsPath) {
        self.visit_log.lock().record(path.as_str());
    }

    /// Resolve the formatted display strings (size / date / type / name) for
    /// `entry`, reusing a cached result when neither the entry metadata nor
    /// the locale / show-extensions setting changed since the last snapshot.
    ///
    /// This is the hot path: `build_tab_payload` calls it once per visible row
    /// (~96) per snapshot, and a Fluent `tr_args` lookup plus a `chrono`
    /// format parse per call was the dominant CPU cost on scroll / selection.
    pub(super) fn entry_text_for(
        &self,
        entry: &orchid_fs::FsEntry,
        show_extensions: bool,
        locale: &LocaleConfig,
        locale_tag: &str,
    ) -> Arc<EntryText> {
        let path_key = entry.path.as_str().to_string();
        let modified_ms = entry
            .metadata
            .modified
            .map(|t| t.timestamp_millis())
            .unwrap_or(0);
        let is_dir = matches!(entry.metadata.kind, orchid_fs::FsEntryKind::Directory);
        {
            let cache = self.entry_text_cache.lock();
            if let Some(c) = cache.get(&path_key) {
                if c.size == entry.metadata.size
                    && c.modified_ms == modified_ms
                    && c.name == entry.name
                    && c.is_dir == is_dir
                    && c.show_ext == show_extensions
                    && c.locale_tag == locale_tag
                    && c.date_format == locale.date_format
                    && c.time_format == locale.time_format
                {
                    return Arc::clone(c);
                }
            }
        }
        let display_name = entry_display_name(&entry.name, is_dir, show_extensions);
        let size_text = self.deps.locale.format_byte_size(entry.metadata.size);
        let modified_text = entry
            .metadata
            .modified
            .map(|t| locale.format_datetime(t))
            .unwrap_or_default();
        let type_text = if is_dir {
            self.deps.locale.tr("fm-properties-kind-folder")
        } else {
            classify(&self.deps.locale, &entry.name, false)
        };
        let icon: &'static str = if is_dir { "folder" } else { "file" };
        let text = Arc::new(EntryText {
            size: entry.metadata.size,
            modified_ms,
            name: entry.name.clone(),
            is_dir,
            show_ext: show_extensions,
            locale_tag: locale_tag.to_string(),
            date_format: locale.date_format.clone(),
            time_format: locale.time_format.clone(),
            display_name,
            size_text,
            modified_text,
            type_text,
            icon,
        });
        self.entry_text_cache
            .lock()
            .insert(path_key, Arc::clone(&text));
        text
    }

    pub(super) fn visit_history_payload(&self) -> Vec<VisitHistoryItemPayload> {
        self.visit_log
            .lock()
            .menu_items()
            .into_iter()
            .map(|item| VisitHistoryItemPayload {
                path: item.path,
                frequent: item.frequent,
                is_header: item.is_header,
            })
            .collect()
    }

    pub(super) fn install_dir_watch_handlers(self: &Arc<Self>) {
        if self.deps.file_watcher.is_none() {
            return;
        }
        use orchid_core::{Event, EventFilter, HandlerPriority};
        use orchid_fs::{FsCreatedEvent, FsDeletedEvent, FsModifiedEvent, FsRenamedEvent};

        let filter = EventFilter::default()
            .add_type(FsCreatedEvent::event_type())
            .add_type(FsModifiedEvent::event_type())
            .add_type(FsDeletedEvent::event_type())
            .add_type(FsRenamedEvent::event_type());
        let this = Arc::downgrade(self);
        match self
            .bus
            .subscribe_async(filter, HandlerPriority::Normal, move |env| {
                let this = this.clone();
                async move {
                    let Some(inner) = this.upgrade() else {
                        return;
                    };
                    for path in fs_event_paths(&env) {
                        inner.schedule_external_refresh(&path);
                    }
                }
            }) {
            Ok(handle) => {
                self.dir_watch_subs.lock().push(handle);
            }
            Err(e) => {
                warn!(error = %e, "fm: failed to subscribe to directory watch events");
            }
        }
    }

    pub(super) fn clear_dir_watches(&self) {
        self.watch_handles.lock().clear();
        self.watch_paths.write().clear();
        self.dir_watch_subs.lock().clear();
    }

    pub(super) fn drop_tab_watch(&self, tab_id: Uuid) {
        self.watch_handles.lock().remove(&tab_id);
        self.watch_paths.write().remove(&tab_id);
    }

    pub(super) async fn rewatch_tab(self: &Arc<Self>, tab: &TabState) {
        self.drop_tab_watch(tab.id);
        if is_virtual(&tab.path) {
            return;
        }
        let Some(watcher) = self.deps.file_watcher.as_ref() else {
            return;
        };
        // Non-recursive: the listing only needs sibling-entry events. Recursive
        // watches on home/OneDrive trees block startup for minutes.
        match watcher.watch(tab.path.clone(), false).await {
            Ok(handle) => {
                self.watch_paths
                    .write()
                    .insert(tab.id, tab.path.as_str().to_string());
                self.watch_handles.lock().insert(tab.id, handle);
            }
            Err(e) => {
                debug!(
                    error = %e,
                    path = %tab.path.as_str(),
                    "fm: directory watch unavailable"
                );
            }
        }
    }

    pub(super) fn schedule_external_refresh(self: &Arc<Self>, path: &orchid_fs::FsPath) {
        let path_str = path.as_str();
        let affected: Vec<TabState> = {
            let watch_paths = self.watch_paths.read();
            let state = self.state.lock();
            let mut out = Vec::new();
            for (tab_id, root) in watch_paths.iter() {
                if !fs_event_affects_listing(path_str, root) {
                    continue;
                }
                if let Some(tab) = find_tab_by_id(&state, *tab_id) {
                    out.push(tab.clone());
                }
            }
            out
        };
        if affected.is_empty() {
            return;
        }
        let gen = self.external_refresh_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let this = Arc::clone(self);
        let changed_path = path.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            if this.external_refresh_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let show_hidden = this.config.read().show_hidden;
            let mut need_publish = false;
            for tab in affected {
                // Prefer the live tab state in case the user navigated away.
                let live = {
                    let state = this.state.lock();
                    find_tab_by_id(&state, tab.id).cloned()
                };
                let Some(live) = live else {
                    continue;
                };
                if live.path != tab.path {
                    continue;
                }
                // Fast path: patch a single direct child without re-listing.
                if this
                    .try_incremental_entry_update(&live, &changed_path, show_hidden)
                    .await
                {
                    need_publish = true;
                    continue;
                }
                this.refresh_tab(&live, show_hidden).await;
                need_publish = true;
            }
            if need_publish {
                this.publish_refresh();
            }
        });
    }

    /// Patch a single listing entry for `changed` when it is a direct child of `tab.path`.
    /// Returns `true` when the listing was updated without a full re-list.
    pub(super) async fn try_incremental_entry_update(
        self: &Arc<Self>,
        tab: &TabState,
        changed: &orchid_fs::FsPath,
        show_hidden: bool,
    ) -> bool {
        if is_virtual(&tab.path) || tab.branch_view {
            return false;
        }
        let Some(parent) = changed.parent() else {
            return false;
        };
        if parent.as_str() != tab.path.as_str() {
            // Nested noise under the open folder — keep coalesced full refresh.
            return false;
        }
        let Some(provider) = self.deps.registry.for_path(changed) else {
            return false;
        };
        let exists = match provider.exists(changed).await {
            Ok(v) => v,
            Err(_) => return false,
        };
        let mut entries = {
            let guard = self.entries_by_tab.read();
            guard
                .get(&tab.id)
                .map(|v| v.as_ref().clone())
                .unwrap_or_default()
        };
        let key = changed.as_str();
        if !exists {
            let before = entries.len();
            entries.retain(|e| e.path.as_str() != key);
            if entries.len() == before {
                return false;
            }
        } else {
            let Ok(meta) = provider.metadata(changed).await else {
                return false;
            };
            if !show_hidden && meta.hidden {
                entries.retain(|e| e.path.as_str() != key);
            } else {
                let name = changed
                    .file_name()
                    .map(String::from)
                    .unwrap_or_else(|| key.to_string());
                let mut entry = orchid_fs::FsEntry {
                    path: changed.clone(),
                    name,
                    metadata: meta,
                };
                let mut batch = vec![entry];
                self.apply_entry_metadata(&mut batch);
                entry = batch.remove(0);
                if let Some(slot) = entries.iter_mut().find(|e| e.path.as_str() == key) {
                    *slot = entry;
                } else {
                    entries.push(entry);
                }
                sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            }
        }
        self.entries_by_tab
            .write()
            .insert(tab.id, Arc::new(entries));
        true
    }

    pub(super) fn enabled_network_mounts(&self) -> Vec<orchid_storage::NetworkMountConfig> {
        self.deps
            .network_mounts
            .read()
            .iter()
            .filter(|m| m.enabled && !m.uri.trim().is_empty())
            .cloned()
            .collect()
    }

    pub(super) fn network_mount_payloads(&self) -> Vec<NetworkMountPayload> {
        self.enabled_network_mounts()
            .into_iter()
            .filter_map(|m| {
                let uri = orchid_fs::normalize_mount_uri(&m.uri)?;
                Some(NetworkMountPayload {
                    name: network_mount_display_name(&m, &uri),
                    uri,
                })
            })
            .collect()
    }

    pub(super) fn managed_folder_payloads(&self) -> Vec<ManagedFolderSidebarPayload> {
        let roots = self.managed_roots.read().clone();
        let stats = self.managed_stats.read();
        let policies = self.managed_policies.read();
        roots
            .into_iter()
            .map(|path| {
                let st = stats.get(&path);
                let policy = policies.get(&path).and_then(|p| p.as_ref());
                let files_tracked = st.map(|s| s.files_tracked as u32).unwrap_or(0);
                let dedup_bytes = st
                    .map(|s| s.logical_bytes.saturating_sub(s.physical_bytes))
                    .unwrap_or(0);
                ManagedFolderSidebarPayload {
                    path,
                    files_tracked,
                    dedup_bytes,
                    policy_max_bytes: policy.and_then(|p| p.max_size_bytes),
                    policy_retention_days: policy.and_then(|p| p.retention_days),
                    policy_exclude_count: policy
                        .map(|p| p.exclude_patterns.len() as u32)
                        .unwrap_or(0),
                }
            })
            .collect()
    }

    pub(super) fn set_activity_notice(&self, key: &str, name: Option<String>) {
        *self.activity_notice_key.write() = Some(key.to_string());
        *self.activity_notice_name.write() = name;
        *self.activity_notice_at.write() = Some(std::time::Instant::now());
        self.publish_refresh();
    }

    pub(super) fn activity_notice_key(&self) -> Option<String> {
        let at = *self.activity_notice_at.read();
        if at
            .map(|t| t.elapsed() < std::time::Duration::from_secs(8))
            .unwrap_or(false)
        {
            self.activity_notice_key.read().clone()
        } else {
            None
        }
    }

    pub(super) fn activity_notice_name(&self) -> Option<String> {
        let at = *self.activity_notice_at.read();
        if at
            .map(|t| t.elapsed() < std::time::Duration::from_secs(8))
            .unwrap_or(false)
        {
            self.activity_notice_name.read().clone()
        } else {
            None
        }
    }

    pub(super) fn transfer_error_label(&self) -> Option<String> {
        let notice = self.transfer_notice.read();
        if let Some((msg, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(msg.clone());
            }
        }
        None
    }

    pub(super) fn set_transfer_notice(&self, message: String) {
        *self.transfer_notice.write() = Some((message, std::time::Instant::now()));
        self.publish_refresh();
    }

    pub(super) fn passphrase_error_label(&self) -> Option<String> {
        let notice = self.passphrase_error.read();
        if let Some((msg, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(msg.clone());
            }
        }
        None
    }

    pub(super) fn set_passphrase_error(&self, message: String) {
        *self.passphrase_error.write() = Some((message, std::time::Instant::now()));
        self.publish_refresh();
    }

    pub(super) fn clear_passphrase_error(&self) {
        *self.passphrase_error.write() = None;
    }

    pub(super) fn ingest_error_label(&self) -> Option<String> {
        let notice = self.ingest_error.read();
        if let Some((name, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(name.clone());
            }
        }
        None
    }

    pub(super) fn set_ingest_error(&self, name: String) {
        *self.ingest_error.write() = Some((name, std::time::Instant::now()));
        self.publish_refresh();
    }

    pub(super) fn clear_ingest_error(&self) {
        *self.ingest_error.write() = None;
    }

    pub(super) fn activity_indicator_label(&self) -> Option<String> {
        if self.ingest_in_flight.load(Ordering::Relaxed) > 0 {
            return self.ingest_current.read().clone();
        }
        let notice = self.ingest_notice.read();
        if let Some((name, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(name.clone());
            }
        }
        None
    }

    pub(super) fn handle_managed_ingest_started(&self, path: &orchid_fs::FsPath) {
        self.ingest_in_flight.fetch_add(1, Ordering::Relaxed);
        let label = path
            .file_name()
            .map(String::from)
            .unwrap_or_else(|| path.as_str().to_string());
        *self.ingest_current.write() = Some(label);
        self.publish_refresh();
    }

    pub(super) fn handle_managed_ingest_finished(&self) {
        let prev = self.ingest_in_flight.fetch_sub(1, Ordering::Relaxed);
        if prev <= 1 {
            *self.ingest_current.write() = None;
        }
        self.publish_refresh();
    }

    pub(super) fn handle_managed_ingest_failed(&self, path: &orchid_fs::FsPath) {
        self.handle_managed_ingest_finished();
        let name = path
            .file_name()
            .map(String::from)
            .unwrap_or_else(|| path.as_str().to_string());
        self.set_ingest_error(name);
    }

    pub(super) async fn handle_managed_ingest(&self, path: &orchid_fs::FsPath) {
        self.handle_managed_ingest_finished();
        self.clear_ingest_error();
        let label = path
            .file_name()
            .map(String::from)
            .unwrap_or_else(|| path.as_str().to_string());
        *self.ingest_notice.write() = Some((label, std::time::Instant::now()));
        self.publish_refresh();
        self.refresh_managed_roots().await;
        self.publish_refresh();
    }

    pub(super) fn begin_transfer(&self, is_copy: bool) {
        let queue_len = self.xfer.queue.lock().len() as u32;
        *self.transfer.write() = TransferState {
            active: true,
            is_copy,
            queue_len,
            ..TransferState::default()
        };
        self.publish_refresh();
    }

    pub(super) fn apply_transfer_progress(&self, p: &orchid_fs::OperationProgress) {
        let name = p
            .current_path
            .file_name()
            .map(str::to_string)
            .unwrap_or_default();
        let should_publish = {
            let mut st = self.transfer.write();
            st.active = true;
            st.current_name = name;
            st.processed_bytes = p.processed_bytes;
            st.total_bytes = p.total_bytes;
            st.last_publish
                .map(|t| t.elapsed() >= std::time::Duration::from_millis(100))
                .unwrap_or(true)
        };
        if should_publish {
            self.transfer.write().last_publish = Some(std::time::Instant::now());
            self.publish_refresh();
        }
    }

    pub(super) fn end_transfer(&self) {
        let queue_len = self.xfer.queue.lock().len() as u32;
        *self.transfer.write() = TransferState {
            queue_len,
            ..TransferState::default()
        };
        self.publish_refresh();
    }

    pub(super) async fn refresh_all_tabs(self: &Arc<Self>) {
        self.refresh_all_tabs_with_opts(RefreshOpts {
            publish: true,
            indicate_loading: false,
        })
        .await;
    }

    pub(super) async fn refresh_all_tabs_with_opts(self: &Arc<Self>, opts: RefreshOpts) {
        let tabs = {
            let state = self.state.lock();
            let mut tabs = vec![state.left_pane.active_tab().clone()];
            if let Some(right) = state.right_pane.as_ref() {
                tabs.push(right.active_tab().clone());
            }
            tabs
        };
        self.refresh_tabs_with_opts(&tabs, opts).await;
    }

    /// Re-list only the given tabs (e.g. the pane that just navigated).
    pub(super) async fn refresh_tabs_with_opts(
        self: &Arc<Self>,
        tabs: &[TabState],
        opts: RefreshOpts,
    ) {
        if tabs.is_empty() {
            return;
        }
        // Catalog IO must not block the directory list; first paint uses the
        // last-known roots (empty only on a cold widget) and badges catch up.
        {
            let this = Arc::clone(self);
            tokio::spawn(async move {
                let prev_managed = this.managed_roots.read().clone();
                let prev_encrypted = this.encrypted_paths.read().clone();
                this.refresh_managed_roots().await;
                this.refresh_encrypted_paths().await;
                if *this.managed_roots.read() != prev_managed
                    || *this.encrypted_paths.read() != prev_encrypted
                {
                    this.reapply_catalog_flags();
                    this.publish_refresh();
                }
            });
        }
        let show_hidden = self.config.read().show_hidden;

        let loading_ids: Vec<Uuid> = tabs.iter().map(|t| t.id).collect();

        if opts.indicate_loading {
            {
                let mut loading = self.loading_tabs.write();
                for id in &loading_ids {
                    loading.insert(*id);
                }
            }
            {
                let mut entries = self.entries_by_tab.write();
                for id in &loading_ids {
                    entries.remove(id);
                }
            }
            self.publish_refresh();
        }

        let listings = tabs.iter().cloned().map(|tab| {
            let this = Arc::clone(self);
            async move { this.refresh_tab(&tab, show_hidden).await }
        });
        futures::future::join_all(listings).await;

        if opts.indicate_loading {
            let mut loading = self.loading_tabs.write();
            for id in &loading_ids {
                loading.remove(id);
            }
        }

        if opts.publish {
            self.publish_refresh();
        }
    }

    /// Re-sort a tab's already-loaded entries without touching the filesystem.
    pub(super) fn resort_tab_in_memory(&self, tab_id: Uuid, sort_by: SortBy, descending: bool) {
        let mut entries = self.entries_by_tab.write();
        if let Some(list) = entries.get_mut(&tab_id) {
            sort_entries(Arc::make_mut(list), sort_by, descending);
        }
    }

    pub(super) async fn delete_paths(
        &self,
        paths: &[String],
        to_recycle: Option<bool>,
    ) -> WidgetResult<()> {
        let recycle_paths: Vec<String> = paths
            .iter()
            .filter(|p| orchid_fs::is_recycle_item(p))
            .cloned()
            .collect();
        if !recycle_paths.is_empty() {
            orchid_fs::purge_recycle(&recycle_paths)
                .await
                .map_err(map_fs_error)?;
        }
        let rest: Vec<&String> = paths
            .iter()
            .filter(|p| !orchid_fs::is_recycle_item(p))
            .collect();
        if rest.is_empty() {
            return Ok(());
        }
        let registry = &self.deps.registry;
        let to_recycle_bin = to_recycle.unwrap_or_else(|| self.config.read().delete_to_recycle);
        let opts = orchid_fs::operations::delete::DeleteOptions {
            to_recycle_bin,
            recursive: !to_recycle_bin, // permanent deletes need recurse for folders
        };
        for p in rest {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            orchid_fs::operations::delete::delete(registry, &fp, opts)
                .await
                .map_err(map_fs_error)?;
        }
        Ok(())
    }

    pub(super) async fn refresh_tab(self: &Arc<Self>, tab: &TabState, show_hidden: bool) {
        let path = tab.path.clone();
        let epoch = self.bump_listing_epoch(tab.id);
        let t0 = std::time::Instant::now();
        debug!(path = %path.as_str(), "fm refresh_tab start");

        let entries = if is_virtual(&path) {
            self.tab_errors.write().insert(tab.id, None);
            let mut entries = self.list_virtual(&path).await;
            if self.listing_epoch(tab.id) != epoch {
                return;
            }
            sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            let entries = Arc::new(entries);
            self.entries_by_tab
                .write()
                .insert(tab.id, Arc::clone(&entries));
            entries
        } else if tab.branch_view {
            let result = self.navigator.list_branch(&path, show_hidden).await;
            if self.listing_epoch(tab.id) != epoch {
                return;
            }
            self.tab_errors.write().insert(tab.id, result.error.clone());
            let mut entries = result.entries;
            self.apply_entry_metadata(&mut entries);
            sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            let entries = Arc::new(entries);
            self.entries_by_tab
                .write()
                .insert(tab.id, Arc::clone(&entries));
            entries
        } else {
            let this = Arc::clone(self);
            let tab_id = tab.id;
            let sort_by = tab.sort_by;
            let sort_desc = tab.sort_descending;
            let tab_preview = tab.clone();
            let result = self
                .navigator
                .navigate_with_preview(
                    &path,
                    show_hidden,
                    FM_VIRTUALIZE_THRESHOLD,
                    move |mut partial| {
                        if this.listing_epoch(tab_id) != epoch {
                            return;
                        }
                        this.apply_entry_metadata(&mut partial);
                        sort_entries(&mut partial, sort_by, sort_desc);
                        this.entries_by_tab
                            .write()
                            .insert(tab_id, Arc::new(partial));
                        this.publish_refresh();
                        this.spawn_view_decorations(tab_preview);
                    },
                )
                .await;
            if self.listing_epoch(tab.id) != epoch {
                return;
            }
            self.tab_errors.write().insert(tab.id, result.error.clone());
            let mut entries = result.entries;
            self.apply_entry_metadata(&mut entries);
            sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            let entries = Arc::new(entries);
            self.entries_by_tab
                .write()
                .insert(tab.id, Arc::clone(&entries));
            entries
        };

        debug!(
            path = %path.as_str(),
            entries = entries.len(),
            elapsed_ms = t0.elapsed().as_millis(),
            "fm refresh_tab listed"
        );

        // Watch + icons/thumbs off the critical path so restore/bootstrap can
        // finish and the main window can open without waiting on notify.
        let this = Arc::clone(self);
        let tab = tab.clone();
        tokio::spawn(async move {
            this.rewatch_tab(&tab).await;
            this.decorate_view(&tab).await;
        });
    }

    pub(super) fn record_recent(&self, path: &orchid_fs::FsPath) {
        self.deps.recent_files.touch(path, Some(&self.bus));
    }

    pub(super) fn collect_catalog_candidates(&self) -> Vec<orchid_fs::FsPath> {
        let mut paths: Vec<orchid_fs::FsPath> =
            self.deps.tag_manager.starred_paths().unwrap_or_default();
        paths.extend(
            self.deps
                .recent_files
                .paths()
                .into_iter()
                .filter_map(|p| orchid_fs::FsPath::new(&p).ok()),
        );
        for tag in self.deps.tag_manager.all_tags().unwrap_or_default() {
            paths.extend(
                self.deps
                    .tag_manager
                    .paths_with_tag(&tag)
                    .unwrap_or_default(),
            );
        }
        paths.sort_by_key(|p| p.as_str().to_string());
        paths.dedup();
        paths
    }

    pub(super) async fn hydrate_entries_metadata(&self, entries: &mut [orchid_fs::FsEntry]) {
        for e in entries.iter_mut() {
            // Local FindFirstFile / rclone `lsjson` already filled kind, size,
            // mtime, and often mime. Re-statting every row (especially over
            // the network) was a sequential round-trip storm after each list.
            // Only fill gaps left by virtual folders / catalog paths.
            if matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory) {
                continue;
            }
            if e.metadata.modified.is_some() || e.metadata.mime.is_some() {
                continue;
            }
            let Some(provider) = self.deps.registry.for_path(&e.path) else {
                continue;
            };
            if let Ok(meta) = provider.metadata(&e.path).await {
                if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                    continue;
                }
                e.metadata = meta;
            }
        }
    }

    pub(super) async fn list_category(&self, cat: FileCategory) -> Vec<orchid_fs::FsEntry> {
        let mut path_keys: std::collections::HashSet<String> = self
            .collect_catalog_candidates()
            .into_iter()
            .map(|p| p.as_str().to_string())
            .collect();
        for p in self.search_category_paths(cat).await {
            path_keys.insert(p.as_str().to_string());
        }
        let mut entries = Vec::new();
        for key in path_keys {
            let Ok(p) = orchid_fs::FsPath::new(&key) else {
                continue;
            };
            let Some(provider) = self.deps.registry.for_path(&p) else {
                continue;
            };
            let meta = match provider.metadata(&p).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                continue;
            }
            let entry = orchid_fs::FsEntry {
                path: p,
                name: key.rsplit('/').next().unwrap_or(&key).to_string(),
                metadata: meta,
            };
            if entry_matches_category(&entry, cat) {
                entries.push(entry);
            }
            if entries.len() >= 200 {
                break;
            }
        }
        self.apply_entry_metadata(&mut entries);
        entries
    }

    pub(super) async fn search_category_paths(&self, cat: FileCategory) -> Vec<orchid_fs::FsPath> {
        let Some(engine) = self.deps.search.as_ref() else {
            return Vec::new();
        };
        let mut paths = Vec::new();
        for ext in category_search_extensions(cat) {
            let mut q = orchid_search::query::QueryBuilder::new()
                .extension(*ext)
                .limit(50)
                .build();
            q.only_files = true;
            if let Ok(results) = engine.search(q).await {
                for hit in results.hits {
                    if let Ok(p) = orchid_fs::FsPath::new(&hit.path) {
                        paths.push(p);
                    }
                }
            }
        }
        paths.sort_by_key(|p| p.as_str().to_string());
        paths.dedup();
        paths
    }

    pub(super) fn filtered_paths_for_tab(&self, tab: &TabState) -> Vec<String> {
        let guard = self.entries_by_tab.read();
        let Some(entries) = guard.get(&tab.id) else {
            return Vec::new();
        };
        let quick = tab.quick_filter.trim();
        if quick.is_empty() {
            return entries
                .iter()
                .map(|e| e.path.as_str().to_string())
                .collect();
        }
        let q = quick.to_lowercase();
        entries
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&q))
            .map(|e| e.path.as_str().to_string())
            .collect()
    }

    pub(super) fn select_all_in_pane(&self, pane: u8) {
        let (tab_id, paths) = {
            let state = self.state.lock();
            let tab = match active_tab_ref(&state, pane) {
                Ok(t) => t,
                Err(_) => return,
            };
            (tab.id, self.filtered_paths_for_tab(tab))
        };
        let mut state = self.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.tabs.iter_mut().find(|t| t.id == tab_id)
            } else {
                state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id)
            }
        } else {
            state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id)
        };
        if let Some(t) = tab {
            t.selection.select_all(&paths);
        }
    }

    pub(super) fn invert_selection_in_pane(&self, pane: u8) {
        self.mutate_selection_in_pane(pane, |sel, paths, _| sel.invert(paths));
    }

    pub(super) fn apply_filter_in_pane(&self, pane: u8, op: MaskOp, filter: &SelectFilter) {
        self.mutate_selection_in_pane(pane, |sel, paths, entries| {
            sel.apply_matching(paths, op, |p| {
                entries
                    .iter()
                    .find(|e| e.path.as_str() == p)
                    .is_some_and(|e| selection::entry_matches_filter(e, filter))
            });
        });
    }

    pub(super) fn select_index_range_in_pane(
        &self,
        pane: u8,
        from: usize,
        to: usize,
        additive: bool,
        columns: usize,
    ) {
        self.mutate_selection_in_pane(pane, |sel, paths, _| {
            sel.select_index_rect(paths, from, to, columns, additive);
        });
    }

    pub(super) fn mutate_selection_in_pane(
        &self,
        pane: u8,
        f: impl FnOnce(&mut SelectionModel, &[String], &[orchid_fs::FsEntry]),
    ) {
        let (tab_id, paths, entries) = {
            let state = self.state.lock();
            let tab = match active_tab_ref(&state, pane) {
                Ok(t) => t,
                Err(_) => return,
            };
            let entries = self
                .entries_by_tab
                .read()
                .get(&tab.id)
                .cloned()
                .unwrap_or_else(|| Arc::new(Vec::new()));
            (tab.id, self.filtered_paths_for_tab(tab), entries)
        };
        let mut state = self.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.tabs.iter_mut().find(|t| t.id == tab_id)
            } else {
                state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id)
            }
        } else {
            state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id)
        };
        if let Some(t) = tab {
            f(&mut t.selection, &paths, entries.as_slice());
        }
    }

    pub(super) fn selection_bytes_in_tab(&self, tab: &TabState) -> u64 {
        let Some(entries) = self.entries_by_tab.read().get(&tab.id).cloned() else {
            return 0;
        };
        entries
            .iter()
            .filter(|e| tab.selection.is_selected(e.path.as_str()))
            .map(|e| e.metadata.size)
            .sum()
    }

    pub(super) fn move_selection_in_pane(&self, pane: u8, delta: i32, extend: bool) {
        let (tab_id, ordered) = {
            let state = self.state.lock();
            let tab = match active_tab_ref(&state, pane) {
                Ok(t) => t,
                Err(_) => return,
            };
            (tab.id, self.filtered_paths_for_tab(tab))
        };
        let mut state = self.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.tabs.iter_mut().find(|t| t.id == tab_id)
            } else {
                state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id)
            }
        } else {
            state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id)
        };
        if let Some(t) = tab {
            t.selection.select_relative(&ordered, delta, extend);
        }
    }

    pub(super) fn deselect_all_in_pane(&self, pane: u8) {
        let mut state = self.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        tab.selection.clear();
    }

    pub(super) async fn create_folder_at(
        &self,
        parent: &orchid_fs::FsPath,
        name: &str,
    ) -> WidgetResult<()> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains(':') {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-invalid-folder-name".into(),
            ));
        }
        let new_path = parent.join(name);
        let provider =
            self.deps.registry.for_path(parent).ok_or_else(|| {
                WidgetError::InvalidStateForOperation("fm-no-provider-parent".into())
            })?;
        provider
            .create_dir(&new_path, false)
            .await
            .map_err(map_fs_error)?;
        self.record_undo(undo::FsUndoOp::Create {
            paths: vec![new_path.as_str().to_string()],
            recycle_items: Vec::new(),
        });
        Ok(())
    }

    pub(super) async fn create_file_at(
        &self,
        parent: &orchid_fs::FsPath,
        name: &str,
    ) -> WidgetResult<()> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains(':') {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-invalid-folder-name".into(),
            ));
        }
        let new_path = parent.join(name);
        let provider =
            self.deps.registry.for_path(parent).ok_or_else(|| {
                WidgetError::InvalidStateForOperation("fm-no-provider-parent".into())
            })?;
        if provider.metadata(&new_path).await.is_ok() {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-invalid-folder-name".into(),
            ));
        }
        provider.write(&new_path, &[]).await.map_err(map_fs_error)?;
        self.record_undo(undo::FsUndoOp::Create {
            paths: vec![new_path.as_str().to_string()],
            recycle_items: Vec::new(),
        });
        Ok(())
    }

    pub(super) fn spawn_view_decorations(self: &Arc<Self>, tab: TabState) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.decorate_view(&tab).await;
        });
    }

    pub(super) async fn decorate_view(self: &Arc<Self>, tab: &TabState) {
        let Some(entries) = self.entries_by_tab.read().get(&tab.id).cloned() else {
            return;
        };
        let probe = self.probe_encrypted_directories(tab, entries.as_slice());
        let icons = self.ensure_shell_icons(tab, entries.as_slice());
        tokio::join!(probe, icons);
        if config_for_mode(tab.view_mode, 1.0).show_thumbnails {
            self.ensure_thumbnails(tab, entries.as_slice()).await;
        }
    }

    pub(super) fn visible_entry_range(&self, tab: &TabState, len: usize) -> (usize, usize) {
        let pane = {
            let state = self.state.lock();
            if state.left_pane.active_tab().id == tab.id {
                Some(0u8)
            } else if state
                .right_pane
                .as_ref()
                .is_some_and(|r| r.active_tab().id == tab.id)
            {
                Some(1)
            } else {
                None
            }
        };
        let stored = pane.and_then(|p| self.viewport_by_pane.read().get(&p).copied());
        clamp_entry_window(stored, len, 96)
    }

    pub(super) fn collect_missing_shell_icons(
        &self,
        entries: &[orchid_fs::FsEntry],
        range: std::ops::Range<usize>,
        size: orchid_fs::ShellIconSize,
    ) -> Vec<(String, orchid_fs::FsPath, bool)> {
        let cache = self.shell_icon_rgba.read();
        let mut pending = Vec::new();
        for e in entries.get(range).into_iter().flatten() {
            let path_key = e.path.as_str().to_string();
            let cache_key = shell_icon_cache_key(&path_key, size);
            if cache.contains_key(&cache_key) {
                continue;
            }
            let is_dir = matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory);
            pending.push((path_key, e.path.clone(), is_dir));
        }
        pending
    }

    pub(super) async fn extract_shell_icon_batch(
        &self,
        pending: &[(String, orchid_fs::FsPath, bool)],
        size: orchid_fs::ShellIconSize,
        display_px: u32,
    ) -> bool {
        const BATCH: usize = 24;
        let mut any_hit = false;
        for chunk in pending.chunks(BATCH) {
            let futs = chunk.iter().map(|(path_key, path, is_dir)| {
                let path_key = path_key.clone();
                let path = path.clone();
                let is_dir = *is_dir;
                async move {
                    let icon = tokio::task::spawn_blocking(move || {
                        orchid_fs::shell_icon(&path, is_dir, size)
                            .map(|icon| downscale_shell_icon(icon, display_px))
                    })
                    .await
                    .ok()
                    .flatten();
                    (path_key, icon)
                }
            });
            let results = futures::future::join_all(futs).await;
            {
                let mut cache = self.shell_icon_rgba.write();
                let mut order = self.shell_icon_order.write();
                for (path_key, icon) in results {
                    let Some(icon) = icon else {
                        continue;
                    };
                    insert_capped_icon(
                        &mut cache,
                        &mut order,
                        shell_icon_cache_key(&path_key, size),
                        orchid_viewers::Thumbnail {
                            rgba: icon.rgba,
                            width: icon.width,
                            height: icon.height,
                        },
                        SHELL_ICON_CACHE_BYTES,
                    );
                    any_hit = true;
                }
            }
        }
        any_hit
    }

    pub(super) async fn ensure_shell_icons(
        self: &Arc<Self>,
        tab: &TabState,
        entries: &[orchid_fs::FsEntry],
    ) {
        let size = shell_icon_size_for_mode(tab.view_mode);
        let display_px = shell_icon_display_px(tab.view_mode);
        let (first, end) = self.visible_entry_range(tab, entries.len());
        let visible = self.collect_missing_shell_icons(entries, first..end, size);
        if !visible.is_empty()
            && self
                .extract_shell_icon_batch(&visible, size, display_px)
                .await
        {
            // First visible batch paints immediately; further decoration churn
            // is coalesced so hover/scroll are not fighting every icon tick.
            self.publish_refresh();
        }
        let prefetch_end = (end + 48).min(entries.len());
        let extra = self.collect_missing_shell_icons(entries, end..prefetch_end, size);
        if !extra.is_empty()
            && self
                .extract_shell_icon_batch(&extra, size, display_px)
                .await
        {
            self.schedule_decoration_publish();
        }
    }

    pub(super) async fn probe_encrypted_directories(
        self: &Arc<Self>,
        tab: &TabState,
        entries: &[orchid_fs::FsEntry],
    ) {
        let (first, end) = self.visible_entry_range(tab, entries.len());
        let mut hits = Vec::new();
        for e in entries.get(first..end).into_iter().flatten() {
            if !matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory) {
                continue;
            }
            if e.metadata.extended.is_encrypted {
                continue;
            }
            if orchid_fs::encrypted::marker::looks_encrypted_directory(&e.path) {
                hits.push(e.path.as_str().to_string());
            }
        }
        if hits.is_empty() {
            return;
        }
        {
            let mut map = self.entries_by_tab.write();
            let Some(list) = map.get_mut(&tab.id) else {
                return;
            };
            let list = Arc::make_mut(list);
            for e in list.iter_mut() {
                if hits.iter().any(|p| p == e.path.as_str()) {
                    e.metadata.extended.is_encrypted = true;
                }
            }
        }
        self.schedule_decoration_publish();
    }

    pub(super) async fn ensure_thumbnails(
        self: &Arc<Self>,
        tab: &TabState,
        entries: &[orchid_fs::FsEntry],
    ) {
        let mode_cfg = config_for_mode(tab.view_mode, 1.0);
        if !mode_cfg.show_thumbnails {
            return;
        }
        let thumb_size = viewer_thumb_size(self.config.read().thumbnail_size);

        let (first, end) = self.visible_entry_range(tab, entries.len());
        let mut pending = Vec::new();
        {
            let cache = self.thumbnail_rgba.read();
            for e in entries.get(first..end).into_iter().flatten() {
                if !is_image_entry(e) {
                    continue;
                }
                let path_key = e.path.as_str().to_string();
                if cache.contains_key(&path_key) {
                    continue;
                }
                let modified_ms = e
                    .metadata
                    .modified
                    .map(|t| t.timestamp_millis())
                    .unwrap_or(0);
                let key = orchid_viewers::ThumbnailService::cache_key(&e.path, modified_ms);
                pending.push((path_key, e.path.clone(), key));
            }
        }
        if pending.is_empty() {
            return;
        }

        const CONCURRENCY: usize = 4;
        let mut any_thumb = false;
        let mut published_once = false;
        let chunks = pending.chunks(CONCURRENCY).collect::<Vec<_>>();
        for chunk in chunks {
            let futs = chunk.iter().map(|(path_key, path, key)| {
                let path_key = path_key.clone();
                let path = path.clone();
                let key = *key;
                let thumbs = Arc::clone(&self.deps.thumbnails);
                let registry = Arc::clone(&self.deps.registry);
                async move {
                    if let Ok(Some(thumb)) = thumbs.get_cached(&key, thumb_size).await {
                        return Some((path_key, thumb));
                    }
                    // Local files: mmap decode avoids a full Vec copy.
                    if path.scheme() == "local" {
                        if let Ok(os_path) = path.to_local() {
                            match thumbs
                                .generate_from_local_path(key, thumb_size, os_path)
                                .await
                            {
                                Ok(thumb) => return Some((path_key, thumb)),
                                Err(_) => return None,
                            }
                        }
                    }
                    let provider = registry.for_path(&path)?;
                    let bytes = match provider.read(&path).await {
                        Ok(b) if b.len() <= 16 * 1024 * 1024 => b,
                        _ => return None,
                    };
                    match thumbs
                        .generate_from_image_bytes(key, thumb_size, bytes)
                        .await
                    {
                        Ok(thumb) => Some((path_key, thumb)),
                        Err(_) => None,
                    }
                }
            });
            let results = futures::future::join_all(futs).await;
            let mut chunk_hit = false;
            {
                let mut cache = self.thumbnail_rgba.write();
                let mut order = self.thumbnail_order.write();
                for (path_key, thumb) in results.into_iter().flatten() {
                    insert_capped_thumbnail(
                        &mut cache,
                        &mut order,
                        path_key,
                        thumb,
                        THUMBNAIL_CACHE_CAP,
                    );
                    chunk_hit = true;
                    any_thumb = true;
                }
            }
            if chunk_hit {
                if !published_once {
                    self.publish_refresh();
                    published_once = true;
                } else {
                    self.schedule_decoration_publish();
                }
            }
        }
        if any_thumb {
            self.schedule_decoration_publish();
        }
    }

    pub(super) async fn list_virtual(&self, path: &orchid_fs::FsPath) -> Vec<orchid_fs::FsEntry> {
        let raw = path.as_str();
        if raw == "virtual:recent" {
            let mut entries: Vec<orchid_fs::FsEntry> = self
                .deps
                .recent_files
                .paths()
                .into_iter()
                .take(50)
                .filter_map(|p| orchid_fs::FsPath::new(&p).ok())
                .map(|p| orchid_fs::FsEntry {
                    name: p.file_name().map(String::from).unwrap_or_default(),
                    metadata: orchid_fs::FsMetadata {
                        kind: orchid_fs::FsEntryKind::File,
                        size: 0,
                        created: None,
                        modified: None,
                        accessed: None,
                        readonly: false,
                        hidden: false,
                        system: false,
                        mime: None,
                        extended: orchid_fs::ExtendedAttributes::default(),
                    },
                    path: p,
                })
                .collect();
            self.hydrate_entries_metadata(&mut entries).await;
            self.apply_entry_metadata(&mut entries);
            return entries;
        }
        if raw == "virtual:starred" {
            let paths = self.deps.tag_manager.starred_paths().unwrap_or_default();
            let mut entries: Vec<orchid_fs::FsEntry> = paths
                .into_iter()
                .take(200)
                .map(|p| orchid_fs::FsEntry {
                    name: p.file_name().map(String::from).unwrap_or_default(),
                    metadata: orchid_fs::FsMetadata {
                        kind: orchid_fs::FsEntryKind::File,
                        size: 0,
                        created: None,
                        modified: None,
                        accessed: None,
                        readonly: false,
                        hidden: false,
                        system: false,
                        mime: None,
                        extended: orchid_fs::ExtendedAttributes {
                            starred: true,
                            ..orchid_fs::ExtendedAttributes::default()
                        },
                    },
                    path: p,
                })
                .collect();
            self.hydrate_entries_metadata(&mut entries).await;
            self.apply_entry_metadata(&mut entries);
            return entries;
        }
        if let Some(cat) = category_for_virtual_path(raw) {
            return self.list_category(cat).await;
        }
        if raw == "virtual:tags" {
            return self.list_tagged_paths().await;
        }
        if raw == "virtual:network" {
            return self.list_network_mounts();
        }
        if raw == "virtual:search" {
            return self.list_search_sessions();
        }
        if orchid_fs::is_recycle_listing(raw) {
            return self.list_recycle_bin().await;
        }
        if let Some(id) = find::search_session_id(raw) {
            return self
                .search_sessions
                .read()
                .get(&id)
                .map(|s| s.entries.clone())
                .unwrap_or_default();
        }
        Vec::new()
    }

    pub(super) async fn list_recycle_bin(&self) -> Vec<orchid_fs::FsEntry> {
        match orchid_fs::list_recycle().await {
            Ok(items) => orchid_fs::recycle_entries(&items),
            Err(e) => {
                warn!(error = %e, "recycle bin list failed");
                Vec::new()
            }
        }
    }

    pub(super) fn list_search_sessions(&self) -> Vec<orchid_fs::FsEntry> {
        let mut sessions: Vec<find::SearchSession> = self
            .search_sessions
            .read()
            .values()
            .filter(|s| s.saved)
            .cloned()
            .collect();
        sessions.sort_by(|a, b| a.label.cmp(&b.label));
        sessions
            .into_iter()
            .filter_map(|s| {
                let path = orchid_fs::FsPath::new(s.virtual_path()).ok()?;
                Some(orchid_fs::FsEntry {
                    name: s.label,
                    path,
                    metadata: orchid_fs::FsMetadata {
                        kind: orchid_fs::FsEntryKind::Directory,
                        size: s.entries.len() as u64,
                        created: None,
                        modified: None,
                        accessed: None,
                        readonly: false,
                        hidden: false,
                        system: false,
                        mime: None,
                        extended: orchid_fs::ExtendedAttributes::default(),
                    },
                })
            })
            .collect()
    }

    pub(super) fn list_network_mounts(&self) -> Vec<orchid_fs::FsEntry> {
        let mut entries: Vec<orchid_fs::FsEntry> = self
            .enabled_network_mounts()
            .into_iter()
            .filter_map(|m| {
                let uri = orchid_fs::normalize_mount_uri(&m.uri)?;
                let path = orchid_fs::FsPath::new(&uri).ok()?;
                Some(orchid_fs::FsEntry {
                    name: network_mount_display_name(&m, &uri),
                    metadata: orchid_fs::FsMetadata {
                        kind: orchid_fs::FsEntryKind::Directory,
                        size: 0,
                        created: None,
                        modified: None,
                        accessed: None,
                        readonly: false,
                        hidden: false,
                        system: false,
                        mime: None,
                        extended: orchid_fs::ExtendedAttributes::default(),
                    },
                    path,
                })
            })
            .collect();
        for share in list_mapped_network_shares() {
            if entries.iter().any(|e| e.path.as_str() == share.path) {
                continue;
            }
            let Ok(path) = orchid_fs::FsPath::new(&share.path) else {
                continue;
            };
            entries.push(orchid_fs::FsEntry {
                name: share.label,
                metadata: orchid_fs::FsMetadata {
                    kind: orchid_fs::FsEntryKind::Directory,
                    size: 0,
                    created: None,
                    modified: None,
                    accessed: None,
                    readonly: false,
                    hidden: false,
                    system: false,
                    mime: None,
                    extended: orchid_fs::ExtendedAttributes::default(),
                },
                path,
            });
        }
        entries
    }

    pub(super) async fn list_tagged_paths(&self) -> Vec<orchid_fs::FsEntry> {
        let mut seen = std::collections::BTreeSet::new();
        let mut paths = Vec::new();
        for tag in self.deps.tag_manager.all_tags().unwrap_or_default() {
            for p in self
                .deps
                .tag_manager
                .paths_with_tag(&tag)
                .unwrap_or_default()
            {
                let key = p.as_str().to_string();
                if seen.insert(key) {
                    paths.push(p);
                }
            }
            if paths.len() >= 200 {
                break;
            }
        }
        let mut entries: Vec<orchid_fs::FsEntry> = paths
            .into_iter()
            .take(200)
            .map(|p| orchid_fs::FsEntry {
                name: p.file_name().map(String::from).unwrap_or_default(),
                metadata: orchid_fs::FsMetadata {
                    kind: orchid_fs::FsEntryKind::File,
                    size: 0,
                    created: None,
                    modified: None,
                    accessed: None,
                    readonly: false,
                    hidden: false,
                    system: false,
                    mime: None,
                    extended: orchid_fs::ExtendedAttributes::default(),
                },
                path: p,
            })
            .collect();
        self.hydrate_entries_metadata(&mut entries).await;
        self.apply_entry_metadata(&mut entries);
        entries
    }

    pub(super) fn reapply_catalog_flags(&self) {
        let mut map = self.entries_by_tab.write();
        for list in map.values_mut() {
            let list = Arc::make_mut(list);
            self.apply_entry_metadata(list);
        }
    }

    pub(super) fn apply_entry_metadata(&self, entries: &mut [orchid_fs::FsEntry]) {
        if entries.is_empty() {
            return;
        }
        let encrypted_paths = self.encrypted_paths.read().clone();
        let managed_roots = self.managed_roots.read().clone();
        let paths: Vec<orchid_fs::FsPath> = entries.iter().map(|e| e.path.clone()).collect();
        let tags = self.deps.tag_manager.get_many(&paths).unwrap_or_default();
        for e in entries.iter_mut() {
            if let Some(tag) = tags.get(e.path.as_str()) {
                e.metadata.extended.starred = tag.starred;
                e.metadata.extended.tags = tag.tags.clone();
                e.metadata.extended.color_label = tag.color_label;
            }
            let path_str = e.path.as_str();
            if managed_roots.iter().any(|root| path_str.starts_with(root)) {
                e.metadata.extended.is_managed = true;
            }
            let covered = encrypted_paths
                .iter()
                .any(|p| path_str == p || path_str.starts_with(p));
            if covered {
                e.metadata.extended.is_encrypted = true;
                continue;
            }
            // Extension check only — directory marker files are probed off the
            // listing path in [`Self::probe_encrypted_directories`].
            if orchid_fs::encrypted::marker::looks_encrypted(&e.path) {
                e.metadata.extended.is_encrypted = true;
            }
        }
    }

    pub(super) fn is_path_encrypted(&self, path: &orchid_fs::FsPath) -> bool {
        if orchid_fs::encrypted::marker::looks_encrypted(path)
            || orchid_fs::encrypted::marker::looks_encrypted_directory(path)
        {
            return true;
        }
        self.encrypted_paths
            .read()
            .iter()
            .any(|p| path.as_str() == p || path.as_str().starts_with(p))
    }

    pub(super) async fn refresh_encrypted_paths(&self) {
        let paths = if let Some(engine) = self.deps.encrypted.as_ref() {
            engine
                .list_encrypted()
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|r| r.enabled)
                .map(|r| r.path.as_str().to_string())
                .collect()
        } else {
            Vec::new()
        };
        *self.encrypted_paths.write() = paths;
    }

    pub(super) async fn encrypt_paths(
        &self,
        paths: &[String],
        passphrase: &str,
    ) -> WidgetResult<()> {
        let Some(engine) = self.deps.encrypted.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-encryption-unavailable".into(),
            ));
        };
        let identity = orchid_crypto::Identity::passphrase(passphrase);
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            let is_dir = if let Some(provider) = self.deps.registry.for_path(&fp) {
                provider
                    .metadata(&fp)
                    .await
                    .map(|meta| matches!(meta.kind, orchid_fs::FsEntryKind::Directory))
                    .unwrap_or(false)
            } else {
                false
            };
            if is_dir {
                engine
                    .encrypt_directory_in_place(&fp, identity.clone())
                    .await
                    .map_err(map_fs_error)?;
            } else {
                engine
                    .encrypt_in_place(&fp, identity.clone())
                    .await
                    .map_err(map_fs_error)?;
            }
        }
        self.refresh_encrypted_paths().await;
        let name = paths
            .first()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .unwrap_or("files")
            .to_string();
        self.set_activity_notice("fm-encrypted", Some(name));
        Ok(())
    }

    pub(super) async fn decrypt_paths(
        &self,
        paths: &[String],
        passphrase: &str,
    ) -> WidgetResult<()> {
        let Some(engine) = self.deps.encrypted.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-encryption-unavailable".into(),
            ));
        };
        let identity = orchid_crypto::Identity::passphrase(passphrase);
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            engine
                .decrypt_in_place(&fp, identity.clone())
                .await
                .map_err(map_fs_error)?;
        }
        self.refresh_encrypted_paths().await;
        let name = paths
            .first()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .unwrap_or("files")
            .to_string();
        self.set_activity_notice("fm-decrypted", Some(name));
        Ok(())
    }

    pub(super) async fn reveal_paths(
        &self,
        paths: &[String],
        passphrase: &str,
    ) -> WidgetResult<Vec<String>> {
        let Some(engine) = self.deps.encrypted.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-encryption-unavailable".into(),
            ));
        };
        let identity = orchid_crypto::Identity::passphrase(passphrase);
        let mut revealed = Vec::with_capacity(paths.len());
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            let session = engine
                .reveal(&fp, identity.clone())
                .await
                .map_err(map_fs_error)?;
            revealed.push(session.revealed_path.to_string_lossy().into_owned());
        }
        let name = paths
            .first()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .unwrap_or("files")
            .to_string();
        self.set_activity_notice("fm-revealed", Some(name));
        Ok(revealed)
    }

    pub(super) async fn refresh_managed_roots(&self) {
        let mut roots = Vec::new();
        let mut stats = std::collections::HashMap::new();
        let mut policies = std::collections::HashMap::new();
        if let Some(engine) = self.deps.managed.as_ref() {
            if let Ok(folders) = engine.list_folders().await {
                for f in folders.into_iter().filter(|f| f.enabled) {
                    let key = f.path.as_str().to_string();
                    policies.insert(key.clone(), f.policy.clone());
                    roots.push(key.clone());
                    if let Ok(st) = engine.folder_stats(&f.path).await {
                        stats.insert(key, st);
                    }
                }
            }
        }
        *self.managed_roots.write() = roots;
        *self.managed_stats.write() = stats;
        *self.managed_policies.write() = policies;
    }

    pub(super) fn managed_root_for_path(&self, path: &str) -> Option<String> {
        self.managed_roots
            .read()
            .iter()
            .find(|root| path.starts_with(root.as_str()))
            .cloned()
    }

    pub(super) async fn register_managed_folder(
        &self,
        folder: &orchid_fs::FsPath,
    ) -> WidgetResult<()> {
        let Some(engine) = self.deps.managed.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-managed-unavailable".into(),
            ));
        };
        let cfg = orchid_fs::ManagedFolderConfig {
            path: folder.clone(),
            chunk_size: orchid_crypto::ChunkerConfig::default(),
            enabled: true,
            auto_ingest: true,
            policy: None,
        };
        engine.add_folder(cfg).await.map_err(map_fs_error)?;
        Ok(())
    }

    pub(super) async fn add_selection_to_managed(&self, paths: &[String]) -> WidgetResult<()> {
        let folder = self.resolve_managed_folder_target(paths).await?;
        self.register_managed_folder(&folder).await?;
        if let Some(engine) = self.deps.managed.as_ref() {
            for p in paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                if let Some(provider) = self.deps.registry.for_path(&fp) {
                    if let Ok(meta) = provider.metadata(&fp).await {
                        if matches!(meta.kind, orchid_fs::FsEntryKind::File) {
                            if let Err(e) = engine.ingest(&fp).await {
                                warn!(error = %e, path = %p, "managed ingest failed");
                            }
                        }
                    }
                }
            }
        }
        self.refresh_managed_roots().await;
        self.set_activity_notice("fm-managed-added", None);
        Ok(())
    }

    pub(super) async fn remove_selection_from_managed(&self, paths: &[String]) -> WidgetResult<()> {
        let Some(engine) = self.deps.managed.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-managed-unavailable".into(),
            ));
        };
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            engine.remove_folder(&fp).await.map_err(map_fs_error)?;
        }
        self.refresh_managed_roots().await;
        self.set_activity_notice("fm-managed-removed", None);
        Ok(())
    }

    pub(super) async fn resolve_managed_folder_target(
        &self,
        paths: &[String],
    ) -> WidgetResult<orchid_fs::FsPath> {
        if paths.is_empty() {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-managed-no-selection".into(),
            ));
        }
        let mut folder_candidates: Vec<orchid_fs::FsPath> = Vec::new();
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            if let Some(provider) = self.deps.registry.for_path(&fp) {
                if let Ok(meta) = provider.metadata(&fp).await {
                    if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                        folder_candidates.push(fp);
                        continue;
                    }
                }
            }
            let parent = fp.parent().ok_or_else(|| {
                WidgetError::InvalidStateForOperation("fm-no-parent-folder".into())
            })?;
            folder_candidates.push(parent);
        }
        let first = folder_candidates[0].as_str();
        if !folder_candidates.iter().all(|f| f.as_str() == first) {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-selection-multiple-folders".into(),
            ));
        }
        Ok(folder_candidates[0].clone())
    }
}
