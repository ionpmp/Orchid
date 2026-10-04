//! Mail client widget — accounts, folders, reading, compose, and setup wizard.

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use secrecy::SecretString;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{MailAccountRow, MailFolderRow, MailMessageRow, MailPayload};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_mail::{AuthKind, ComposeMessage, MailAccount, MailEngine, ServerEndpoint, TlsMode};
use orchid_storage::{LifecycleState, WidgetSize};

/// Stable type id.
pub const TYPE_ID: &str = "mail";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct MailPersisted {
    selected_account: Option<Uuid>,
    selected_folder: String,
    selected_uid: Option<u32>,
    allow_remote_images: bool,
}

struct MailHandle {
    instance_id: Uuid,
    engine: Arc<MailEngine>,
    bus: Arc<orchid_core::EventBus>,
    jobs: Arc<orchid_core::BackgroundJobQueue>,
    state: RwLock<UiState>,
}

#[derive(Debug, Clone)]
struct UiState {
    mode: i32,
    selected_account: Option<Uuid>,
    selected_folder: String,
    selected_uid: Option<u32>,
    reading_from: String,
    reading_to: String,
    reading_subject: String,
    reading_date: String,
    reading_text: String,
    reading_html: String,
    reading_has_html: bool,
    status: String,
    syncing: bool,
    wizard_email: String,
    wizard_display_name: String,
    wizard_password: String,
    wizard_imap_host: String,
    wizard_imap_port: String,
    wizard_smtp_host: String,
    wizard_smtp_port: String,
    wizard_tls_imap: String,
    wizard_tls_smtp: String,
    wizard_source: String,
    wizard_oauth_provider: String,
    wizard_error: String,
    oauth_port: Option<u16>,
    compose_to: String,
    compose_cc: String,
    compose_subject: String,
    compose_body: String,
    compose_in_reply_to: Option<String>,
    compose_references: Option<String>,
    search: String,
    search_hits: Option<Vec<orchid_mail::MessageHeader>>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            mode: 0,
            selected_account: None,
            selected_folder: "INBOX".into(),
            selected_uid: None,
            reading_from: String::new(),
            reading_to: String::new(),
            reading_subject: String::new(),
            reading_date: String::new(),
            reading_text: String::new(),
            reading_html: String::new(),
            reading_has_html: false,
            status: String::new(),
            syncing: false,
            wizard_email: String::new(),
            wizard_display_name: String::new(),
            wizard_password: String::new(),
            wizard_imap_host: String::new(),
            wizard_imap_port: "993".into(),
            wizard_smtp_host: String::new(),
            wizard_smtp_port: "587".into(),
            wizard_tls_imap: "implicit".into(),
            wizard_tls_smtp: "starttls".into(),
            wizard_source: String::new(),
            wizard_oauth_provider: String::new(),
            wizard_error: String::new(),
            oauth_port: None,
            compose_to: String::new(),
            compose_cc: String::new(),
            compose_subject: String::new(),
            compose_body: String::new(),
            compose_in_reply_to: None,
            compose_references: None,
            search: String::new(),
            search_hits: None,
        }
    }
}

static MAIL_LIVE: LazyLock<DashMap<Uuid, Arc<MailHandle>>> = LazyLock::new(DashMap::new);
static MAIL_ENGINE: LazyLock<RwLock<Option<Arc<MailEngine>>>> = LazyLock::new(|| RwLock::new(None));

fn job_key(instance_id: Uuid) -> String {
    format!("mail:{instance_id}")
}

impl MailHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn payload(&self) -> MailPayload {
        let st = self.state.read().clone();
        let accounts = self.engine.accounts();
        let selected = st
            .selected_account
            .or_else(|| accounts.first().map(|a| a.id));
        let account_rows: Vec<MailAccountRow> = accounts
            .iter()
            .map(|a| MailAccountRow {
                id: a.id.to_string(),
                label: if a.display_name.is_empty() {
                    a.email.clone()
                } else {
                    a.display_name.clone()
                },
                email: a.email.clone(),
                selected: Some(a.id) == selected,
            })
            .collect();

