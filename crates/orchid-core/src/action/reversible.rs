//! Helpers for actions that can be undone within a limited time window.

use crate::action::Action;

/// Window, in seconds, within which a reversible action can still be undone
/// from the action-history UI.
///
/// Matches the design spec's "undo toast" timeout.
pub const REVERSIBLE_WINDOW_SECONDS: i64 = 15;

/// Pairing of a forward action with its precomputed reverse.
///
/// Useful for building reversible operations up front: construct the reverse
/// once and check that both sides agree on the `command_text` they would
/// produce.
///
/// # Examples
///
/// ```ignore
/// let pair = ReversiblePair {
///     forward: Box::new(FileMove { src: "a".into(), dst: "b".into() }),
///     reverse: Box::new(FileMove { src: "b".into(), dst: "a".into() }),
/// };
/// ```
pub struct ReversiblePair {
    /// The action to run first.
    pub forward: Box<dyn Action>,
    /// The action that undoes `forward`.
    pub reverse: Box<dyn Action>,
}

impl std::fmt::Debug for ReversiblePair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReversiblePair")
            .field("forward", &self.forward.id())
            .field("reverse", &self.reverse.id())
            .finish()
    }
}
