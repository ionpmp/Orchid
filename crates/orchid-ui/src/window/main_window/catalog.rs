//! Widget Catalog handlers for [`MainWindowController`].

use slint::{ComponentHandle, SharedString};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use orchid_i18n::LocaleManager;
use orchid_storage::WidgetSize;
use orchid_widgets::{CreateWidgetRequest, WidgetManager};

use crate::slint_generated::{DockWidgetType, WidgetCatalog};
use crate::window::spawn;

use super::{AddWidgetPlacement, MainWindowController};

impl MainWindowController {
    pub(super) fn on_catalog_dismiss(self: &Arc<Self>) {
        if !self.catalog.read().visible {
            return;
        }
        {
            let mut cat = self.catalog.write();
            cat.visible = false;
            cat.search_query.clear();
        }
        self.sync_widget_catalog_global();
    }

    pub(super) fn on_catalog_search_changed(self: &Arc<Self>, query: &SharedString) {
        self.catalog.write().search_query = query.to_string();
        self.sync_widget_catalog_items_only();
    }

    pub(super) fn on_catalog_pick(self: &Arc<Self>, type_id: &SharedString) {
        let placement = {
            let cat = self.catalog.read();
            AddWidgetPlacement::CanvasPoint {
                content_x: cat.content_x,
                content_y: cat.content_y,
            }
        };
        self.on_catalog_dismiss();
        self.spawn_add_widget(type_id.as_str(), placement);
    }

    pub(super) fn spawn_add_widget(self: &Arc<Self>, type_id: &str, placement: AddWidgetPlacement) {
        if type_id == "document-editor" {
            self.spawn_open_document_editor(placement);
            return;
        }
        if type_id == "media-viewer" {
            self.spawn_open_media_player(placement);
            return;
        }
        if !is_known_widget_type(type_id) {
            warn!(type_id, "unknown widget type");
            return;
        }
        let le = self.layout_engine.clone();
        let wm = self.widget_manager.clone();
        let wsm = self.workspace_manager.clone();
        let t = Arc::downgrade(self);
        let type_id_owned = type_id.to_string();
        let canonical = orchid_widgets::WidgetRegistry::canonical_type_id(&type_id_owned);
        let focus_search_input = canonical == "universal-search";
        let focus_password_input = canonical == "password-manager";
        spawn::spawn_local(async move {
            let wid = match wsm.active() {
                Ok(w) => w.id,
                Err(_) => return,
            };
            let size = Self::minimal_widget_size(&wm, &type_id_owned);
            let new_id = match wm
                .create(CreateWidgetRequest {
                    type_id: type_id_owned,
                    workspace_id: wid,
                    position: None,
                    size: Some(size),
                    initial_lifecycle: None,
                    config_bytes: None,
                })
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    warn!(?e, "add widget");
                    return;
                }
            };
            match placement {
                AddWidgetPlacement::AutoSlot => {
                    Self::move_new_widget_to_free_slot(&le, &wm, wid, new_id).await;
                }
                AddWidgetPlacement::CanvasPoint {
                    content_x,
                    content_y,
                } => {
                    if let Some(c) = t.upgrade() {
                        c.place_widget_at_canvas_point(wid, new_id, size, content_x, content_y)
                            .await;
                    }
                }
            }
            if let Err(e) = wm.refresh_snapshot_cache(new_id).await {
                warn!(?e, widget_id = %new_id, "prime snapshot cache after add");
            }
            if let Some(c) = t.upgrade() {
                if focus_search_input {
                    *c.search_autofocus_pending.lock() = Some(new_id);
                }
                if focus_password_input {
                    c.password_autofocus_pending.write().insert(new_id, true);
                }
                c.schedule_rebuild();
            }
        });
    }

    pub(super) async fn place_widget_at_canvas_point(
        self: &Arc<Self>,
        workspace_id: Uuid,
        instance_id: Uuid,
        size: WidgetSize,
        content_x: f32,
        content_y: f32,
    ) {
        let (vw, vh) = *self.canvas_size.lock();
        let viewport = ViewportSize {
            width_px: vw,
            height_px: vh,
        };
        let preferred = self
            .layout_engine
            .placement_from_content_top_left(viewport, content_x, content_y, size);
        let instances = self.widget_manager.instances_for_workspace(workspace_id);
        let place = if self
            .layout_engine
            .can_place(workspace_id, instance_id, preferred, size, &instances)
            .is_ok()
        {
            preferred
        } else {
            match self.layout_engine.auto_place_excluding_with_growth(
                workspace_id,
                size,
                &instances,
                instance_id,
            ) {
                Ok(p) => p,
                Err(e) => {
                    warn!(?e, "catalog place: no free cell");
                    return;
                }
            }
        };
        if let Err(e) = self.widget_manager.move_to(instance_id, place).await {
            warn!(?e, "catalog place: move_to");
        }
    }

    pub(super) fn sync_widget_catalog_global(self: &Arc<Self>) {
        let cat = self.catalog.read().clone();
        let items = filter_catalog_items(&self.locale, &cat.search_query);
        let empty = items.is_empty();
        let visible_ids: std::collections::HashSet<&str> =
            items.iter().map(|d| d.type_id.as_str()).collect();
        info!(
            count = items.len(),
            query = %cat.search_query,
            visible = cat.visible,
            "widget catalog sync"
        );
        let g = self.window.global::<WidgetCatalog>();
        apply_catalog_row_visibility(&g, &visible_ids);
        g.set_is_empty(empty);
        g.set_search_query(cat.search_query.clone().into());
        g.set_screen_x(cat.screen_x);
        g.set_screen_y(cat.screen_y);
        g.set_visible(cat.visible);
    }

    /// Update filtered card visibility while typing without resetting the search field.
    pub(super) fn sync_widget_catalog_items_only(self: &Arc<Self>) {
        let cat = self.catalog.read().clone();
        let items = filter_catalog_items(&self.locale, &cat.search_query);
        let empty = items.is_empty();
        let visible_ids: std::collections::HashSet<&str> =
            items.iter().map(|d| d.type_id.as_str()).collect();
        let g = self.window.global::<WidgetCatalog>();
        apply_catalog_row_visibility(&g, &visible_ids);
        g.set_is_empty(empty);
    }

    pub(super) fn minimal_widget_size(wm: &WidgetManager, type_id: &str) -> WidgetSize {
        wm.registry()
            .get(type_id)
            .map(|d| d.min_size.unwrap_or(d.default_size))
            .unwrap_or(WidgetSize::Medium)
    }
}