        let (folders, messages) = if let Some(aid) = selected {
            let folders = self
                .engine
                .folders(aid)
                .unwrap_or_default()
                .into_iter()
                .map(|f| MailFolderRow {
                    path: f.path.clone(),
                    name: f.name,
                    depth: f.depth as i32,
                    unread: f.unread as i32,
                    selected: f.path == st.selected_folder,
                })
                .collect();
            let headers = if st.search.trim().is_empty() {
                self.engine
                    .headers(aid, &st.selected_folder, 100)
                    .unwrap_or_default()
            } else if let Some(hits) = &st.search_hits {
                hits.clone()
            } else {
                let query = st.search.trim().to_lowercase();
                self.engine
                    .headers(aid, &st.selected_folder, 100)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|header| header_matches(header, &query))
                    .collect()
            };
            let rows = orchid_mail::arrange(&headers);
            let messages = rows
                .into_iter()
                .map(|row| {
                    let header = &headers[row.index];
                    MailMessageRow {
                        uid: header.uid as i32,
                        from: header.from.clone(),
                        subject: header.subject.clone(),
                        date: header.date.clone(),
                        snippet: header.snippet.clone(),
                        seen: header.seen,
                        flagged: header.flagged,
                        has_attachment: header.has_attachment,
                        selected: Some(header.uid) == st.selected_uid,
                        thread_indent: i32::from(row.indent),
                    }
                })
                .collect();
            (folders, messages)
        } else {
            (Vec::new(), Vec::new())
        };

        let mode = if accounts.is_empty() && st.mode == 0 {
            1
        } else {
            st.mode
        };

        MailPayload {
            mode,
            accounts: account_rows,
            folders,
            messages,
            selected_account_id: selected.map(|id| id.to_string()).unwrap_or_default(),
            selected_folder: st.selected_folder,
            selected_uid: st.selected_uid.map(|u| u as i32).unwrap_or(0),
            reading_from: st.reading_from,
            reading_to: st.reading_to,
            reading_subject: st.reading_subject,
            reading_date: st.reading_date,
            reading_text: st.reading_text,
            reading_has_html: st.reading_has_html,
            allow_remote_images: self.engine.allow_remote_images(),
            status: st.status,
            syncing: st.syncing,
            wizard_email: st.wizard_email,
            wizard_display_name: st.wizard_display_name,
            wizard_password: st.wizard_password,
            wizard_imap_host: st.wizard_imap_host,
            wizard_imap_port: st.wizard_imap_port,
            wizard_smtp_host: st.wizard_smtp_host,
            wizard_smtp_port: st.wizard_smtp_port,
            wizard_tls_imap: st.wizard_tls_imap,
            wizard_tls_smtp: st.wizard_tls_smtp,
            wizard_source: st.wizard_source,
            wizard_oauth_provider: st.wizard_oauth_provider,
            wizard_error: st.wizard_error,
            compose_to: st.compose_to,
            compose_cc: st.compose_cc,
            compose_subject: st.compose_subject,
            compose_body: st.compose_body,
            search_query: st.search,
        }
    }

    fn schedule_sync(self: &Arc<Self>) {
        let handle = Arc::clone(self);
        let key = job_key(self.instance_id);
        let interval = std::time::Duration::from_secs(120);
        self.jobs.schedule(key.clone(), interval, move || {
            let handle = Arc::clone(&handle);
            async move {
                handle.periodic().await;
            }
        });
    }

    fn pause_job(&self) {
        self.jobs.pause(&job_key(self.instance_id));
    }

    fn resume_job(self: &Arc<Self>) {
        let handle = Arc::clone(self);
        let key = job_key(self.instance_id);
        let interval = std::time::Duration::from_secs(120);
        self.jobs.resume(key.clone(), interval, move || {
            let handle = Arc::clone(&handle);
            async move {
                handle.periodic().await;
            }
        });
    }

    fn cancel_job(&self) {
        self.jobs.cancel(&job_key(self.instance_id));
    }

    async fn periodic(self: &Arc<Self>) {
        let key = job_key(self.instance_id);
        self.jobs
            .run_coalesced(&key, || {
                let handle = Arc::clone(self);
                async move {
                    handle.sync_now().await;
                }
            })
            .await;
        if self.watch_mailbox().await {
            self.jobs
                .run_coalesced(&key, || {
                    let handle = Arc::clone(self);
                    async move {
                        handle.sync_now().await;
                    }
                })
                .await;
        }
    }

    async fn watch_mailbox(&self) -> bool {
        let account = self
            .state
            .read()
            .selected_account
            .or_else(|| self.engine.accounts().first().map(|account| account.id));
        let Some(account_id) = account else {
            return false;
        };
        let folder = self.state.read().selected_folder.clone();
        match self
            .engine
            .idle_folder(account_id, &folder, std::time::Duration::from_secs(90))
            .await
        {
            Ok(changed) => changed,
            Err(_) => false,
        }
    }

    async fn sync_now(&self) {
        let account = self
            .state
            .read()
            .selected_account
            .or_else(|| self.engine.accounts().first().map(|a| a.id));
        let Some(account_id) = account else {
            return;
        };
        {
            let mut st = self.state.write();
            st.syncing = true;
            st.status = "syncing".into();
        }
        self.publish();
        let folder = self.state.read().selected_folder.clone();
        let result = self
            .engine
            .sync_account(account_id, Some(folder.as_str()))
            .await;
        {
            let mut st = self.state.write();
            st.syncing = false;
            st.status = match &result {
                Ok(()) => String::new(),
                Err(e) => e.to_string(),
            };
            if st.selected_account.is_none() {
                st.selected_account = Some(account_id);
            }
        }
        self.publish();
    }
}

