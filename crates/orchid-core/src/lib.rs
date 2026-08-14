//! Core event bus, action system, command registry, and input abstractions
//! for Orchid.
//!
//! # Layout
//!
//! * [`event`] — typed event bus used as the central nervous system of the
//!   app.
//! * [`action`] — semantic operations triggered by users; an [`ActionDispatcher`]
//!   runs middleware around each action and a [`HistoryRecorder`] persists
//!   executed actions through [`orchid_storage`].
//! * [`command`] — user-visible commands, their metadata, textual parsing,
//!   keyboard shortcuts, and the fuzzy palette search.
//! * [`input`] — platform-agnostic input events, gesture recognition, screen
//!   zones, and the gesture/shortcut-to-command mapper.
//!
//! # Dispatch pipeline
//!
//! ```text
//! InputEvent --> GestureRecognizer --> RecognizedGesture --\
//!                                                           \
//! KeyboardEvent ---> Shortcut ------------------------------ > InputMapper ---> command id
//!                                                                                      |
//!                                                                    CommandRegistry <-+
//!                                                                      |
//!                                                           Box<dyn Action>
//!                                                                      |
//!                                                          ActionDispatcher (middleware)
//!                                                                      |
//!                                                                  EventBus
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
// `CoreError` and `StorageError` aggregate several sizeable upstream error
// types. Boxing them wholesale would allocate on every error path for no real
// benefit — the error path is the cold path.
#![allow(clippy::result_large_err)]

pub mod action;
pub mod command;
pub mod error;
pub mod event;
pub mod input;
pub mod job;

pub use action::{
    Action, ActionContext, ActionDispatcher, ActionMiddleware, ActionOutcome, HistoryRecorder,
    ReversiblePair, REVERSIBLE_WINDOW_SECONDS,
};
pub use command::{
    binding_conflict, display_profile_shortcut, is_file_manager_binding, is_reserved,
    lookup_fm_action, parse_command_line, parse_command_line_with_registry, profile_default_str,
    resolve_profile_shortcut, validate_override, ActionFactory, CommandArg, CommandArgKind,
    CommandCategory, CommandDescriptor, CommandPalette, CommandRegistry, Key, Modifiers,
    PaletteResult, ParsedCommand, ProfileBinding, Shortcut, ShortcutOverrideResult,
    ShortcutProfile, TerminalInvocation, PROFILE_BINDINGS,
};
pub use error::{CoreError, Result};
pub use event::{
    AppShuttingDown, AppStarted, ConfigUpdated, Event, EventBus, EventBusConfig, EventBusMetrics,
    EventEnvelope, EventFilter, EventReceiver, EventSource, HandlerPriority, SlowConsumerPolicy,
    SubscriptionHandle, SubscriptionId,
};
pub use input::{
    default_bindings, default_bindings_mirrored, Edge, GestureConfig, GesturePattern,
    GestureRecognizer, InputBindings, InputEvent, InputMapper, KeyEventKind, KeyboardEvent,
    MouseButton, MouseButtons, MouseEvent, MouseEventKind, PenEvent, Point, RecognizedGesture,
    ScreenBounds, ScreenZone, SwipeDirection, TouchEvent, TouchPhase,
};
pub use job::{BackgroundJobQueue, BoxedJobFuture, JobFactory};

/// Returns the version of this crate.
///
/// # Examples
///
/// ```
/// assert!(!orchid_core::version().is_empty());
/// ```
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
