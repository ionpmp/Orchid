//! Input system: raw events, screen zones, gesture recognition, and binding
//! resolution.
//!
//! The flow is:
//!
//! 1. The UI layer produces [`InputEvent`]s from the platform.
//! 2. [`GestureRecognizer`] consumes touch / pen events and emits
//!    [`RecognizedGesture`]s.
//! 3. [`InputMapper`] resolves recognised gestures and parsed keyboard
//!    [`crate::Shortcut`]s into command ids.
//! 4. The application hands that command id to [`crate::CommandRegistry`]
//!    and dispatches the resulting [`crate::Action`].

pub mod event;
pub mod gesture;
pub mod mapper;
pub mod zone;

pub use event::{
    InputEvent, KeyEventKind, KeyboardEvent, MouseButton, MouseButtons, MouseEvent, MouseEventKind,
    PenEvent, Point, TouchEvent, TouchPhase,
};
pub use gesture::{
    Edge, GestureConfig, GestureRecognizer, RecognizedGesture, SwipeDirection,
};
pub use mapper::{default_bindings, GesturePattern, InputBindings, InputMapper};
pub use zone::{ScreenBounds, ScreenZone};