/// Current HTML document for WebView2 (empty when plain-only).
#[must_use]
pub fn reading_html(instance_id: Uuid) -> Option<String> {
    let handle = MAIL_LIVE.get(&instance_id)?;
    let st = handle.state.read();
    if st.reading_html.is_empty() {
        None
    } else {
        Some(orchid_mail::sanitize_html_for_webview(
            &st.reading_html,
            handle.engine.allow_remote_images(),
        ))
    }
}

/// Select an account.
pub fn select_account(instance_id: Uuid, account_id: &str) {
    if let Some(h) = MAIL_LIVE.get(&instance_id) {
        if let Ok(id) = Uuid::parse_str(account_id) {
            h.state.write().selected_account = Some(id);
            h.state.write().selected_folder = "INBOX".into();
            h.state.write().selected_uid = None;
            h.publish();
            let handle = Arc::clone(h.value());
            let jobs = Arc::clone(&handle.jobs);
            let handle = Arc::clone(&handle);
            jobs.spawn_coalesced(job_key(instance_id), move || {
                let handle = Arc::clone(&handle);
                async move {
                    handle.sync_now().await;
                }
            });
        }
    }
}

/// Select a folder.
pub fn select_folder(instance_id: Uuid, folder: &str) {
    if let Some(h) = MAIL_LIVE.get(&instance_id) {
        h.state.write().selected_folder = folder.to_string();
        h.state.write().selected_uid = None;
        h.state.write().search.clear();
        h.state.write().search_hits = None;
        h.publish();
        let account = h.state.read().selected_account;
        let folder = folder.to_string();
        let handle = Arc::clone(h.value());
        let jobs = Arc::clone(&handle.jobs);
        let handle = Arc::clone(&handle);
        jobs.spawn_coalesced(job_key(instance_id), move || {
            let handle = Arc::clone(&handle);
            let folder = folder.clone();
            async move {
                if let Some(aid) = account {
                    let _ = handle.engine.sync_account(aid, Some(folder.as_str())).await;
                }
                handle.publish();
            }
        });
    }
}

