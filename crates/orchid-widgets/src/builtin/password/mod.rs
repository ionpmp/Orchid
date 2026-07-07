//! Password-manager widget.
//!
//! Supports list, search, copy, and creating new entries. Edit / generate UIs
//! ship in a later task. The database is expected to already be unlocked —
//! the unlock dialog is a separate piece of UI that the widget does not own.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use parking_lot::RwLock;
use secrecy::ExposeSecret;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::payloads::{
    PasswordEntryDetailView, PasswordEntryView, PasswordManagerPayload,
};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};

/// Stable type id.
pub const TYPE_ID: &str = "password-manager";

/// Live password widget cores keyed by instance id (for UI-side callbacks
/// without holding `WidgetManager` locks).
static PASSWORD_LIVE: LazyLock<DashMap<Uuid, Arc<PasswordHandle>>> =
    LazyLock::new(DashMap::new);

/// Push a search query into the live password widget.
pub fn update_search(instance_id: Uuid, query: String) {
    if let Some(h) = PASSWORD_LIVE.get(&instance_id) {
        h.on_search_changed(query);
    }
}

/// Select an entry by id (UUID string) on the live password widget.
pub fn select_entry(instance_id: Uuid, entry_id: String) {
    if let Some(h) = PASSWORD_LIVE.get(&instance_id) {
        h.on_entry_clicked(entry_id.as_str());
    }
}

/// Copy password (30s auto-clear) for `entry_id`.
pub async fn copy_password(instance_id: Uuid, entry_id: &str) -> Result<(), String> {
    let inner = PASSWORD_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "password widget not live".to_string())?;
    inner.copy_password(entry_id).await
}

/// Copy username (no auto-clear) for `entry_id`.
pub async fn copy_username(instance_id: Uuid, entry_id: &str) -> Result<(), String> {
    let inner = PASSWORD_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "password widget not live".to_string())?;
    inner.copy_username(entry_id).await
}

/// Copy TOTP (30s auto-clear) for `entry_id`.
pub async fn copy_totp(instance_id: Uuid, entry_id: &str) -> Result<(), String> {
    let inner = PASSWORD_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "password widget not live".to_string())?;
    inner.copy_totp(entry_id).await
}

/// Create a new vault entry, persist to disk, and refresh the live widget.
pub fn create_entry(
    instance_id: Uuid,
    vault: Arc<orchid_crypto::PasswordVault>,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
) -> Result<Uuid, String> {
    let inner = PASSWORD_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "password widget not live".to_string())?;
    inner.create_entry(vault, title, username, password, url)
}

/// Unlock the vault with a master passphrase and refresh live widgets.
pub fn unlock_with_passphrase(
    vault: Arc<orchid_crypto::PasswordVault>,
    bus: Arc<orchid_core::EventBus>,
    passphrase: &str,
) -> Result<(), String> {
    use secrecy::SecretString;
    vault
        .unlock_with_passphrase(SecretString::new(passphrase.to_string()))
        .map_err(|e| e.to_string())?;
    notify_vault_unlocked(&vault, &bus);
    Ok(())
}

/// Unlock the vault via Windows Hello and refresh live widgets.
pub fn unlock_with_biometric(
    vault: Arc<orchid_crypto::PasswordVault>,
    bus: Arc<orchid_core::EventBus>,
    prompt: &str,
) -> Result<(), String> {
    vault
        .unlock_with_biometric(prompt)
        .map_err(|e| e.to_string())?;
    notify_vault_unlocked(&vault, &bus);
    Ok(())
}

/// Lock the vault and refresh live password widgets.
pub fn lock_vault(vault: Arc<orchid_crypto::PasswordVault>, bus: Arc<orchid_core::EventBus>) {
    vault.lock();
    notify_vault_locked(&bus);
}