// Helpers from mod.rs that can be standalone in this module
use orchid_widgets::layout::ViewportSize;

pub(super) fn is_known_widget_type(type_id: &str) -> bool {
    matches!(
        orchid_widgets::WidgetRegistry::canonical_type_id(type_id),
        "terminal"
            | "weather"
            | "moon"
            | "jyotish"
            | "clock"
            | "system"
            | "processes"
            | "calculator"
            | "notes"
            | "calendar"
            | "rss"
            | "recent-files"
            | "universal-search"
            | "media-player"
            | "audio-player"
            | "media-viewer"
            | "video-player"
            | "password-manager"
            | "viewer"
            | "document-editor"
            | "file-manager"
    )
}

fn apply_catalog_row_visibility(g: &WidgetCatalog, visible_ids: &std::collections::HashSet<&str>) {
    g.set_show_terminal(visible_ids.contains("terminal"));
    g.set_show_weather(visible_ids.contains("weather"));
    g.set_show_moon(visible_ids.contains("moon"));
    g.set_show_jyotish(visible_ids.contains("jyotish"));
    g.set_show_clock(visible_ids.contains("clock"));
    g.set_show_system(visible_ids.contains("system"));
    g.set_show_processes(visible_ids.contains("processes"));
    g.set_show_calculator(visible_ids.contains("calculator"));
    g.set_show_notes(visible_ids.contains("notes"));
    g.set_show_calendar(visible_ids.contains("calendar"));
    g.set_show_rss(visible_ids.contains("rss"));
    g.set_show_recent_files(visible_ids.contains("recent-files"));
    g.set_show_search(visible_ids.contains("search"));
    g.set_show_media(visible_ids.contains("media"));
    g.set_show_audio_player(visible_ids.contains("audio-player"));
    g.set_show_media_viewer(visible_ids.contains("media-viewer"));
    g.set_show_video_player(visible_ids.contains("video-player"));
    g.set_show_password(visible_ids.contains("password"));
    g.set_show_viewer(visible_ids.contains("viewer"));
    g.set_show_document_editor(visible_ids.contains("document-editor"));
    g.set_show_file_manager(visible_ids.contains("file-manager"));
}

pub(super) fn filter_catalog_items(locale: &LocaleManager, query: &str) -> Vec<DockWidgetType> {
    let q = query.trim().to_lowercase();
    dock_types_vec(locale)
        .into_iter()
        .filter(|d| {
            q.is_empty()
                || d.label.as_str().to_lowercase().contains(&q)
                || d.description.as_str().to_lowercase().contains(&q)
                || d.type_id.as_str().to_lowercase().contains(&q)
                || d.icon.as_str().to_lowercase().contains(&q)
        })
        .collect()
}

pub(super) fn dock_widget_description(locale: &LocaleManager, type_id: &str) -> SharedString {
    let key = match type_id {
        "terminal" => "widget-terminal-desc",
        "weather" => "widget-weather-desc",
        "moon" => "widget-moon-desc",
        "jyotish" => "widget-jyotish-desc",
        "clock" => "widget-clock-desc",
        "system" => "widget-system-desc",
        "processes" => "widget-processes-desc",
        "calculator" => "widget-calculator-desc",
        "notes" => "widget-notes-desc",
        "calendar" => "widget-calendar-desc",
        "rss" => "widget-rss-desc",
        "recent-files" => "widget-recent-files-desc",
        "search" | "universal-search" => "widget-search-desc",
        "media" => "widget-media-desc",
        "audio-player" => "widget-audio-player-desc",
        "media-viewer" => "widget-media-viewer-desc",
        "video-player" => "widget-video-player-desc",
        "password" => "widget-password-desc",
        "viewer" => "widget-viewer-desc",
        "document-editor" => "widget-document-editor-desc",
        "file-manager" => "widget-fm-desc",
        _ => return SharedString::new(),
    };
    locale.tr(key).into()
}