/// Open a message.
pub fn select_message(instance_id: Uuid, uid: i32) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let account = h.state.read().selected_account;
    let folder = h.state.read().selected_folder.clone();
    let Some(account_id) = account else {
        return;
    };
    h.state.write().selected_uid = Some(uid as u32);
    h.publish();
    let handle = Arc::clone(h.value());
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-body:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let folder = folder.clone();
        async move {
            match handle
                .engine
                .message_body(account_id, &folder, uid as u32)
                .await
            {
                Ok(body) => {
                    let text = handle.engine.reading_text(&body);
                    let mut st = handle.state.write();
                    st.reading_text = text;
                    st.reading_html = body.html.clone();
                    st.reading_has_html = !body.html.trim().is_empty();
                    if let Ok(headers) = handle.engine.headers(account_id, &folder, 100) {
                        if let Some(h) = headers.into_iter().find(|h| h.uid == uid as u32) {
                            st.reading_from = h.from;
                            st.reading_to = h.to;
                            st.reading_subject = h.subject;
                            st.reading_date = h.date;
                        }
                    }
                    st.status.clear();
                }
                Err(e) => {
                    handle.state.write().status = e.to_string();
                }
            }
            handle.publish();
        }
    });
}

/// Refresh sync.
pub fn refresh(instance_id: Uuid) {
    if let Some(h) = MAIL_LIVE.get(&instance_id) {
        let handle = Arc::clone(h.value());
        let jobs = Arc::clone(&handle.jobs);
        let handle = Arc::clone(&handle);
        jobs.spawn_coalesced(job_key(instance_id), move || {
            let handle = Arc::clone(&handle);
            async move {
                handle.sync_now().await;
            }
        });
    }
}

/// Filter the open folder and ask IMAP for `TEXT` matches.
pub fn set_search(instance_id: Uuid, query: &str) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let query = query.to_string();
    {
        let mut state = h.state.write();
        state.search = query.clone();
        state.search_hits = None;
    }
    h.publish();
    let query = query.trim().to_string();
    if query.is_empty() {
        return;
    }
    let account = h.state.read().selected_account;
    let folder = h.state.read().selected_folder.clone();
    let handle = Arc::clone(h.value());
    let jobs = Arc::clone(&handle.jobs);
    jobs.spawn_coalesced(format!("mail-search:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let query = query.clone();
        let folder = folder.clone();
        async move {
            let Some(account_id) = account else {
                return;
            };
            match handle
                .engine
                .search_folder(account_id, &folder, &query)
                .await
            {
                Ok(hits) => {
                    let mut state = handle.state.write();
                    if state.search.trim() == query {
                        state.search_hits = Some(hits);
                    }
                }
                Err(err) => {
                    handle.state.write().status = err.to_string();
                }
            }
            handle.publish();
        }
    });
}

/// Open the account wizard.
pub fn open_wizard(instance_id: Uuid) {
    if let Some(h) = MAIL_LIVE.get(&instance_id) {
        h.state.write().mode = 1;
        h.publish();
    }
}

/// Close wizard / compose back to mailbox.
pub fn show_mailbox(instance_id: Uuid) {
    if let Some(h) = MAIL_LIVE.get(&instance_id) {
        h.state.write().mode = 0;
        h.publish();
    }
}

/// Update a wizard text field.
pub fn wizard_set(instance_id: Uuid, field: &str, value: &str) {
    if let Some(h) = MAIL_LIVE.get(&instance_id) {
        let mut st = h.state.write();
        match field {
            "email" => st.wizard_email = value.into(),
            "display_name" => st.wizard_display_name = value.into(),
            "password" => st.wizard_password = value.into(),
            "imap_host" => st.wizard_imap_host = value.into(),
            "imap_port" => st.wizard_imap_port = value.into(),
            "smtp_host" => st.wizard_smtp_host = value.into(),
            "smtp_port" => st.wizard_smtp_port = value.into(),
            "tls_imap" => st.wizard_tls_imap = value.into(),
            "tls_smtp" => st.wizard_tls_smtp = value.into(),
            _ => {}
        }
        drop(st);
        h.publish();
    }
}