fn notify_vault_locked(bus: &orchid_core::EventBus) {
    for entry in PASSWORD_LIVE.iter() {
        entry.value().on_vault_locked();
        bus.publish(
            orchid_core::EventSource::Widget(*entry.key()),
            WidgetSnapshotUpdated {
                instance_id: *entry.key(),
            },
        );
    }
}

fn notify_vault_unlocked(vault: &orchid_crypto::PasswordVault, bus: &orchid_core::EventBus) {
    for entry in PASSWORD_LIVE.iter() {
        entry.value().on_vault_unlocked(vault);
        bus.publish(
            orchid_core::EventSource::Widget(*entry.key()),
            WidgetSnapshotUpdated {
                instance_id: *entry.key(),
            },
        );
    }
}

fn set_unlock_error(message: String) {
    for entry in PASSWORD_LIVE.iter() {
        entry.value().state.write().unlock_error = Some(message.clone());
        entry.value().bus.publish(
            orchid_core::EventSource::Widget(*entry.key()),
            WidgetSnapshotUpdated {
                instance_id: *entry.key(),
            },
        );
    }
}

/// Record a failed unlock attempt on live widgets.
pub fn record_unlock_error(message: String) {
    set_unlock_error(message);
}

/// Handle to the shared password vault + clipboard.
#[derive(Clone)]
pub struct PasswordDeps {
    /// Locked/unlocked vault.
    pub vault: Arc<orchid_crypto::PasswordVault>,
    /// Secure clipboard used for auto-clearing copies.
    pub clipboard: Arc<dyn orchid_crypto::SecureClipboard>,
}

impl std::fmt::Debug for PasswordDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordDeps").finish_non_exhaustive()
    }
}

/// Password-manager widget.
pub struct PasswordManagerWidget {
    inner: Arc<PasswordHandle>,
    refresh: PeriodicRefresh,
}

#[derive(Default, Clone, Debug)]
struct State {
    entries: Vec<orchid_crypto::PasswordEntry>,
    search_query: String,
    selected_id: Option<Uuid>,
    error: Option<String>,
    unlock_error: Option<String>,
}

struct PasswordHandle {
    instance_id: Uuid,
    deps: PasswordDeps,
    state: Arc<RwLock<State>>,
    bus: Arc<orchid_core::EventBus>,
}

impl std::fmt::Debug for PasswordManagerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordManagerWidget")
            .field("instance_id", &self.inner.instance_id)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for PasswordHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordHandle")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl PasswordManagerWidget {
    /// Construct over an unlocked database.
    pub fn new(instance_id: Uuid, deps: PasswordDeps, bus: Arc<orchid_core::EventBus>) -> Self {
        let inner = Arc::new(PasswordHandle {
            instance_id,
            deps,
            state: Arc::new(RwLock::new(State::default())),
            bus,
        });
        PASSWORD_LIVE.insert(instance_id, Arc::clone(&inner));
        Self {
            inner,
            refresh: PeriodicRefresh::new(Duration::from_secs(1)),
        }
    }
}

impl PasswordHandle {
    fn on_vault_unlocked(&self, vault: &orchid_crypto::PasswordVault) {
        self.state.write().unlock_error = None;
        if vault.is_unlocked() {
            self.refresh_entries(None);
        }
    }

    fn on_vault_locked(&self) {
        let mut state = self.state.write();
        state.entries.clear();
        state.selected_id = None;
        state.error = None;
        state.unlock_error = None;
        state.search_query.clear();
    }

