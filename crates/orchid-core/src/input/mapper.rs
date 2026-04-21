//! Gesture + shortcut → command-id mapping.
//!
//! [`InputMapper`] sits between [`crate::GestureRecognizer`] /
//! [`crate::Shortcut`] on one side and [`crate::CommandRegistry`] on the
//! other: given a recognised gesture or a parsed shortcut it answers
//! *"which command should I dispatch?"*.

use parking_lot::RwLock;

use crate::command::shortcut::Shortcut;
use crate::input::gesture::{Edge, RecognizedGesture, SwipeDirection};
use crate::input::zone::{ScreenBounds, ScreenZone};

/// Pattern of a gesture that should resolve to a command.
#[derive(Debug, Clone)]
pub enum GesturePattern {
    /// Swipe originating from a given edge with a specific pointer count.
    EdgeSwipe {
        /// Originating edge.
        edge: Edge,
        /// Number of simultaneous pointers.
        pointer_count: u8,
    },
    /// Multi-touch directional swipe.
    MultiTouchSwipe {
        /// Direction of the swipe.
        direction: SwipeDirection,
        /// Number of simultaneous pointers.
        pointer_count: u8,
    },
    /// Long press that falls into a specific screen zone.
    LongPressInZone {
        /// Expected zone.
        zone: ScreenZone,
    },
    /// Double-tap within a specific screen zone.
    DoubleTapInZone {
        /// Expected zone.
        zone: ScreenZone,
    },
}

/// Set of bindings applied by an [`InputMapper`].
#[derive(Debug, Default, Clone)]
pub struct InputBindings {
    /// Gesture → command-id mapping.
    pub gesture_bindings: Vec<(GesturePattern, String)>,
    /// Shortcut → command-id mapping.
    pub shortcut_bindings: Vec<(Shortcut, String)>,
}

/// Thread-safe dispatcher from input events to command ids.
pub struct InputMapper {
    bindings: RwLock<InputBindings>,
}

impl std::fmt::Debug for InputMapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let b = self.bindings.read();
        f.debug_struct("InputMapper")
            .field("gesture_bindings", &b.gesture_bindings.len())
            .field("shortcut_bindings", &b.shortcut_bindings.len())
            .finish()
    }
}

impl Default for InputMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl InputMapper {
    /// Build an empty mapper.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::{InputBindings, InputMapper};
    /// let m = InputMapper::new();
    /// m.set_bindings(InputBindings::default());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: RwLock::new(InputBindings::default()),
        }
    }

    /// Replace the active bindings.
    pub fn set_bindings(&self, bindings: InputBindings) {
        *self.bindings.write() = bindings;
    }

    /// Snapshot the current bindings.
    #[must_use]
    pub fn snapshot(&self) -> InputBindings {
        self.bindings.read().clone()
    }

    /// Try to resolve a gesture against the current bindings. Returns the
    /// first matching command id.
    #[must_use]
    pub fn resolve_gesture(
        &self,
        gesture: &RecognizedGesture,
        bounds: ScreenBounds,
    ) -> Option<String> {
        let b = self.bindings.read();
        for (pattern, cmd) in &b.gesture_bindings {
            if pattern_matches(pattern, gesture, bounds) {
                return Some(cmd.clone());
            }
        }
        None
    }

    /// Try to resolve a shortcut against the current bindings. Equal
    /// `Shortcut` values match.
    #[must_use]
    pub fn resolve_shortcut(&self, shortcut: &Shortcut) -> Option<String> {
        let b = self.bindings.read();
        b.shortcut_bindings
            .iter()
            .find(|(s, _)| s == shortcut)
            .map(|(_, cmd)| cmd.clone())
    }
}