/// Run autodiscover for the wizard email.
pub fn wizard_discover(instance_id: Uuid) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let email = h.state.read().wizard_email.clone();
    let handle = Arc::clone(h.value());
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-discover:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let email = email.clone();
        async move {
            match handle.engine.discover(&email).await {
                Ok(s) => {
                    let mut st = handle.state.write();
                    st.wizard_imap_host = s.imap.host;
                    st.wizard_imap_port = s.imap.port.to_string();
                    st.wizard_smtp_host = s.smtp.host;
                    st.wizard_smtp_port = s.smtp.port.to_string();
                    st.wizard_tls_imap = tls_label(s.imap.tls);
                    st.wizard_tls_smtp = tls_label(s.smtp.tls);
                    st.wizard_source = s.source;
                    st.wizard_oauth_provider = s.oauth_provider.unwrap_or_default();
                    st.wizard_error.clear();
                    if st.wizard_display_name.is_empty() {
                        st.wizard_display_name = email_local(&email);
                    }
                }
                Err(e) => {
                    handle.state.write().wizard_error = e.to_string();
                }
            }
            handle.publish();
        }
    });
}

/// Save password account from wizard fields.
pub fn wizard_save_password(instance_id: Uuid) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let st = h.state.read().clone();
    let handle = Arc::clone(h.value());
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-save:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let st = st.clone();
        async move {
            let mut account = MailAccount::new(
                st.wizard_email.clone(),
                if st.wizard_display_name.is_empty() {
                    email_local(&st.wizard_email)
                } else {
                    st.wizard_display_name.clone()
                },
            );
            account.imap = ServerEndpoint {
                host: st.wizard_imap_host.clone(),
                port: st.wizard_imap_port.parse().unwrap_or(993),
                tls: parse_tls(&st.wizard_tls_imap),
                username: st.wizard_email.clone(),
            };
            account.smtp = ServerEndpoint {
                host: st.wizard_smtp_host.clone(),
                port: st.wizard_smtp_port.parse().unwrap_or(587),
                tls: parse_tls(&st.wizard_tls_smtp),
                username: st.wizard_email.clone(),
            };
            match handle
                .engine
                .add_password_account(
                    account,
                    SecretString::from(st.wizard_password.clone()),
                    false,
                )
                .await
            {
                Ok(saved) => {
                    {
                        let mut s = handle.state.write();
                        s.mode = 0;
                        s.selected_account = Some(saved.id);
                        s.wizard_password.clear();
                        s.wizard_error.clear();
                        s.status.clear();
                    }
                    handle.sync_now().await;
                }
                Err(e) => {
                    handle.state.write().wizard_error = e.to_string();
                    handle.publish();
                }
            }
        }
    });
}

/// Begin OAuth for the wizard's suggested provider.
pub fn wizard_oauth_start(instance_id: Uuid) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let provider = h.state.read().wizard_oauth_provider.clone();
    if provider.is_empty() {
        h.state.write().wizard_error = "oauth-provider-missing".into();
        h.publish();
        return;
    }
    let handle = Arc::clone(h.value());
    let email = h.state.read().wizard_email.clone();
    let display = h.state.read().wizard_display_name.clone();
    let imap_host = h.state.read().wizard_imap_host.clone();
    let imap_port = h.state.read().wizard_imap_port.clone();
    let smtp_host = h.state.read().wizard_smtp_host.clone();
    let smtp_port = h.state.read().wizard_smtp_port.clone();
    let tls_imap = h.state.read().wizard_tls_imap.clone();
    let tls_smtp = h.state.read().wizard_tls_smtp.clone();
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-oauth:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let email = email.clone();
        let display = display.clone();
        let provider = provider.clone();
        let imap_host = imap_host.clone();
        let imap_port = imap_port.clone();
        let smtp_host = smtp_host.clone();
        let smtp_port = smtp_port.clone();
        let tls_imap = tls_imap.clone();
        let tls_smtp = tls_smtp.clone();
        async move {
            match handle.engine.begin_oauth(&provider).await {
                Ok((url, port, rx)) => {
                    handle.state.write().oauth_port = Some(port);
                    handle.state.write().wizard_error = "oauth-waiting".into();
                    handle.publish();
                    let _ = opener::open(&url);
                    match rx.await {
                        Ok(code) => {
                            let mut account = MailAccount::new(
                                email.clone(),
                                if display.is_empty() {
                                    email_local(&email)
                                } else {
                                    display
                                },
                            );
                            account.imap = ServerEndpoint {
                                host: imap_host,
                                port: imap_port.parse().unwrap_or(993),
                                tls: parse_tls(&tls_imap),
                                username: email.clone(),
                            };
                            account.smtp = ServerEndpoint {
                                host: smtp_host,
                                port: smtp_port.parse().unwrap_or(587),
                                tls: parse_tls(&tls_smtp),
                                username: email,
                            };
                            account.auth = AuthKind::Oauth2;
                            match handle
                                .engine
                                .finish_oauth_account(account, &provider, &code, port)
                                .await
                            {
                                Ok(saved) => {
                                    {
                                        let mut s = handle.state.write();
                                        s.mode = 0;
                                        s.selected_account = Some(saved.id);
                                        s.wizard_error.clear();
                                        s.oauth_port = None;
                                    }
                                    handle.sync_now().await;
                                }
                                Err(e) => {
                                    handle.state.write().wizard_error = e.to_string();
                                    handle.publish();
                                }
                            }
                        }
                        Err(_) => {
                            handle.state.write().wizard_error = "oauth-cancelled".into();
                            handle.publish();
                        }
                    }
                }
                Err(e) => {
                    handle.state.write().wizard_error = e.to_string();
                    handle.publish();
                }
            }
        }
    });
}

