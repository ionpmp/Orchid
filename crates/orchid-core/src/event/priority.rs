//! Priority levels for event handlers.

use serde::{Deserialize, Serialize};

/// Priority tier at which an event handler runs.
///
/// Handlers are invoked in ascending numeric order: [`Critical`] runs first,
/// [`Audit`] runs last. Within a single tier the order is the order of
/// subscription (FIFO).
///
/// [`Critical`]: HandlerPriority::Critical
/// [`Audit`]: HandlerPriority::Audit
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(i8)]
pub enum HandlerPriority {
    /// Runs first. Use sparingly, e.g. for security gates.
    Critical = -100,
    /// Runs before [`HandlerPriority::Normal`].
    High = -50,
    /// Default priority.
    #[default]
    Normal = 0,
    /// Runs after [`HandlerPriority::Normal`].
    Low = 50,
    /// Runs last. Use for logging / metrics.
    Audit = 100,
}

impl HandlerPriority {
    /// Numeric rank used for sorting (`Critical < Audit`).
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::HandlerPriority;
    /// assert!(HandlerPriority::Critical.rank() < HandlerPriority::Audit.rank());
    /// ```
    #[must_use]
    pub fn rank(self) -> i8 {
        self as i8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_ascending_by_rank() {
        let mut v = vec![
            HandlerPriority::Low,
            HandlerPriority::Critical,
            HandlerPriority::Audit,
            HandlerPriority::High,
            HandlerPriority::Normal,
        ];
        v.sort();
        assert_eq!(
            v,
            vec![
                HandlerPriority::Critical,
                HandlerPriority::High,
                HandlerPriority::Normal,
                HandlerPriority::Low,
                HandlerPriority::Audit,
            ]
        );
    }

    #[test]
    fn default_is_normal() {
        assert_eq!(HandlerPriority::default(), HandlerPriority::Normal);
    }
}
