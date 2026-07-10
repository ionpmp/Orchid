//! Password-vault handlers for [`MainWindowController`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::SharedString;
use tracing::warn;
use uuid::Uuid;


use crate::window::errors::password_localized_error;
use crate::window::spawn;
use crate::window::models::PasswordAddDialogOverlay;


use super::{MainWindowController, PasswordCopyKind};

impl MainWindowController {
        pub(super) fn touch_vault_activity(self: &Arc<Self>) {
        *self.vault_last_activity.lock() = Some(Instant::now());
    }
        pub(super) fn check_vault_auto_lock(self: &Arc<Self>) {
        let timeout_secs = self.config.read().privacy.vault_auto_lock_seconds;
        if timeout_secs == 0 {
            return;
        }
        if !self.password_vault.is_unlocked() {
            return;
        }
        let mut last = self.vault_last_activity.lock();
        let Some(at) = *last else {
            // Unlocked without a recorded touch (e.g. restored session) — start the timer now.
            *last = Some(Instant::now());
            return;
        };
        if at.elapsed() >= Duration::from_secs(u64::from(timeout_secs)) {
            drop(last);
            self.push_notification(
                &self.locale.tr("widget-password-name"),
                &self.locale.tr("password-locked"),
                0,
            );
            self.on_password_lock_vault();
        }
    }
        pub(super) fn on_password_search_changed(self: &Arc<Self>, q: &SharedString) {
        let query = q.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        orchid_widgets::builtin::password::update_search(inst_id, query);
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }
        pub(super) fn on_password_entry_clicked(self: &Arc<Self>, id: &SharedString) {
        let entry_id = id.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        orchid_widgets::builtin::password::select_entry(inst_id, entry_id);
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }
        pub(super) fn on_password_copy(self: &Arc<Self>, id: &SharedString, kind: PasswordCopyKind) {
        let entry_id = id.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        let clear_clipboard_secs = self.config.read().privacy.clear_clipboard_seconds;
        let t = Arc::downgrade(self);
        let locale = self.locale.clone();
        spawn::spawn_local_compat(async move {
            let toast_key = match kind {
                PasswordCopyKind::Password => {
                    match orchid_widgets::builtin::password::copy_password(
                        inst_id,
                        &entry_id,
                        clear_clipboard_secs,
                    )
                    .await
                    {
                        Ok(()) => "password-password-copied",
                        Err(e) => {
                            warn!(?e, "copy password");
                            return;
                        }
                    }
                }
                PasswordCopyKind::Username => {
                    match orchid_widgets::builtin::password::copy_username(inst_id, &entry_id).await
                    {
                        Ok(()) => "password-username-copied",
                        Err(e) => {
                            warn!(?e, "copy username");
                            return;
                        }
                    }
                }
                PasswordCopyKind::Totp => {
                    match orchid_widgets::builtin::password::copy_totp(
                        inst_id,
                        &entry_id,
                        clear_clipboard_secs,
                    )
                    .await
                    {
                        Ok(()) => "password-totp-copied",
                        Err(e) => {
                            warn!(?e, "copy totp");
                            return;
                        }
                    }
                }
            };

            let Some(c) = t.upgrade() else {
                return;
            };
            let msg = locale.tr(toast_key).to_string();
            let title = locale.tr("widget-password-name");
            c.password_toasts.write().insert(inst_id, (msg.clone(), true));
            c.push_notification(&title, &msg, 1);
            c.schedule_rebuild();

            let t2 = Arc::downgrade(&c);
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if let Some(cc) = t2.upgrade() {
                cc.password_toasts.write().remove(&inst_id);
                cc.schedule_rebuild();
            }
        });
    }
        pub(super) fn on_password_open_url(self: &Arc<Self>, url: &SharedString) {
        let url_str = url.to_string();
        if url_str.is_empty() {
            return;
        }
        if let Err(e) = opener::open(&url_str) {
            warn!(?e, "failed to open URL");
        }
    }
        pub(super) fn on_password_unlock_submit(self: &Arc<Self>, passphrase: &SharedString) {
        let pass = passphrase.to_string();
        if pass.is_empty() {
            return;
        }
        let vault = self.password_vault.clone();
        let bus = self.bus.clone();
        match orchid_widgets::builtin::password::unlock_with_passphrase(vault, bus, &pass) {
            Ok(()) => self.touch_vault_activity(),
            Err(e) => orchid_widgets::builtin::password::record_unlock_error(e),
        }
        self.schedule_rebuild_after_password_unlock();
    }
        pub(super) fn on_password_unlock_biometric(self: &Arc<Self>) {
        let prompt = self.locale.tr("password-unlock-biometric-prompt");
        let vault = self.password_vault.clone();
        let bus = self.bus.clone();
        match orchid_widgets::builtin::password::unlock_with_biometric(vault, bus, &prompt) {
            Ok(()) => self.touch_vault_activity(),
            Err(e) => orchid_widgets::builtin::password::record_unlock_error(e),
        }
        self.schedule_rebuild_after_password_unlock();
    }
        pub(super) fn schedule_rebuild_after_password_unlock(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_password_widget() else {
            self.schedule_rebuild();
            return;
        };
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }
        pub(super) fn on_password_lock_vault(self: &Arc<Self>) {
        orchid_widgets::builtin::password::lock_vault(
            self.password_vault.clone(),
            self.bus.clone(),
        );
        *self.vault_last_activity.lock() = None;
        self.schedule_rebuild_after_password_unlock();
    }
        pub(super) fn on_password_add_entry_request(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        self.password_add_dialogs.write().insert(
            inst_id,
            PasswordAddDialogOverlay {
                visible: true,
                error: None,
                request_autofocus: true,
                ..Default::default()
            },
        );
        self.schedule_rebuild_after_password_unlock();
    }
        pub(super) fn on_password_add_entry_commit(
        self: &Arc<Self>,
        title: &SharedString,
        username: &SharedString,
        password: &SharedString,
        url: &SharedString,
    ) {
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        let url_opt = if url.is_empty() {
            None
        } else {
            Some(url.to_string())
        };
        match orchid_widgets::builtin::password::create_entry(
            inst_id,
            self.password_vault.clone(),
            title.to_string(),
            username.to_string(),
            password.to_string(),
            url_opt,
        ) {
            Ok(_) => {
                self.password_add_dialogs.write().remove(&inst_id);
                let msg = self.locale.tr("password-entry-added");
                self.password_toasts.write().insert(inst_id, (msg, true));
                self.schedule_rebuild_after_password_unlock();
                let t = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if let Some(c) = t.upgrade() {
                        c.password_toasts.write().remove(&inst_id);
                        c.schedule_rebuild_after_password_unlock();
                    }
                });
            }
            Err(e) => {
                let error = password_localized_error(&self.locale, &e);
                self.password_add_dialogs.write().insert(
                    inst_id,
                    PasswordAddDialogOverlay {
                        visible: true,
                        error: Some(error),
                        request_autofocus: false,
                        ..self
                            .password_add_dialogs
                            .read()
                            .get(&inst_id)
                            .cloned()
                            .unwrap_or_default()
                    },
                );
                self.schedule_rebuild_after_password_unlock();
            }
        }
    }
        pub(super) fn on_password_add_entry_cancel(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.password_add_dialogs.write().remove(&inst_id);
        self.schedule_rebuild_after_password_unlock();
    }
        pub(super) fn on_password_add_entry_generate_password(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        let password = orchid_crypto::generate_password(orchid_crypto::DEFAULT_PASSWORD_LENGTH)
            .unwrap_or_default();
        let mut overlay = self
            .password_add_dialogs
            .read()
            .get(&inst_id)
            .cloned()
            .unwrap_or_default();
        overlay.visible = true;
        overlay.generation_seq = overlay.generation_seq.saturating_add(1);
        overlay.generated_password = Some(password);
        self.password_add_dialogs.write().insert(inst_id, overlay);
        self.schedule_rebuild_after_password_unlock();
    }
        pub(super) fn find_active_password_widget(&self) -> Option<Uuid> {
        let w = self.workspace_manager.active().ok()?;
        for inst in self.widget_manager.instances_for_workspace(w.id) {
            if inst.type_id == "password-manager" {
                return Some(inst.id);
            }
        }
        None
    }
}