/// Toggle remote images.
pub fn set_allow_remote_images(instance_id: Uuid, allow: bool) {
    if let Some(h) = MAIL_LIVE.get(&instance_id) {
        h.engine.set_allow_remote_images(allow);
        h.publish();
    }
}

/// Flag / unflag.
pub fn toggle_flagged(instance_id: Uuid) {
    message_flag_action(instance_id, true);
}

fn message_flag_action(instance_id: Uuid, flagged_toggle: bool) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let st = h.state.read().clone();
    let Some(account_id) = st.selected_account else {
        return;
    };
    let Some(uid) = st.selected_uid else {
        return;
    };
    let currently = h
        .engine
        .headers(account_id, &st.selected_folder, 100)
        .ok()
        .and_then(|hs| hs.into_iter().find(|m| m.uid == uid).map(|m| m.flagged))
        .unwrap_or(false);
    let handle = Arc::clone(h.value());
    let folder = st.selected_folder;
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-flag:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let folder = folder.clone();
        async move {
            if flagged_toggle {
                let _ = handle
                    .engine
                    .set_flagged(account_id, &folder, uid, !currently)
                    .await;
            }
            let _ = handle.engine.sync_account(account_id, Some(&folder)).await;
            handle.publish();
        }
    });
}

/// Mark selected message seen/unseen.
pub fn toggle_seen(instance_id: Uuid) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let st = h.state.read().clone();
    let Some(account_id) = st.selected_account else {
        return;
    };
    let Some(uid) = st.selected_uid else {
        return;
    };
    let currently = h
        .engine
        .headers(account_id, &st.selected_folder, 100)
        .ok()
        .and_then(|hs| hs.into_iter().find(|m| m.uid == uid).map(|m| m.seen))
        .unwrap_or(true);
    let handle = Arc::clone(h.value());
    let folder = st.selected_folder;
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-seen:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let folder = folder.clone();
        async move {
            let _ = handle
                .engine
                .set_seen(account_id, &folder, uid, !currently)
                .await;
            handle.publish();
        }
    });
}

/// Move selected message to Trash.
pub fn delete_selected(instance_id: Uuid) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let st = h.state.read().clone();
    let Some(account_id) = st.selected_account else {
        return;
    };
    let Some(uid) = st.selected_uid else {
        return;
    };
    let trash = h
        .engine
        .folders(account_id)
        .ok()
        .and_then(|fs| {
            fs.into_iter()
                .find(|f| f.role.as_deref() == Some("trash"))
                .map(|f| f.path)
        })
        .unwrap_or_else(|| "Trash".into());
    let handle = Arc::clone(h.value());
    let folder = st.selected_folder;
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-del:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let folder = folder.clone();
        let trash = trash.clone();
        async move {
            let _ = handle
                .engine
                .move_message(account_id, &folder, uid, &trash)
                .await;
            {
                let mut s = handle.state.write();
                s.selected_uid = None;
                s.reading_text.clear();
                s.reading_html.clear();
            }
            let _ = handle.engine.sync_account(account_id, Some(&folder)).await;
            handle.publish();
        }
    });
}

