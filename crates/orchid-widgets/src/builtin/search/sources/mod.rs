//! Search-source trait + shared candidate types.

pub mod calculator;
pub mod commands;
pub mod files;
pub mod settings;

use async_trait::async_trait;

pub use calculator::CalculatorSource;
pub use commands::CommandsSource;
pub use files::FilesSource;
pub use settings::SettingsSource;

/// What happens when the user activates a candidate.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum ActionTarget {
    OpenFile(String),
    RunCommand(String),
    OpenSettings(String),
    /// Copy plain text to the system clipboard.
    CopyText(String),
}

/// A single search candidate produced by a source.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct SearchCandidate {
    pub id: String,
    pub source_id: &'static str,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: &'static str,
    pub score: i32,
    pub action_hint: Option<String>,
    pub action_target: ActionTarget,
}

/// Abstract search source.
#[async_trait]
pub trait SearchSource: Send + Sync {
    /// Stable source identifier.
    fn id(&self) -> &'static str;
    /// i18n key for the user-facing source name.
    fn name_key(&self) -> &'static str;
    /// Icon name for the source badge.
    fn icon(&self) -> &'static str;
    /// Execute a search and return up to `limit` candidates.
    async fn search(&self, query: &str, limit: usize) -> Vec<SearchCandidate>;
}