pub(super) fn dock_types_vec(locale: &LocaleManager) -> Vec<DockWidgetType> {
    vec![
        DockWidgetType {
            type_id: "terminal".into(),
            label: locale.tr("dock-widget-terminal").into(),
            description: dock_widget_description(locale, "terminal"),
            icon: "terminal".into(),
        },
        DockWidgetType {
            type_id: "weather".into(),
            label: locale.tr("dock-widget-weather").into(),
            description: dock_widget_description(locale, "weather"),
            icon: "weather".into(),
        },
        DockWidgetType {
            type_id: "moon".into(),
            label: locale.tr("dock-widget-moon").into(),
            description: dock_widget_description(locale, "moon"),
            icon: "moon".into(),
        },
        DockWidgetType {
            type_id: "jyotish".into(),
            label: locale.tr("dock-widget-jyotish").into(),
            description: dock_widget_description(locale, "jyotish"),
            icon: "jyotish".into(),
        },
        DockWidgetType {
            type_id: "clock".into(),
            label: locale.tr("dock-widget-clock").into(),
            description: dock_widget_description(locale, "clock"),
            icon: "clock".into(),
        },
        DockWidgetType {
            type_id: "system".into(),
            label: locale.tr("dock-widget-system").into(),
            description: dock_widget_description(locale, "system"),
            icon: "system".into(),
        },
        DockWidgetType {
            type_id: "processes".into(),
            label: locale.tr("dock-widget-processes").into(),
            description: dock_widget_description(locale, "processes"),
            icon: "processes".into(),
        },
        DockWidgetType {
            type_id: "calculator".into(),
            label: locale.tr("dock-widget-calculator").into(),
            description: dock_widget_description(locale, "calculator"),
            icon: "calculator".into(),
        },
        DockWidgetType {
            type_id: "notes".into(),
            label: locale.tr("dock-widget-notes").into(),
            description: dock_widget_description(locale, "notes"),
            icon: "notes".into(),
        },
        DockWidgetType {
            type_id: "calendar".into(),
            label: locale.tr("dock-widget-calendar").into(),
            description: dock_widget_description(locale, "calendar"),
            icon: "calendar".into(),
        },
        DockWidgetType {
            type_id: "rss".into(),
            label: locale.tr("dock-widget-rss").into(),
            description: dock_widget_description(locale, "rss"),
            icon: "rss".into(),
        },
        DockWidgetType {
            type_id: "recent-files".into(),
            label: locale.tr("dock-widget-recent-files").into(),
            description: dock_widget_description(locale, "recent-files"),
            icon: "recent-files".into(),
        },
        DockWidgetType {
            type_id: "search".into(),
            label: locale.tr("dock-widget-search").into(),
            description: dock_widget_description(locale, "search"),
            icon: "search".into(),
        },
        DockWidgetType {
            type_id: "media".into(),
            label: locale.tr("dock-widget-media").into(),
            description: dock_widget_description(locale, "media"),
            icon: "media".into(),
        },
        DockWidgetType {
            type_id: "audio-player".into(),
            label: locale.tr("dock-widget-audio-player").into(),
            description: dock_widget_description(locale, "audio-player"),
            icon: "audio-player".into(),
        },
        DockWidgetType {
            type_id: "media-viewer".into(),
            label: locale.tr("dock-widget-media-viewer").into(),
            description: dock_widget_description(locale, "media-viewer"),
            icon: "media".into(),
        },
        DockWidgetType {
            type_id: "video-player".into(),
            label: locale.tr("dock-widget-video-player").into(),
            description: dock_widget_description(locale, "video-player"),
            icon: "video-player".into(),
        },
        DockWidgetType {
            type_id: "password".into(),
            label: locale.tr("dock-widget-password").into(),
            description: dock_widget_description(locale, "password"),
            icon: "password".into(),
        },
        DockWidgetType {
            type_id: "viewer".into(),
            label: locale.tr("dock-widget-viewer").into(),
            description: dock_widget_description(locale, "viewer"),
            icon: "viewer".into(),
        },
        DockWidgetType {
            type_id: "document-editor".into(),
            label: locale.tr("dock-widget-document-editor").into(),
            description: dock_widget_description(locale, "document-editor"),
            icon: "document".into(),
        },
        DockWidgetType {
            type_id: "file-manager".into(),
            label: locale.tr("dock-widget-fm").into(),
            description: dock_widget_description(locale, "file-manager"),
            icon: "fm".into(),
        },
    ]
}