/// Open compose mode.
pub fn open_compose(instance_id: Uuid, kind: &str) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let st = h.state.read().clone();
    let mut next = st.clone();
    next.mode = 2;
    match kind {
        "reply" => {
            next.compose_to = st.reading_from.clone();
            next.compose_subject = if st.reading_subject.to_lowercase().starts_with("re:") {
                st.reading_subject.clone()
            } else {
                format!("Re: {}", st.reading_subject)
            };
            next.compose_body = format!(
                "\n\nOn {} {} wrote:\n{}",
                st.reading_date, st.reading_from, st.reading_text
            );
        }
        "reply-all" => {
            next.compose_to = st.reading_from.clone();
            next.compose_cc = st.reading_to.clone();
            next.compose_subject = if st.reading_subject.to_lowercase().starts_with("re:") {
                st.reading_subject.clone()
            } else {
                format!("Re: {}", st.reading_subject)
            };
            next.compose_body = format!(
                "\n\nOn {} {} wrote:\n{}",
                st.reading_date, st.reading_from, st.reading_text
            );
        }
        "forward" => {
            next.compose_to.clear();
            next.compose_subject = if st.reading_subject.to_lowercase().starts_with("fwd:") {
                st.reading_subject.clone()
            } else {
                format!("Fwd: {}", st.reading_subject)
            };
            next.compose_body = format!(
                "\n\n---------- Forwarded message ----------\nFrom: {}\nDate: {}\nSubject: {}\n\n{}",
                st.reading_from, st.reading_date, st.reading_subject, st.reading_text
            );
        }
        _ => {
            next.compose_to.clear();
            next.compose_cc.clear();
            next.compose_subject.clear();
            next.compose_body.clear();
        }
    }
    *h.state.write() = next;
    h.publish();
}

/// Update compose fields.
pub fn compose_set(instance_id: Uuid, field: &str, value: &str) {
    if let Some(h) = MAIL_LIVE.get(&instance_id) {
        let mut st = h.state.write();
        match field {
            "to" => st.compose_to = value.into(),
            "cc" => st.compose_cc = value.into(),
            "subject" => st.compose_subject = value.into(),
            "body" => st.compose_body = value.into(),
            _ => {}
        }
        drop(st);
        h.publish();
    }
}

/// Send the compose draft.
pub fn compose_send(instance_id: Uuid) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let st = h.state.read().clone();
    let account_id = st
        .selected_account
        .or_else(|| h.engine.accounts().first().map(|a| a.id));
    let Some(account_id) = account_id else {
        return;
    };
    let compose = ComposeMessage {
        account_id: Some(account_id),
        to: st.compose_to,
        cc: st.compose_cc,
        bcc: String::new(),
        subject: st.compose_subject,
        body: st.compose_body,
        attachments: Vec::new(),
        in_reply_to: st.compose_in_reply_to,
        references: st.compose_references,
    };
    let handle = Arc::clone(h.value());
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-send:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let compose = compose.clone();
        async move {
            match handle.engine.send(&compose).await {
                Ok(()) => {
                    let mut s = handle.state.write();
                    s.mode = 0;
                    s.status.clear();
                    s.compose_to.clear();
                    s.compose_cc.clear();
                    s.compose_subject.clear();
                    s.compose_body.clear();
                }
                Err(e) => {
                    handle.state.write().status = e.to_string();
                }
            }
            handle.publish();
        }
    });
}

/// Save compose as IMAP draft.
pub fn compose_save_draft(instance_id: Uuid) {
    let Some(h) = MAIL_LIVE.get(&instance_id) else {
        return;
    };
    let st = h.state.read().clone();
    let account_id = st
        .selected_account
        .or_else(|| h.engine.accounts().first().map(|a| a.id));
    let Some(account_id) = account_id else {
        return;
    };
    let compose = ComposeMessage {
        account_id: Some(account_id),
        to: st.compose_to,
        cc: st.compose_cc,
        bcc: String::new(),
        subject: st.compose_subject,
        body: st.compose_body,
        attachments: Vec::new(),
        in_reply_to: None,
        references: None,
    };
    let handle = Arc::clone(h.value());
    let jobs = Arc::clone(&handle.jobs);
    let handle = Arc::clone(&handle);
    jobs.spawn_coalesced(format!("mail-draft:{instance_id}"), move || {
        let handle = Arc::clone(&handle);
        let compose = compose.clone();
        async move {
            match handle.engine.save_draft(&compose).await {
                Ok(()) => {
                    handle.state.write().status = "draft-saved".into();
                    handle.state.write().mode = 0;
                }
                Err(e) => {
                    handle.state.write().status = e.to_string();
                }
            }
            handle.publish();
        }
    });
}