    fn refresh_entries(&self, query: Option<String>) {
        let Some(db) = self.deps.vault.database() else {
            let mut state = self.state.write();
            state.entries.clear();
            state.selected_id = None;
            state.error = None;
            if let Some(new_q) = query {
                state.search_query = new_q;
            }
            return;
        };
        let q = match &query {
            Some(q) => q.clone(),
            None => self.state.read().search_query.clone(),
        };
        let (entries, err) = if q.trim().is_empty() {
            match db.list_entries(None) {
                Ok(v) => (v, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            }
        } else {
            let search = orchid_crypto::SearchQuery {
                text: Some(q.clone()),
                limit: Some(200),
                ..Default::default()
            };
            match db.search(&search) {
                Ok(hits) => (hits.into_iter().map(|h| h.entry).collect::<Vec<_>>(), None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            }
        };
        let mut state = self.state.write();
        state.entries = entries;
        state.error = err;
        if let Some(new_q) = query {
            state.search_query = new_q;
        }
    }

    fn on_search_changed(&self, query: String) {
        self.refresh_entries(Some(query));
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated { instance_id: self.instance_id },
        );
    }

    fn on_entry_clicked(&self, id_str: &str) {
        let Ok(id) = Uuid::parse_str(id_str) else {
            return;
        };
        self.state.write().selected_id = Some(id);
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated { instance_id: self.instance_id },
        );
    }

