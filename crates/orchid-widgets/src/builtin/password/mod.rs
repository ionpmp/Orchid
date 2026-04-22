//! Password-manager widget.
//!
//! The widget is list + search + copy only; add / edit / generate UIs
//! ship in a later task. The database is expected to already be
//! unlocked — the unlock dialog is a separate piece of UI that the
//! widget does not own.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
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

/// Handle to the shared password database + clipboard.
#[derive(Clone)]
pub struct PasswordDeps {
    /// Unlocked KDBX database.
    pub database: Arc<orchid_crypto::PasswordDatabase>,
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
    instance_id: Uuid,
    deps: PasswordDeps,
    state: Arc<RwLock<State>>,
    refresh: PeriodicRefresh,
    bus: Arc<orchid_core::EventBus>,
}

#[derive(Default, Clone)]
struct State {
    entries: Vec<orchid_crypto::PasswordEntry>,
    search_query: String,
    selected_id: Option<Uuid>,
    error: Option<String>,
}

impl std::fmt::Debug for PasswordManagerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordManagerWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl PasswordManagerWidget {
    /// Construct over an unlocked database.
    pub fn new(instance_id: Uuid, deps: PasswordDeps, bus: Arc<orchid_core::EventBus>) -> Self {
        Self {
            instance_id,
            deps,
            state: Arc::new(RwLock::new(State::default())),
            refresh: PeriodicRefresh::new(Duration::from_secs(1)),
            bus,
        }
    }

    /// Refresh the cached entry list (optionally with a fresh search query).
    pub fn refresh_entries(&self, query: Option<String>) {
        let q = match &query {
            Some(q) => q.clone(),
            None => self.state.read().search_query.clone(),
        };
        let entries = if q.trim().is_empty() {
            self.deps
                .database
                .list_entries(None)
                .unwrap_or_default()
        } else {
            let search = orchid_crypto::SearchQuery {
                text: Some(q.clone()),
                limit: Some(200),
                ..Default::default()
            };
            self.deps
                .database
                .search(&search)
                .map(|hits| hits.into_iter().map(|h| h.entry).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        let mut state = self.state.write();
        state.entries = entries;
        if let Some(new_q) = query {
            state.search_query = new_q;
        }
    }

    /// Callback: user changed the search text.
    pub fn on_search_changed(&self, query: String) {
        self.refresh_entries(Some(query));
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    /// Callback: user clicked an entry row.
    pub fn on_entry_clicked(&self, id_str: &str) {
        let Ok(id) = Uuid::parse_str(id_str) else {
            return;
        };
        self.state.write().selected_id = Some(id);
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    /// Copy the entry's password to the clipboard (30 s auto-clear).
    ///
    /// # Errors
    ///
    /// Propagates KDBX lookup errors and clipboard failures.
    pub async fn copy_password(&self, id_str: &str) -> Result<(), String> {
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        let entry = self
            .deps
            .database
            .get_entry(id)
            .map_err(|e| e.to_string())?;
        self.deps
            .clipboard
            .copy_with_auto_clear(entry.password.clone(), Duration::from_secs(30))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Copy the entry's username without auto-clear.
    ///
    /// # Errors
    ///
    /// Propagates KDBX lookup errors and clipboard failures.
    pub async fn copy_username(&self, id_str: &str) -> Result<(), String> {
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        let entry = self
            .deps
            .database
            .get_entry(id)
            .map_err(|e| e.to_string())?;
        // Username isn't secret; copy without auto-clear.
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

    /// Copy the currently-valid TOTP code (30 s auto-clear).
    ///
    /// # Errors
    ///
    /// Returns the string `"no TOTP"` when the entry is not configured for
    /// TOTP, and propagates clipboard / database errors otherwise.
    pub async fn copy_totp(&self, id_str: &str) -> Result<(), String> {
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        let entry = self
            .deps
            .database
            .get_entry(id)
            .map_err(|e| e.to_string())?;
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

    /// Open the entry's URL in the default browser.
    ///
    /// # Errors
    ///
    /// Returns the string `"no url"` when the entry has no URL, and
    /// propagates `opener` errors otherwise.
    pub fn open_url(&self, id_str: &str) -> Result<(), String> {
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        let entry = self
            .deps
            .database
            .get_entry(id)
            .map_err(|e| e.to_string())?;
        let url = entry.url.ok_or_else(|| "no url".to_string())?;
        opener::open(url).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl Widget for PasswordManagerWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh_entries(None);
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let state = self.state.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        // One-second ticker — only fires a snapshot event when an entry
        // with TOTP is selected (so the countdown updates).
        self.refresh.start(move || {
            let state = state.clone();
            let bus = bus.clone();
            async move {
                let need_tick = state.read().selected_id.is_some();
                if need_tick {
                    bus.publish(
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
        let state = self.state.read().clone();
        let payload = build_payload(&state);
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
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

fn build_payload(state: &State) -> PasswordManagerPayload {
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
        is_unlocked: state.error.is_none(),
        lock_reason: state.error.clone(),
        entries,
        selected,
        search_query: state.search_query.clone(),
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
    database: Arc<orchid_crypto::PasswordDatabase>,
    clipboard: Arc<dyn orchid_crypto::SecureClipboard>,
) -> WidgetDescriptor {
    let deps = PasswordDeps { database, clipboard };
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
