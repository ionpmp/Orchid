//! Renderer-agnostic bridge between [`orchid_widgets::WidgetSnapshot`] and
//! the UI layer.
//!
//! Every built-in widget registers a [`WidgetView`] with the [`WidgetViewDispatcher`].
//! The Slint shell (added in a follow-up task) calls
//! [`WidgetViewDispatcher::render`] once per instance per frame to obtain the
//! concrete payload it should push into the corresponding Slint component.
//!
//! This module intentionally stops short of touching Slint types — the
//! dispatcher speaks only to [`SlintPayload`], a Slint-shaped mirror of
//! [`orchid_widgets::WidgetPayload`]. The final adapter that turns
//! [`SlintPayload::Terminal`] cells into a `slint::Model` lives alongside the
//! Slint component once the UI shell lands.

use std::collections::HashMap;

use orchid_widgets::{WidgetPayload, WidgetSnapshot};
use parking_lot::RwLock;

/// Rust-side mirror of the Slint struct emitted into the workspace model.
#[derive(Debug, Clone)]
pub struct SlintTerminalCell {
    /// Visible character.
    pub ch: char,
    /// Foreground RGBA.
    pub fg_rgba: [u8; 4],
    /// Background RGBA.
    pub bg_rgba: [u8; 4],
    /// Bold flag.
    pub bold: bool,
    /// Italic flag.
    pub italic: bool,
    /// Underline flag.
    pub underline: bool,
}

/// Renderer-facing payload. Exactly mirrors [`WidgetPayload`] but uses
/// owned `String`s and [`SlintTerminalCell`] so it is easy to adapt into
/// Slint models at the final step.
#[derive(Debug, Clone)]
pub enum SlintPayload {
    /// Nothing to render yet.
    Empty,
    /// A vertical list of text rows.
    Text(Vec<String>),
    /// Generic key / value rows.
    KeyValueList(Vec<(String, String)>),
    /// Terminal cells + cursor.
    Terminal {
        /// Columns.
        cols: i32,
        /// Rows.
        rows: i32,
        /// Cells in row-major order.
        cells: Vec<SlintTerminalCell>,
        /// Cursor column.
        cursor_col: i32,
        /// Cursor row.
        cursor_row: i32,
        /// Whether the cursor should be drawn.
        cursor_visible: bool,
    },
}

impl SlintPayload {
    /// Convert a framework payload into the renderer-friendly shape.
    #[must_use]
    pub fn from_widget(payload: &WidgetPayload) -> Self {
        match payload {
            WidgetPayload::Empty => Self::Empty,
            WidgetPayload::Text { lines } => Self::Text(lines.clone()),
            WidgetPayload::KeyValueList { entries } => {
                Self::KeyValueList(entries.clone())
            }
            WidgetPayload::Terminal(payload) => Self::Terminal {
                cols: payload.cols as i32,
                rows: payload.rows as i32,
                cells: payload
                    .cells
                    .iter()
                    .map(|c| SlintTerminalCell {
                        ch: c.ch,
                        fg_rgba: c.fg_rgba,
                        bg_rgba: c.bg_rgba,
                        bold: c.bold,
                        italic: c.italic,
                        underline: c.underline,
                    })
                    .collect(),
                cursor_col: payload.cursor_col as i32,
                cursor_row: payload.cursor_row as i32,
                cursor_visible: payload.cursor_visible,
            },
        }
    }
}

/// Implemented by each widget type so that the dispatcher can produce a
/// [`SlintPayload`] for it.
///
/// Most widget types share the same trivial implementation — the default
/// body just converts the framework payload with [`SlintPayload::from_widget`].
/// Widgets that need to massage their payload at render time (e.g. collapse
/// large terminal grids for previews) override it.
pub trait WidgetView: Send + Sync {
    /// Stable widget type id.
    fn type_id(&self) -> &'static str;

    /// Produce the renderer-facing payload for `snapshot`.
    fn render(&self, snapshot: &WidgetSnapshot) -> SlintPayload {
        SlintPayload::from_widget(&snapshot.payload)
    }
}

/// Directory of per-type [`WidgetView`]s. A single instance is held by the
/// workspace controller.
#[derive(Default)]
pub struct WidgetViewDispatcher {
    views: RwLock<HashMap<&'static str, Box<dyn WidgetView>>>,
}

impl std::fmt::Debug for WidgetViewDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetViewDispatcher")
            .field("registered", &self.views.read().len())
            .finish()
    }
}

impl WidgetViewDispatcher {
    /// New, empty dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a view for `type_id`.
    pub fn register(&self, view: Box<dyn WidgetView>) {
        let type_id = view.type_id();
        self.views.write().insert(type_id, view);
    }

    /// Produce the payload for `snapshot`. Falls back to a generic conversion
    /// when the widget type has no registered view.
    #[must_use]
    pub fn render(&self, snapshot: &WidgetSnapshot) -> SlintPayload {
        let views = self.views.read();
        match views.get(snapshot.widget_type) {
            Some(v) => v.render(snapshot),
            None => SlintPayload::from_widget(&snapshot.payload),
        }
    }

    /// How many views are currently registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.views.read().len()
    }

    /// Whether no views are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.views.read().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_widgets::{WidgetSnapshot, WidgetStatus};

    struct DemoView;
    impl WidgetView for DemoView {
        fn type_id(&self) -> &'static str {
            "demo"
        }
    }

    fn snapshot_of(widget_type: &'static str, payload: WidgetPayload) -> WidgetSnapshot {
        WidgetSnapshot {
            instance_id: uuid::Uuid::nil(),
            widget_type,
            title: "t".into(),
            status: WidgetStatus::Ready,
            payload,
        }
    }

    #[test]
    fn dispatcher_uses_registered_view() {
        let d = WidgetViewDispatcher::new();
        d.register(Box::new(DemoView));
        assert_eq!(d.len(), 1);
        let out = d.render(&snapshot_of(
            "demo",
            WidgetPayload::Text {
                lines: vec!["hello".into()],
            },
        ));
        match out {
            SlintPayload::Text(rows) => assert_eq!(rows, vec!["hello".to_string()]),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn dispatcher_falls_back_when_no_view_registered() {
        let d = WidgetViewDispatcher::new();
        let out = d.render(&snapshot_of(
            "unknown",
            WidgetPayload::KeyValueList {
                entries: vec![("k".into(), "v".into())],
            },
        ));
        match out {
            SlintPayload::KeyValueList(kv) => {
                assert_eq!(kv, vec![("k".to_string(), "v".to_string())])
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
