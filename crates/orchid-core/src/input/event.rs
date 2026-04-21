//! Unified input events from every supported device.

use std::time::Instant;

use bitflags::bitflags;

use crate::command::shortcut::{Key, Modifiers};

/// 2D point in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

impl Point {
    /// Construct a point.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Euclidean distance to another point.
    #[must_use]
    pub fn distance_to(self, other: Point) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Single-frame input event.
///
/// Consumed by [`crate::GestureRecognizer`] and (for keyboard events) by
/// [`crate::InputMapper`].
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Touch (finger) event.
    Touch(TouchEvent),
    /// Mouse event.
    Mouse(MouseEvent),
    /// Keyboard event.
    Keyboard(KeyboardEvent),
    /// Pen / stylus event.
    Pen(PenEvent),
}

// ---------------------------------------------------------------------------
// Touch
// ---------------------------------------------------------------------------

/// Touch frame for a single pointer.
#[derive(Debug, Clone)]
pub struct TouchEvent {
    /// OS-assigned pointer id, stable for the lifetime of a touch.
    pub pointer_id: u32,
    /// Phase of the touch in its lifecycle.
    pub phase: TouchPhase,
    /// Current position on screen.
    pub position: Point,
    /// Normalised pressure, `0.0..=1.0`.
    pub pressure: f32,
    /// Contact area in pixels (OS-reported).
    pub size: f32,
    /// Monotonic timestamp.
    pub timestamp: Instant,
}

/// Touch lifecycle phase.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Began,
    Moved,
    Ended,
    Cancelled,
}

// ---------------------------------------------------------------------------
// Mouse
// ---------------------------------------------------------------------------

bitflags! {
    /// Bitset of mouse buttons currently pressed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct MouseButtons: u8 {
        /// Primary / left button.
        const LEFT   = 1 << 0;
        /// Secondary / right button.
        const RIGHT  = 1 << 1;
        /// Middle button (wheel click).
        const MIDDLE = 1 << 2;
        /// Extra button 1 (typically "back").
        const X1     = 1 << 3;
        /// Extra button 2 (typically "forward").
        const X2     = 1 << 4;
    }
}

/// Mouse frame.
#[derive(Debug, Clone)]
pub struct MouseEvent {
    /// Current cursor position.
    pub position: Point,
    /// Buttons currently held.
    pub buttons: MouseButtons,
    /// What specifically changed in this frame.
    pub kind: MouseEventKind,
    /// Monotonic timestamp.
    pub timestamp: Instant,
}

/// What a [`MouseEvent`] is reporting.
#[derive(Debug, Clone)]
pub enum MouseEventKind {
    /// Cursor moved without button change.
    Moved,
    /// Button was pressed.
    Pressed(MouseButton),
    /// Button was released.
    Released(MouseButton),
    /// Scroll-wheel or trackpad scroll.
    Scroll {
        /// Horizontal delta (pixels / lines).
        delta_x: f32,
        /// Vertical delta (pixels / lines).
        delta_y: f32,
    },
}

/// Named mouse button, used inside [`MouseEventKind::Pressed`] /
/// [`MouseEventKind::Released`].
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

/// Keyboard frame.
#[derive(Debug, Clone)]
pub struct KeyboardEvent {
    /// Canonical key.
    pub key: Key,
    /// Modifiers currently held.
    pub modifiers: Modifiers,
    /// Press or release.
    pub kind: KeyEventKind,
    /// Whether this is a repeated auto-key event.
    pub is_repeat: bool,
    /// Monotonic timestamp.
    pub timestamp: Instant,
}

/// Press vs release.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventKind {
    Pressed,
    Released,
}

// ---------------------------------------------------------------------------
// Pen
// ---------------------------------------------------------------------------

/// Pen / stylus frame.
#[derive(Debug, Clone)]
pub struct PenEvent {
    /// Tip position on screen.
    pub position: Point,
    /// Tip pressure, `0.0..=1.0`.
    pub pressure: f32,
    /// Horizontal tilt in radians (positive = right).
    pub tilt_x: f32,
    /// Vertical tilt in radians (positive = forward).
    pub tilt_y: f32,
    /// Lifecycle phase (mirrors touch phase).
    pub phase: TouchPhase,
    /// Whether the barrel button is held.
    pub barrel_button: bool,
    /// Whether the eraser end is active.
    pub eraser: bool,
    /// Monotonic timestamp.
    pub timestamp: Instant,
}
