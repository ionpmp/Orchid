//! Action system.
//!
//! An **Action** is any semantically meaningful operation a user can
//! trigger — "move file", "create widget", "open terminal". Actions are the
//! single abstraction that unifies gestures, keyboard shortcuts, terminal
//! verbs, and command-palette entries: every one of those inputs ultimately
//! resolves to an [`Action`] and hands it to an [`ActionDispatcher`].
//!
//! See the [`dispatcher`] module for the middleware pipeline and the
//! [`history`] module for automatic persistence of executed actions.

pub mod context;
pub mod dispatcher;
pub mod history;
pub mod reversible;

use async_trait::async_trait;

pub use context::{ActionContext, ActionOutcome};
pub use dispatcher::{ActionDispatcher, ActionMiddleware};
pub use history::HistoryRecorder;
pub use reversible::{ReversiblePair, REVERSIBLE_WINDOW_SECONDS};

use crate::error::Result;

/// A unit of user-visible work.
///
/// Every Action carries a stable id (`"fs.move"`, ...), an i18n display key,
/// and a textual command representation that round-trips through history.
///
/// Implementations must be `Send + Sync + 'static` so the dispatcher can hold
/// them in `Box<dyn Action>`.
///
/// # Examples
///
/// ```
/// use async_trait::async_trait;
/// use orchid_core::{Action, ActionContext, ActionOutcome, Result};
///
/// struct Greet { pub name: String }
///
/// #[async_trait]
/// impl Action for Greet {
///     fn id(&self) -> &'static str { "demo.greet" }
///     fn display_name_key(&self) -> &'static str { "demo.greet.name" }
///     fn command_text(&self) -> String { format!("orc demo greet {:?}", self.name) }
///     async fn execute(&self, _ctx: &ActionContext) -> Result<ActionOutcome> {
///         Ok(ActionOutcome::ok_with_message(format!("hello, {}", self.name)))
///     }
/// }
/// ```
#[async_trait]
pub trait Action: Send + Sync + 'static {
    /// Stable textual id. Convention: `"domain.verb"`.
    fn id(&self) -> &'static str;

    /// i18n lookup key for the action's display name in menus and palettes.
    fn display_name_key(&self) -> &'static str;

    /// Textual command representation used in history, scripts, and the
    /// terminal. Must round-trip through the command parser.
    fn command_text(&self) -> String;

    /// Execute the action. May publish events, write to storage, etc.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CoreError`] on any internal failure. Domain-level
    /// failures should be encoded in a [`ActionOutcome`] with
    /// `success = false` instead of an `Err`.
    async fn execute(&self, ctx: &ActionContext) -> Result<ActionOutcome>;

    /// Whether this action is reversible. Defaults to `false`.
    fn is_reversible(&self) -> bool {
        false
    }

    /// Produce the inverse action, if this one is reversible.
    fn reverse(&self) -> Option<Box<dyn Action>> {
        None
    }

    /// Short description of what the action operates on. Used in history
    /// entries (`target` field).
    fn target(&self) -> Option<String> {
        None
    }
}