/// Default bindings shipped with Orchid.
///
/// These command ids are placeholders — the commands themselves are
/// registered by other crates (`orchid-widgets`, `orchid-fs`, `orchid-ui`).
/// The mapping below encodes the design-spec defaults:
///
/// | trigger                      | command id                            |
/// |------------------------------|---------------------------------------|
/// | Edge swipe from left         | `"navigation.show_workspace_panel"`   |
/// | Edge swipe from right        | `"notification.show_center"`          |
/// | Edge swipe from bottom       | `"dock.show"`                         |
/// | Edge swipe from top          | `"search.show_universal"`             |
/// | Three-finger swipe up        | `"widget.show_all"`                   |
/// | Four-finger swipe left       | `"workspace.switch_previous"`         |
/// | Four-finger swipe right      | `"workspace.switch_next"`             |
/// | Two-finger tap (anywhere)    | `"navigation.back"`                   |
#[must_use]
pub fn default_bindings() -> InputBindings {
    InputBindings {
        gesture_bindings: vec![
            (
                GesturePattern::EdgeSwipe {
                    edge: Edge::Left,
                    pointer_count: 1,
                },
                "navigation.show_workspace_panel".into(),
            ),
            (
                GesturePattern::EdgeSwipe {
                    edge: Edge::Right,
                    pointer_count: 1,
                },
                "notification.show_center".into(),
            ),
            (
                GesturePattern::EdgeSwipe {
                    edge: Edge::Bottom,
                    pointer_count: 1,
                },
                "dock.show".into(),
            ),
            (
                GesturePattern::EdgeSwipe {
                    edge: Edge::Top,
                    pointer_count: 1,
                },
                "search.show_universal".into(),
            ),
            (
                GesturePattern::MultiTouchSwipe {
                    direction: SwipeDirection::Up,
                    pointer_count: 3,
                },
                "widget.show_all".into(),
            ),
            (
                GesturePattern::MultiTouchSwipe {
                    direction: SwipeDirection::Left,
                    pointer_count: 4,
                },
                "workspace.switch_previous".into(),
            ),
            (
                GesturePattern::MultiTouchSwipe {
                    direction: SwipeDirection::Right,
                    pointer_count: 4,
                },
                "workspace.switch_next".into(),
            ),
        ],
        shortcut_bindings: Vec::new(),
    }
}

fn pattern_matches(
    pattern: &GesturePattern,
    gesture: &RecognizedGesture,
    bounds: ScreenBounds,
) -> bool {
    match (pattern, gesture) {
        (
            GesturePattern::EdgeSwipe {
                edge: pe,
                pointer_count: pc,
            },
            RecognizedGesture::EdgeSwipe {
                edge,
                pointer_count,
                ..
            },
        ) => pe == edge && pc == pointer_count,

        (
            GesturePattern::MultiTouchSwipe {
                direction: pd,
                pointer_count: pc,
            },
            RecognizedGesture::Swipe {
                direction,
                pointer_count,
                ..
            },
        ) => pd == direction && pc == pointer_count,

        (
            GesturePattern::LongPressInZone { zone },
            RecognizedGesture::LongPress { position, .. },
        ) => ScreenZone::classify(*position, bounds) == *zone,

        (
            GesturePattern::DoubleTapInZone { zone },
            RecognizedGesture::DoubleTap { position },
        ) => ScreenZone::classify(*position, bounds) == *zone,

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::Point;

    #[test]
    fn default_bindings_resolve_left_edge_swipe() {
        let mapper = InputMapper::new();
        mapper.set_bindings(default_bindings());

        let g = RecognizedGesture::EdgeSwipe {
            edge: Edge::Left,
            distance: 120.0,
            pointer_count: 1,
        };
        let cmd = mapper.resolve_gesture(&g, ScreenBounds::new(1920.0, 1080.0));
        assert_eq!(cmd.as_deref(), Some("navigation.show_workspace_panel"));
    }

    #[test]
    fn default_bindings_resolve_four_finger_swipe_left() {
        let mapper = InputMapper::new();
        mapper.set_bindings(default_bindings());

        let g = RecognizedGesture::Swipe {
            from: Point::new(100.0, 100.0),
            to: Point::new(50.0, 100.0),
            direction: SwipeDirection::Left,
            velocity: 1.0,
            pointer_count: 4,
        };
        let cmd = mapper.resolve_gesture(&g, ScreenBounds::new(1920.0, 1080.0));
        assert_eq!(cmd.as_deref(), Some("workspace.switch_previous"));
    }

    #[test]
    fn shortcut_lookup_matches_exact() {
        let mapper = InputMapper::new();
        let ctrl_p = Shortcut::parse("Ctrl+P").unwrap();
        let bindings = InputBindings {
            gesture_bindings: Vec::new(),
            shortcut_bindings: vec![(ctrl_p.clone(), "command.palette.open".into())],
        };
        mapper.set_bindings(bindings);
        assert_eq!(
            mapper.resolve_shortcut(&ctrl_p).as_deref(),
            Some("command.palette.open")
        );
        assert!(mapper
            .resolve_shortcut(&Shortcut::parse("Ctrl+Shift+P").unwrap())
            .is_none());
    }

    #[test]
    fn unmatched_gesture_returns_none() {
        let mapper = InputMapper::new();
        mapper.set_bindings(default_bindings());
        let g = RecognizedGesture::Tap {
            position: Point::new(0.0, 0.0),
            pointer_count: 1,
        };
        let cmd = mapper.resolve_gesture(&g, ScreenBounds::new(1920.0, 1080.0));
        assert!(cmd.is_none());
    }
}
