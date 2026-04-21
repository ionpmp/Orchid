//! Fuzzy-search entry point for the command palette.

use std::sync::Arc;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use parking_lot::Mutex;

use crate::command::descriptor::CommandDescriptor;
use crate::command::registry::CommandRegistry;

/// One hit returned by [`CommandPalette::search`].
#[derive(Debug, Clone)]
pub struct PaletteResult {
    /// Matched descriptor.
    pub descriptor: CommandDescriptor,
    /// Fuzzy score (higher is better).
    pub score: i32,
    /// Character positions in `descriptor.display_name_key` (for a later
    /// highlight pass in the UI).
    pub match_positions: Vec<u32>,
}

/// Search engine for the command palette.
///
/// This type owns only the search logic; the UI that wraps it lives in
/// `orchid-ui`.
pub struct CommandPalette {
    registry: Arc<CommandRegistry>,
    matcher: Mutex<Matcher>,
}

impl std::fmt::Debug for CommandPalette {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandPalette").finish_non_exhaustive()
    }
}

impl CommandPalette {
    /// Build a palette over an existing registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use orchid_core::{CommandPalette, CommandRegistry};
    /// let palette = CommandPalette::new(Arc::new(CommandRegistry::new()));
    /// assert!(palette.browse().is_empty());
    /// ```
    #[must_use]
    pub fn new(registry: Arc<CommandRegistry>) -> Self {
        Self {
            registry,
            matcher: Mutex::new(Matcher::new(Config::DEFAULT)),
        }
    }

    /// Ranked fuzzy search across display name, id, and verb.
    ///
    /// Returns up to `limit` results, sorted by score descending. Ties are
    /// broken by the id.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<PaletteResult> {
        let all = self.registry.list_all();
        if query.trim().is_empty() || limit == 0 {
            return Vec::new();
        }

        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut matcher = self.matcher.lock();

        let mut hits: Vec<PaletteResult> = Vec::new();
        let mut haystack_buf = Vec::new();
        for desc in all {
            // Score against the three searchable strings; keep the best one.
            let candidates = [
                desc.display_name_key.clone(),
                desc.id.clone(),
                desc.terminal_invocation.as_ref().map_or(String::new(), |t| t.verb.clone()),
            ];

            let mut best_score: Option<u32> = None;
            let mut best_positions: Vec<u32> = Vec::new();
            let mut best_on_display = false;

            for (idx, hay) in candidates.iter().enumerate() {
                if hay.is_empty() {
                    continue;
                }
                haystack_buf.clear();
                let h = Utf32Str::new(hay, &mut haystack_buf);
                let mut positions = Vec::new();
                if let Some(score) = pattern.indices(h, &mut matcher, &mut positions) {
                    if best_score.is_none_or(|b| score > b) {
                        best_score = Some(score);
                        best_positions = positions;
                        best_on_display = idx == 0;
                    }
                }
            }

            if let Some(score) = best_score {
                hits.push(PaletteResult {
                    descriptor: desc,
                    score: score as i32,
                    match_positions: if best_on_display {
                        best_positions
                    } else {
                        // Positions only make sense against the display name
                        // for highlighting; for id / verb matches leave them
                        // empty so the UI doesn't highlight the wrong string.
                        Vec::new()
                    },
                });
            }
        }

        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.descriptor.id.cmp(&b.descriptor.id))
        });
        hits.truncate(limit);
        hits
    }

    /// Return every command, sorted by category then display name.
    #[must_use]
    pub fn browse(&self) -> Vec<CommandDescriptor> {
        let mut all = self.registry.list_all();
        all.sort_by(|a, b| {
            format!("{:?}", a.category)
                .cmp(&format!("{:?}", b.category))
                .then_with(|| a.display_name_key.cmp(&b.display_name_key))
        });
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, ActionContext, ActionOutcome};
    use crate::command::descriptor::{CommandCategory, CommandDescriptor, TerminalInvocation};
    use crate::command::registry::ActionFactory;
    use crate::error::Result;
    use async_trait::async_trait;

    struct Noop;
    #[async_trait]
    impl Action for Noop {
        fn id(&self) -> &'static str {
            "p.noop"
        }
        fn display_name_key(&self) -> &'static str {
            "p.noop.name"
        }
        fn command_text(&self) -> String {
            "orc p noop".into()
        }
        async fn execute(&self, _: &ActionContext) -> Result<ActionOutcome> {
            Ok(ActionOutcome::ok())
        }
    }

    fn make_desc(id: &str, display: &str, verb: Option<&str>) -> CommandDescriptor {
        CommandDescriptor {
            id: id.into(),
            display_name_key: display.into(),
            description_key: None,
            category: CommandCategory::Developer,
            default_shortcut: None,
            terminal_invocation: verb.map(|v| TerminalInvocation {
                verb: v.into(),
                args: Vec::new(),
            }),
            icon_name: None,
        }
    }

    fn factory() -> ActionFactory {
        Arc::new(|_| Ok(Box::new(Noop) as Box<dyn Action>))
    }

    #[test]
    fn ranked_results_put_exact_match_first() {
        let reg = Arc::new(CommandRegistry::new());
        reg.register(make_desc("fs.move", "Move File", Some("fs move")), factory())
            .unwrap();
        reg.register(make_desc("fs.copy", "Copy File", Some("fs copy")), factory())
            .unwrap();
        reg.register(
            make_desc("widget.create", "Create Widget", Some("widget create")),
            factory(),
        )
        .unwrap();

        let palette = CommandPalette::new(reg);
        let hits = palette.search("move file", 10);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].descriptor.id, "fs.move");
    }

    #[test]
    fn empty_query_returns_no_hits() {
        let reg = Arc::new(CommandRegistry::new());
        reg.register(make_desc("a", "Alpha", None), factory()).unwrap();
        let palette = CommandPalette::new(reg);
        assert!(palette.search("", 5).is_empty());
    }

    #[test]
    fn browse_returns_all_sorted() {
        let reg = Arc::new(CommandRegistry::new());
        reg.register(make_desc("a", "Alpha", None), factory()).unwrap();
        reg.register(make_desc("b", "Beta", None), factory()).unwrap();
        let palette = CommandPalette::new(reg);
        let browse = palette.browse();
        assert_eq!(browse.len(), 2);
    }
}
