//! Command system: descriptors, registry, parser, shortcuts, and palette.
//!
//! A **command** is a user-visible operation identified by a stable string id
//! (e.g. `"fs.move"`). Commands do not own behaviour directly; instead every
//! command carries a [`CommandDescriptor`] (name, category, default
//! shortcut, terminal invocation) plus an [`ActionFactory`] that builds an
//! [`crate::Action`] from a [`ParsedCommand`].
//!
//! The three main surfaces that consume this module:
//!
//! * The command palette uses [`CommandPalette`] to fuzzy-search.
//! * Keyboard / gesture bindings use [`Shortcut`] and [`ParsedCommand`] via
//!   [`crate::input::InputMapper`].
//! * The terminal uses [`parse_command_line`] to turn user input into a
//!   [`ParsedCommand`] resolved through [`CommandRegistry`].

pub mod descriptor;
pub mod palette;
pub mod parser;
pub mod registry;
pub mod shortcut;

pub use descriptor::{
    CommandArg, CommandArgKind, CommandCategory, CommandDescriptor, TerminalInvocation,
};
pub use palette::{CommandPalette, PaletteResult};
pub use parser::{parse_command_line, parse_command_line_with_registry, ParsedCommand};
pub use registry::{ActionFactory, CommandRegistry, ShortcutOverrideResult};
pub use shortcut::{is_reserved, Key, Modifiers, Shortcut};