fn tls_label(mode: TlsMode) -> String {
    match mode {
        TlsMode::Implicit => "implicit".into(),
        TlsMode::StartTls => "starttls".into(),
        TlsMode::None => "none".into(),
    }
}

fn parse_tls(raw: &str) -> TlsMode {
    match raw {
        "starttls" => TlsMode::StartTls,
        "none" => TlsMode::None,
        _ => TlsMode::Implicit,
    }
}

fn email_local(email: &str) -> String {
    email.split('@').next().unwrap_or(email).to_string()
}

struct MailWidget {
    instance_id: Uuid,
    handle: Arc<MailHandle>,
}

impl MailWidget {
    fn new(ctx: &WidgetContext, engine: Arc<MailEngine>, persisted: MailPersisted) -> Self {
        let mut ui = UiState::default();
        ui.selected_account = persisted.selected_account;
        if !persisted.selected_folder.is_empty() {
            ui.selected_folder = persisted.selected_folder;
        }
        ui.selected_uid = persisted.selected_uid;
        engine.set_allow_remote_images(persisted.allow_remote_images);
        let handle = Arc::new(MailHandle {
            instance_id: ctx.instance_id,
            engine,
            bus: ctx.bus.clone(),
            jobs: ctx.jobs.clone(),
            state: RwLock::new(ui),
        });
        MAIL_LIVE.insert(ctx.instance_id, Arc::clone(&handle));
        Self {
            instance_id: ctx.instance_id,
            handle,
        }
    }
}

#[async_trait]
impl Widget for MailWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.schedule_sync();
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.resume_job();
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.pause_job();
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.pause_job();
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.cancel_job();
        MAIL_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: "Mail".into(),
            status: if self.handle.state.read().syncing {
                WidgetStatus::Loading
            } else {
                WidgetStatus::Ready
            },
            payload: WidgetPayload::Mail(self.handle.payload()),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let st = self.handle.state.read();
        let persisted = MailPersisted {
            selected_account: st.selected_account,
            selected_folder: st.selected_folder.clone(),
            selected_uid: st.selected_uid,
            allow_remote_images: self.handle.engine.allow_remote_images(),
        };
        Ok(state_codec::save_state(&persisted)?)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let persisted: MailPersisted = state_codec::restore_state(bytes)?;
        let mut st = self.handle.state.write();
        st.selected_account = persisted.selected_account;
        if !persisted.selected_folder.is_empty() {
            st.selected_folder = persisted.selected_folder;
        }
        st.selected_uid = persisted.selected_uid;
        self.handle
            .engine
            .set_allow_remote_images(persisted.allow_remote_images);
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: false,
        }
    }
}

/// Descriptor; requires a shared [`MailEngine`].
#[must_use]
pub fn descriptor(engine: Arc<MailEngine>) -> WidgetDescriptor {
    *MAIL_ENGINE.write() = Some(Arc::clone(&engine));
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, state_bytes| {
        let persisted = match state_bytes {
            Some(bytes) => state_codec::restore_state(bytes).unwrap_or_default(),
            None => MailPersisted::default(),
        };
        Ok(Box::new(MailWidget::new(&ctx, Arc::clone(&engine), persisted)) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-mail-name",
        description_key: "widget-mail-desc",
        icon_name: "mail",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

fn header_matches(header: &orchid_mail::MessageHeader, query: &str) -> bool {
    header.from.to_lowercase().contains(query)
        || header.subject.to_lowercase().contains(query)
        || header.snippet.to_lowercase().contains(query)
}
