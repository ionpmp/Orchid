//! Search source backed by [`orchid_core::CommandPalette`].

use std::sync::Arc;

use async_trait::async_trait;

use super::{ActionTarget, SearchCandidate, SearchSource};

/// Wraps a shared [`orchid_core::CommandPalette`].
pub struct CommandsSource {
    palette: Arc<orchid_core::CommandPalette>,
}

impl std::fmt::Debug for CommandsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandsSource").finish_non_exhaustive()
    }
}

impl CommandsSource {
    /// Build over an existing palette.
    #[must_use]
    pub fn new(palette: Arc<orchid_core::CommandPalette>) -> Self {
        Self { palette }
    }
}

#[async_trait]
impl SearchSource for CommandsSource {
    fn id(&self) -> &'static str {
        "commands"
    }
    fn name_key(&self) -> &'static str {
        "search-source-commands"
    }
    fn icon(&self) -> &'static str {
        "search-commands"
    }
    async fn search(&self, query: &str, limit: usize) -> Vec<SearchCandidate> {
        let hits = self.palette.search(query, limit);
        hits.into_iter()
            .map(|h| SearchCandidate {
                id: format!("cmd:{}", h.descriptor.id),
                source_id: "commands",
                title: h.descriptor.display_name_key.clone(),
                subtitle: h
                    .descriptor
                    .terminal_invocation
                    .as_ref()
                    .map(|t| format!("orc {}", t.verb)),
                icon: "search-commands",
                score: h.score,
                action_hint: h
                    .descriptor
                    .default_shortcut
                    .as_ref()
                    .map(|s| s.to_string()),
                action_target: ActionTarget::RunCommand(h.descriptor.id.clone()),
            })
            .collect()
    }
}
