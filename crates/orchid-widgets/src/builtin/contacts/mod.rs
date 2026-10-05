//! Local contact cards plus one CardDAV collection.
//!
//! Basic authentication only. A card stores a name, one email, one phone
//! number, and a note. Photos, groups, and extra addresses are ignored.
//! Sync replaces linked cards with the server copy. A card that has not been
//! uploaded stays on this computer.

pub mod carddav;
pub mod config;

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{ContactRow, ContactsPayload};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{merge_remote, Contact, ContactsConfig};

/// Stable type id.
pub const TYPE_ID: &str = "contacts";

static CONTACTS_LIVE: LazyLock<DashMap<Uuid, Arc<ContactsHandle>>> = LazyLock::new(DashMap::new);

struct ContactsHandle {
    instance_id: Uuid,
    config: Arc<RwLock<ContactsConfig>>,
    ui: RwLock<UiState>,
    bus: Arc<orchid_core::EventBus>,
}

#[derive(Debug, Clone, Default)]
struct UiState {
    selected_id: Option<String>,
    url: String,
    user: String,
    password: String,
    status: String,
}

impl ContactsHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn payload(&self) -> ContactsPayload {
        let cfg = self.config.read();
        let ui = self.ui.read();
        let rows = cfg
            .contacts
            .iter()
            .map(|card| ContactRow {
                id: card.id.clone(),
                label: display_name(card),
                selected: ui.selected_id.as_deref() == Some(card.id.as_str()),
            })
            .collect();
        let selected = cfg
            .contacts
            .iter()
            .find(|card| ui.selected_id.as_deref() == Some(card.id.as_str()));
        ContactsPayload {
            rows,
            name: selected.map(|card| card.name.clone()).unwrap_or_default(),
            email: selected.map(|card| card.email.clone()).unwrap_or_default(),
            phone: selected.map(|card| card.phone.clone()).unwrap_or_default(),
            notes: selected.map(|card| card.notes.clone()).unwrap_or_default(),
            has_selection: selected.is_some(),
            account_url: ui.url.clone(),
            account_user: ui.user.clone(),
            status: ui.status.clone(),
        }
    }
}

fn display_name(card: &Contact) -> String {
    if !card.name.trim().is_empty() {
        card.name.trim().to_string()
    } else if !card.email.trim().is_empty() {
        card.email.trim().to_string()
    } else {
        card.phone.trim().to_string()
    }
}

/// Add a blank card and select it.
pub fn new_contact(instance_id: Uuid) {
    let Some(h) = CONTACTS_LIVE.get(&instance_id) else {
        return;
    };
    let card = Contact::blank();
    h.ui.write().selected_id = Some(card.id.clone());
    h.config.write().contacts.push(card);
    h.publish();
}

/// Select a card by id.
pub fn select_contact(instance_id: Uuid, id: &str) {
    let Some(h) = CONTACTS_LIVE.get(&instance_id) else {
        return;
    };
    let exists = h.config.read().contacts.iter().any(|card| card.id == id);
    if exists {
        h.ui.write().selected_id = Some(id.to_string());
        h.publish();
    }
}

/// Edit one field of the selected card. `account_url` and `account_user` edit the form.
pub fn set_field(instance_id: Uuid, field: &str, value: String) {
    let Some(h) = CONTACTS_LIVE.get(&instance_id) else {
        return;
    };
    match field {
        "account_url" => h.ui.write().url = value,
        "account_user" => h.ui.write().user = value,
        "account_password" => h.ui.write().password = value,
        "name" | "email" | "phone" | "notes" => {
            let Some(id) = h.ui.read().selected_id.clone() else {
                return;
            };
            let mut cfg = h.config.write();
            let Some(card) = cfg.contacts.iter_mut().find(|card| card.id == id) else {
                return;
            };
            match field {
                "name" => card.name = value,
                "email" => card.email = value,
                "phone" => card.phone = value,
                "notes" => card.notes = value,
                _ => {}
            }
        }
        _ => return,
    }
    h.publish();
}