    async fn copy_password(&self, id_str: &str) -> Result<(), String> {
        let db = self
            .deps
            .vault
            .database()
            .ok_or_else(|| "vault locked".to_string())?;
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        let entry = db
            .get_entry(id)
            .map_err(|e| e.to_string())?;
        self.deps
            .clipboard
            .copy_with_auto_clear(entry.password.clone(), Duration::from_secs(30))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn copy_username(&self, id_str: &str) -> Result<(), String> {
        let db = self
            .deps
            .vault
            .database()
            .ok_or_else(|| "vault locked".to_string())?;
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        let entry = db.get_entry(id).map_err(|e| e.to_string())?;
        self.deps
            .clipboard
            .copy_with_auto_clear(
                secrecy::SecretString::new(entry.username),
                Duration::from_secs(60 * 60 * 24 * 365),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn copy_totp(&self, id_str: &str) -> Result<(), String> {
        let db = self
            .deps
            .vault
            .database()
            .ok_or_else(|| "vault locked".to_string())?;
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        let entry = db.get_entry(id).map_err(|e| e.to_string())?;
        let Some(cfg) = entry.totp else {
            return Err("no TOTP".into());
        };
        let code = orchid_crypto::generate_code(&cfg, Utc::now()).map_err(|e| e.to_string())?;
        self.deps
            .clipboard
            .copy_with_auto_clear(
                secrecy::SecretString::new(code.code),
                Duration::from_secs(30),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn create_entry(
        &self,
        vault: Arc<orchid_crypto::PasswordVault>,
        title: String,
        username: String,
        password: String,
        url: Option<String>,
    ) -> Result<Uuid, String> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err("title required".into());
        }
        let db = vault.database().ok_or_else(|| "vault locked".to_string())?;
        let group_id = db.root_group().map_err(|e| e.to_string())?.id;
        let now = Utc::now();
        let id = Uuid::new_v4();
        let entry = orchid_crypto::PasswordEntry {
            id,
            title,
            username,
            password: secrecy::SecretString::new(password),
            url: url.filter(|s| !s.trim().is_empty()),
            notes: None,
            tags: Vec::new(),
            custom_fields: BTreeMap::new(),
            totp: None,
            created_at: now,
            modified_at: now,
            group_id,
        };
        db.add_entry(entry).map_err(|e| e.to_string())?;
        vault.persist().map_err(|e| e.to_string())?;
        self.refresh_entries(None);
        self.state.write().selected_id = Some(id);
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
        Ok(id)
    }
}

impl Drop for PasswordManagerWidget {
    fn drop(&mut self) {
        self.refresh.stop();
        PASSWORD_LIVE.remove(&self.inner.instance_id);
    }
}

#[async_trait]
impl Widget for PasswordManagerWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.inner.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.inner.refresh_entries(None);
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let inner = Arc::clone(&self.inner);
        let instance_id = self.inner.instance_id;
        // One-second ticker — only fires a snapshot event when an entry
        // with TOTP is selected (so the countdown updates).
        self.refresh.start(move || {
            let inner = Arc::clone(&inner);
            async move {
                let need_tick = inner.state.read().selected_id.is_some();
                if need_tick {
                    inner.bus.publish(
                        orchid_core::EventSource::Widget(instance_id),
                        WidgetSnapshotUpdated { instance_id },
                    );
                }
            }
        });
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let state = self.inner.state.read().clone();
        let payload = build_payload(&state, &self.inner.deps.vault);
        Some(WidgetSnapshot {
            instance_id: self.inner.instance_id,
            widget_type: TYPE_ID,
            title: "Passwords".into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::PasswordManager(payload),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        Ok(Vec::new())
    }
    fn restore_state(&mut self, _bytes: &[u8]) -> WidgetResult<()> {
        Ok(())
    }
    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: false,
            keeps_state_when_unloaded: false,
            has_settings_panel: false,
        }
    }
}

fn build_payload(state: &State, vault: &orchid_crypto::PasswordVault) -> PasswordManagerPayload {
    let entries = state
        .entries
        .iter()
        .map(|e| PasswordEntryView {
            id: e.id.to_string(),
            title: e.title.clone(),
            username: e.username.clone(),
            url_host: e.url.as_deref().and_then(extract_host),
            has_totp: e.totp.is_some(),
            tags: e.tags.clone(),
            color_label: None,
            modified_text: relative_from(e.modified_at),
        })
        .collect();
    let selected = state
        .selected_id
        .and_then(|id| state.entries.iter().find(|e| e.id == id))
        .map(build_detail);

    PasswordManagerPayload {
        is_unlocked: vault.is_unlocked() && state.error.is_none(),
        lock_reason: if vault.is_unlocked() {
            state.error.clone()
        } else {
            None
        },
        entries,
        selected,
        search_query: state.search_query.clone(),
        biometric_available: vault.biometric_unlock_available(),
        unlock_error: state.unlock_error.clone(),
    }
}

fn build_detail(e: &orchid_crypto::PasswordEntry) -> PasswordEntryDetailView {
    let (totp_code, totp_remaining) = e
        .totp
        .as_ref()
        .and_then(|cfg| orchid_crypto::generate_code(cfg, Utc::now()).ok())
        .map(|c| (Some(c.code), c.remaining_seconds))
        .unwrap_or((None, 0));
    // Touch ExposeSecret so clippy doesn't prune the import even in release
    // builds where notes aren't stored in SecretString form.
    let _ = e
        .custom_fields
        .values()
        .next()
        .map(|s| s.expose_secret().to_string());
    PasswordEntryDetailView {
        id: e.id.to_string(),
        title: e.title.clone(),
        username: e.username.clone(),
        url: e.url.clone(),
        notes: e.notes.clone(),
        totp_code,
        totp_remaining_seconds: totp_remaining,
        tags: e.tags.clone(),
    }
}

fn extract_host(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(String::from))
}

fn relative_from(at: chrono::DateTime<chrono::Utc>) -> String {
    let secs = (Utc::now() - at).num_seconds().max(0);
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Descriptor that wires a shared database + clipboard into every instance.
#[must_use]
pub fn descriptor(
    vault: Arc<orchid_crypto::PasswordVault>,
    clipboard: Arc<dyn orchid_crypto::SecureClipboard>,
) -> WidgetDescriptor {
    let deps = PasswordDeps { vault, clipboard };
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, _bytes| {
        Ok(Box::new(PasswordManagerWidget::new(
            ctx.instance_id,
            deps.clone(),
            ctx.bus.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-password-name",
        description_key: "widget-password-desc",
        icon_name: "password",
        category: WidgetCategory::Security,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: false,
        factory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_host_from_urls() {
        assert_eq!(extract_host("https://example.com/path"), Some("example.com".into()));
        assert_eq!(extract_host("notaurl"), None);
    }
}
