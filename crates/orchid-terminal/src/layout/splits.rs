//! Binary-tree split model.

use uuid::Uuid;

/// A node in a tab's split tree.
#[derive(Debug, Clone)]
pub enum SplitNode {
    /// A terminal pane backed by `session`.
    Leaf {
        /// Backing session id.
        session: Uuid,
    },
    /// An internal split with two children.
    Split {
        /// Layout direction.
        direction: SplitDirection,
        /// Fraction allocated to `first`, in `0.0..=1.0`.
        ratio: f32,
        /// First half (left / top).
        first: Box<SplitNode>,
        /// Second half (right / bottom).
        second: Box<SplitNode>,
    },
}

/// Horizontal = side-by-side; Vertical = stacked.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

impl SplitNode {
    /// Construct a leaf.
    #[must_use]
    pub fn leaf(session: Uuid) -> Self {
        Self::Leaf { session }
    }

    /// Iterate over every leaf in left-to-right, top-to-bottom order.
    pub fn leaves(&self, out: &mut Vec<Uuid>) {
        match self {
            Self::Leaf { session } => out.push(*session),
            Self::Split { first, second, .. } => {
                first.leaves(out);
                second.leaves(out);
            }
        }
    }

    /// Iterate mutable references to every leaf session id.
    pub fn leaves_mut(&mut self, out: &mut Vec<*mut Uuid>) {
        match self {
            Self::Leaf { session } => out.push(session as *mut _),
            Self::Split { first, second, .. } => {
                first.leaves_mut(out);
                second.leaves_mut(out);
            }
        }
    }

    /// Total number of leaves.
    #[must_use]
    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }
}
