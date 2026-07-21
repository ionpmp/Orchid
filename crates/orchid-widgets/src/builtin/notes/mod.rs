//! Notes / scratchpad — tabbed auto-saving notepad.

pub mod config;

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{NotesPayload, NotesTabRow};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{NoteEntry, NotesConfig};

/// Stable type id.
pub const TYPE_ID: &str = "notes";

static NOTES_LIVE: LazyLock<DashMap<Uuid, Arc<NotesHandle>>> = LazyLock::new(DashMap::new);

struct NotesHandle {
    instance_id: Uuid,
    config: Arc<RwLock<NotesConfig>>,
    bus: Arc<orchid_core::EventBus>,
    find_gen: RwLock<i32>,
    find_cursor: RwLock<i32>,
    find_anchor: RwLock<i32>,
}

impl NotesHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }
}

/// Snapshot the live config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<NotesConfig> {
    NOTES_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut NotesConfig)) {
    let Some(h) = NOTES_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    h.publish();
}

/// Replace the active note body and publish a snapshot patch.
///
/// The Slint surface keeps a local draft so the caret survives the patch.
pub fn set_body(instance_id: Uuid, body: String) {
    let Some(h) = NOTES_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        let note = cfg.active_note_mut();
        note.body = body;
        if note.title.trim().is_empty() {
            note.title = title_from_body(&note.body);
        }
    }
    h.publish();
}

/// Rename the active note tab.
pub fn set_title(instance_id: Uuid, title: String) {
    let Some(h) = NOTES_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        cfg.active_note_mut().title = title;
    }
    h.publish();
}

/// Switch the active tab.
pub fn select_tab(instance_id: Uuid, index: i32) {
    if index < 0 {
        return;
    }
    let Some(h) = NOTES_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        if (index as usize) < cfg.notes.len() {
            cfg.active_index = index as u32;
        }
    }
    h.publish();
}

/// Create a new blank tab and focus it.
pub fn new_tab(instance_id: Uuid) {
    let Some(h) = NOTES_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        cfg.notes.push(NoteEntry::blank());
        cfg.active_index = (cfg.notes.len() - 1) as u32;
    }
    h.publish();
}

/// Close a tab by index. Keeps at least one tab.
pub fn close_tab(instance_id: Uuid, index: i32) {
    if index < 0 {
        return;
    }
    let Some(h) = NOTES_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        let idx = index as usize;
        if cfg.notes.len() <= 1 || idx >= cfg.notes.len() {
            // Clear the sole note instead of removing it.
            if cfg.notes.len() == 1 {
                cfg.notes[0] = NoteEntry::blank();
                cfg.active_index = 0;
            }
        } else {
            cfg.notes.remove(idx);
            if cfg.active_index as usize >= cfg.notes.len() {
                cfg.active_index = (cfg.notes.len() - 1) as u32;
            } else if (cfg.active_index as usize) > idx {
                cfg.active_index = cfg.active_index.saturating_sub(1);
            }
        }
        cfg.normalize();
    }
    h.publish();
}

/// Toggle word wrap.
pub fn toggle_wrap(instance_id: Uuid) {
    update_config(instance_id, |cfg| {
        cfg.word_wrap = !cfg.word_wrap;
    });
}

/// Toggle monospace font.
pub fn toggle_mono(instance_id: Uuid) {
    update_config(instance_id, |cfg| {
        cfg.mono_font = !cfg.mono_font;
    });
}

/// Adjust font size by a delta (− / +).
pub fn zoom(instance_id: Uuid, delta: i32) {
    update_config(instance_id, |cfg| {
        let next = i32::from(cfg.font_size).saturating_add(delta);
        cfg.font_size = NotesConfig::clamp_font_size(next.clamp(0, 255) as u8);
    });
}

/// Clear the active note body.
pub fn clear_active(instance_id: Uuid) {
    let Some(h) = NOTES_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        let note = cfg.active_note_mut();
        note.body.clear();
        note.title.clear();
    }
    h.publish();
}