/// Store the collection URL and user. An empty password keeps the previous secret.
pub fn save_account(instance_id: Uuid) {
    let Some(h) = CONTACTS_LIVE.get(&instance_id) else {
        return;
    };
    let ui = h.ui.read().clone();
    let url = ui.url.trim().to_string();
    if !url.is_empty() {
        if let Err(token) = carddav::normalize_collection(&url) {
            h.ui.write().status = token.to_string();
            h.publish();
            return;
        }
    }
    {
        let mut cfg = h.config.write();
        cfg.carddav_url = url;
        cfg.carddav_user = ui.user.trim().to_string();
        if !ui.password.is_empty() {
            match orchid_crypto::protect_for_storage(&ui.password) {
                Ok(stored) => cfg.carddav_password = stored,
                Err(_) => {
                    drop(cfg);
                    h.ui.write().status = "secret".into();
                    h.publish();
                    return;
                }
            }
        }
    }
    h.ui.write().password.clear();
    h.ui.write().status = "account-saved".into();
    h.publish();
}

/// Upload the selected card when an account is saved. Otherwise it stays local.
pub fn save_selected(instance_id: Uuid) {
    let Some(h) = CONTACTS_LIVE.get(&instance_id) else {
        return;
    };
    let Some(id) = h.ui.read().selected_id.clone() else {
        return;
    };
    let account = {
        let cfg = h.config.read();
        let Some(card) = cfg.contacts.iter().find(|card| card.id == id).cloned() else {
            return;
        };
        if cfg.carddav_url.trim().is_empty() {
            h.ui.write().status = "saved-local".into();
            h.publish();
            return;
        }
        let password = match orchid_crypto::resolve_stored_secret(&cfg.carddav_password) {
            Ok(password) => password,
            Err(_) => {
                h.ui.write().status = "secret".into();
                h.publish();
                return;
            }
        };
        (
            cfg.carddav_url.clone(),
            cfg.carddav_user.clone(),
            password,
            card,
        )
    };
    let handle = Arc::clone(&h);
    spawn_net(async move {
        let (url, user, password, card) = account;
        match carddav::put_card(&url, &user, &password, &card).await {
            Ok((href, etag)) => {
                let mut cfg = handle.config.write();
                if let Some(stored) = cfg.contacts.iter_mut().find(|item| item.id == card.id) {
                    stored.href = href;
                    stored.etag = etag;
                }
                handle.ui.write().status = "saved-remote".into();
            }
            Err(err) => handle.ui.write().status = err,
        }
        handle.publish();
    });
}

/// Delete the selected card. A linked card is also removed from the collection.
pub fn delete_selected(instance_id: Uuid) {
    let Some(h) = CONTACTS_LIVE.get(&instance_id) else {
        return;
    };
    let Some(id) = h.ui.read().selected_id.clone() else {
        return;
    };
    let remote = {
        let cfg = h.config.read();
        let Some(card) = cfg.contacts.iter().find(|card| card.id == id) else {
            return;
        };
        if card.href.is_empty() {
            None
        } else {
            let password = match orchid_crypto::resolve_stored_secret(&cfg.carddav_password) {
                Ok(password) => password,
                Err(_) => {
                    h.ui.write().status = "secret".into();
                    h.publish();
                    return;
                }
            };
            Some((
                cfg.carddav_user.clone(),
                password,
                card.href.clone(),
                card.etag.clone(),
            ))
        }
    };
    if remote.is_none() {
        remove_local(&h, &id);
        return;
    }
    let handle = Arc::clone(&h);
    spawn_net(async move {
        let (user, password, href, etag) = remote.unwrap();
        match carddav::delete_card(&user, &password, &href, Some(&etag)).await {
            Ok(()) => remove_local(&handle, &id),
            Err(err) => {
                handle.ui.write().status = err;
                handle.publish();
            }
        }
    });
}

fn remove_local(handle: &ContactsHandle, id: &str) {
    handle.config.write().contacts.retain(|card| card.id != id);
    let mut ui = handle.ui.write();
    if ui.selected_id.as_deref() == Some(id) {
        ui.selected_id = None;
    }
    ui.status = "deleted".into();
    drop(ui);
    handle.publish();
}

