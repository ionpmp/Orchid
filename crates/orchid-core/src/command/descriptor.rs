//! Metadata describing a registered command.

use serde::{Deserialize, Serialize};

use crate::command::shortcut::Shortcut;

/// User-facing description of a command available for invocation.
#[derive(Debug, Clone)]
pub struct CommandDescriptor {
    /// Stable id. Also used as the key in [`crate::CommandRegistry`].
    pub id: String,
    /// i18n key for the palette / menu label.
    pub display_name_key: String,
    /// i18n key for a longer description shown on hover / selection.
    pub description_key: Option<String>,
    /// Category used by the palette to group results.
    pub category: CommandCategory,
    /// Default keyboard shortcut (may be overridden by user config).
    pub default_shortcut: Option<Shortcut>,
    /// How the command is spelled on the terminal.
    pub terminal_invocation: Option<TerminalInvocation>,
    /// Icon name used by the palette, mapped to a glyph in the icon pack.
    pub icon_name: Option<String>,
}

/// Grouping for [`CommandDescriptor::category`].
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandCategory {
    System,
    File,
    Widget,
    Terminal,
    View,
    Navigation,
    Search,
    Settings,
    Developer,
    Custom,
}

/// How the command is spelled on the terminal.
#[derive(Debug, Clone)]
pub struct TerminalInvocation {
    /// Verb as typed after `orc`, e.g. `"fs move"` for `orc fs move ...`.
    pub verb: String,
    /// Positional / flag arguments, used for help text and completion.
    pub args: Vec<CommandArg>,
}

/// A single argument in a [`TerminalInvocation`].
#[derive(Debug, Clone)]
pub struct CommandArg {
    /// Short name shown in help text.
    pub name: String,
    /// i18n key for a longer description.
    pub description_key: Option<String>,
    /// If `false`, the argument may be omitted.
    pub required: bool,
    /// What kind of value this argument accepts.
    pub kind: CommandArgKind,
}

/// Type tag for a command argument.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandArgKind {
    String,
    Path,
    Integer,
    Boolean,
    /// Boolean `--flag` with no value.
    Flag,
}