/// Find the next / previous case-insensitive match in the active body.
pub fn find(instance_id: Uuid, query: &str, forward: bool) {
    let q = query.trim();
    if q.is_empty() {
        return;
    }
    let Some(h) = NOTES_LIVE.get(&instance_id) else {
        return;
    };
    let body = h.config.read().active_note().body.clone();
    let body_lower = body.to_lowercase();
    let q_lower = q.to_lowercase();
    // Slint TextInput::set-selection-offsets uses UTF-8 byte offsets.
    let q_bytes = q_lower.len() as i32;
    if q_bytes <= 0 {
        return;
    }

    let current = (*h.find_cursor.read()).max(0);
    let found_byte = if forward {
        let start = (current as usize).min(body_lower.len());
        body_lower[start..]
            .find(&q_lower)
            .map(|rel| start + rel)
            .or_else(|| body_lower.find(&q_lower))
    } else {
        let end = (current as usize).min(body_lower.len());
        body_lower[..end]
            .rfind(&q_lower)
            .or_else(|| body_lower.rfind(&q_lower))
    };

    let Some(byte_start) = found_byte else {
        return;
    };
    *h.find_anchor.write() = byte_start as i32;
    *h.find_cursor.write() = byte_start as i32 + q_bytes;
    *h.find_gen.write() += 1;
    h.publish();
}

/// Text stats for status-bar display.
#[must_use]
pub fn text_stats(body: &str) -> (i32, i32, i32) {
    let chars = body.chars().count() as i32;
    let words = body
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .count() as i32;
    let lines = if body.is_empty() {
        1
    } else {
        body.lines().count().max(1) as i32
    };
    (chars, words, lines)
}

/// Derive a short tab title from the first non-empty line of the body.
#[must_use]
pub fn title_from_body(body: &str) -> String {
    let line = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.is_empty() {
        return String::new();
    }
    const MAX: usize = 28;
    let mut out = String::new();
    for (i, ch) in line.chars().enumerate() {
        if i >= MAX {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Notes widget implementation.
pub struct NotesWidget {
    instance_id: Uuid,
    handle: Arc<NotesHandle>,
}

impl std::fmt::Debug for NotesWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotesWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl NotesWidget {
    /// Construct with config.
    pub fn new(
        instance_id: Uuid,
        mut config: NotesConfig,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        config.normalize();
        let handle = Arc::new(NotesHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            bus,
            find_gen: RwLock::new(0),
            find_cursor: RwLock::new(0),
            find_anchor: RwLock::new(0),
        });
        NOTES_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }

    fn build_payload(&self) -> NotesPayload {
        let cfg = self.handle.config.read();
        let active = cfg.active_note();
        let (char_count, word_count, line_count) = text_stats(&active.body);
        let tabs = cfg
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let title = if n.title.trim().is_empty() {
                    if n.body.trim().is_empty() {
                        String::new()
                    } else {
                        title_from_body(&n.body)
                    }
                } else {
                    n.title.clone()
                };
                NotesTabRow {
                    id: n.id.clone(),
                    title,
                    is_active: i == cfg.active_index as usize,
                }
            })
            .collect();
        NotesPayload {
            tabs,
            active_index: cfg.active_index as i32,
            title: active.title.clone(),
            body: active.body.clone(),
            font_size: i32::from(cfg.font_size),
            word_wrap: cfg.word_wrap,
            mono_font: cfg.mono_font,
            show_status_bar: cfg.show_status_bar,
            char_count,
            word_count,
            line_count,
            find_gen: *self.handle.find_gen.read(),
            find_cursor: *self.handle.find_cursor.read(),
            find_anchor: *self.handle.find_anchor.read(),
        }
    }
}

#[async_trait]
impl Widget for NotesWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
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
        NOTES_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let payload = self.build_payload();
        let title = {
            let cfg = self.handle.config.read();
            let active = cfg.active_note();
            if !active.title.trim().is_empty() {
                active.title.clone()
            } else {
                let derived = title_from_body(&active.body);
                if derived.is_empty() {
                    String::new()
                } else {
                    derived
                }
            }
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title,
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Notes(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: NotesConfig = state_codec::restore_state(bytes).unwrap_or_default();
        cfg.normalize();
        *self.handle.config.write() = cfg;
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Small),
            max_size: None,
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let mut cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<NotesConfig>(bytes).unwrap_or_default(),
            None => NotesConfig::default(),
        };
        cfg.normalize();
        Ok(Box::new(NotesWidget::new(
            ctx.instance_id,
            cfg,
            ctx.bus.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-notes-name",
        description_key: "widget-notes-desc",
        icon_name: "notes",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Small),
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
    fn title_from_body_uses_first_line() {
        assert_eq!(title_from_body("Hello\nWorld"), "Hello");
        assert_eq!(title_from_body("   \n  Draft  "), "Draft");
        assert!(title_from_body("").is_empty());
    }

    #[test]
    fn text_stats_counts() {
        let (c, w, l) = text_stats("one two\nthree");
        assert_eq!(c, 13);
        assert_eq!(w, 3);
        assert_eq!(l, 2);
        let (c0, w0, l0) = text_stats("");
        assert_eq!((c0, w0, l0), (0, 0, 1));
    }
}