/// Download the collection and replace linked cards.
pub fn sync(instance_id: Uuid) {
    let Some(h) = CONTACTS_LIVE.get(&instance_id) else {
        return;
    };
    let account = {
        let cfg = h.config.read();
        if cfg.carddav_url.trim().is_empty() || cfg.carddav_user.trim().is_empty() {
            h.ui.write().status = "need-account".into();
            h.publish();
            return;
        }
        let password = match orchid_crypto::resolve_stored_secret(&cfg.carddav_password) {
            Ok(password) => password,
            Err(_) => {
                h.ui.write().status = "secret".into();
                h.publish();
                return;
            }
        };
        (cfg.carddav_url.clone(), cfg.carddav_user.clone(), password)
    };
    let handle = Arc::clone(&h);
    spawn_net(async move {
        handle.ui.write().status = "syncing".into();
        handle.publish();
        let (url, user, password) = account;
        match carddav::pull(&url, &user, &password).await {
            Ok(pull) => {
                let mut cfg = handle.config.write();
                merge_remote(&mut cfg.contacts, pull.cards);
                handle.ui.write().status = if pull.truncated {
                    "truncated".into()
                } else {
                    "synced".into()
                };
            }
            Err(err) => handle.ui.write().status = err,
        }
        handle.publish();
    });
}

fn spawn_net(task: impl std::future::Future<Output = ()> + Send + 'static) {
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::spawn(task);
    }
}

struct ContactsWidget {
    instance_id: Uuid,
    handle: Arc<ContactsHandle>,
}

impl ContactsWidget {
    fn new(ctx: &WidgetContext, config: ContactsConfig) -> Self {
        let ui = UiState {
            url: config.carddav_url.clone(),
            user: config.carddav_user.clone(),
            ..UiState::default()
        };
        let handle = Arc::new(ContactsHandle {
            instance_id: ctx.instance_id,
            config: Arc::new(RwLock::new(config)),
            ui: RwLock::new(ui),
            bus: ctx.bus.clone(),
        });
        CONTACTS_LIVE.insert(ctx.instance_id, Arc::clone(&handle));
        Self {
            instance_id: ctx.instance_id,
            handle,
        }
    }
}

#[async_trait]
impl Widget for ContactsWidget {
    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        CONTACTS_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: String::new(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Contacts(self.handle.payload()),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        if let Ok(cfg) = state_codec::restore_state::<ContactsConfig>(bytes) {
            let mut ui = self.handle.ui.write();
            ui.url = cfg.carddav_url.clone();
            ui.user = cfg.carddav_user.clone();
            drop(ui);
            *self.handle.config.write() = cfg;
        }
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

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let config = match state_bytes {
            Some(bytes) => state_codec::restore_state(bytes).unwrap_or_default(),
            None => ContactsConfig::default(),
        };
        Ok(Box::new(ContactsWidget::new(&ctx, config)) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-contacts-name",
        description_key: "widget-contacts-desc",
        icon_name: "contacts",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_a_local_card_and_replaces_a_linked_one() {
        let mut local = vec![
            Contact {
                id: "local".into(),
                name: "Local".into(),
                href: String::new(),
                ..Contact::blank()
            },
            Contact {
                id: "remote".into(),
                name: "Old".into(),
                href: "https://card.example/book/remote.vcf".into(),
                ..Contact::blank()
            },
        ];
        let local_id = local[0].id.clone();
        merge_remote(
            &mut local,
            vec![Contact {
                id: "remote".into(),
                name: "New".into(),
                href: "https://card.example/book/remote.vcf".into(),
                etag: "\"e\"".into(),
                ..Contact::blank()
            }],
        );
        assert_eq!(local.len(), 2);
        assert!(local
            .iter()
            .any(|card| card.id == local_id && card.name == "Local"));
        assert!(local
            .iter()
            .any(|card| card.id == "remote" && card.name == "New"));
    }
}
