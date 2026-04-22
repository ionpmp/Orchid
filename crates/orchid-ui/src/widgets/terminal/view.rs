//! [`WidgetView`] binding for the terminal widget.
//!
//! The dispatcher's default implementation already produces the right
//! [`SlintPayload`] for [`WidgetPayload::Terminal`]; we keep a dedicated
//! type so specific behaviours (e.g. downsampling preview cells) can be
//! grafted on later without disturbing the public surface.

use orchid_widgets::WidgetSnapshot;

use crate::widgets::view::{SlintPayload, WidgetView};

use super::widget::TERMINAL_TYPE_ID;

/// Terminal widget view. Thin wrapper over the default `WidgetView`
/// behaviour.
#[derive(Debug, Default)]
pub struct TerminalWidgetView;

impl TerminalWidgetView {
    /// Convenience constructor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl WidgetView for TerminalWidgetView {
    fn type_id(&self) -> &'static str {
        TERMINAL_TYPE_ID
    }

    fn render(&self, snapshot: &WidgetSnapshot) -> SlintPayload {
        SlintPayload::from_widget(&snapshot.payload)
    }
}
