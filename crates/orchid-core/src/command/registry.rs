//! Central directory of commands available for invocation.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;

use crate::action::Action;
use crate::command::descriptor::{CommandCategory, CommandDescriptor};
use crate::command::parser::ParsedCommand;
use crate::command::shortcut::Shortcut;
use crate::error::{CoreError, Result};

/// Factory function used to construct an [`Action`] from parsed arguments.
///
/// Every registered command provides one. `Arc<dyn Fn(...)>` makes the factory
/// cheap to clone and share across registrations.
pub type ActionFactory = Arc<
    dyn Fn(ParsedCommand) -> Result<Box<dyn Action>> + Send + Sync + 'static,
>;

struct CommandEntry {
    descriptor: CommandDescriptor,
    factory: ActionFactory,
    effective_shortcut: RwLock<Option<Shortcut>>,
}

/// In-memory directory keyed by command id plus a secondary index by verb.
///
/// The registry is thread-safe and designed to be wrapped in an `Arc`.
pub struct CommandRegistry {
    inner: DashMap<String, CommandEntry>,
    by_verb: DashMap<String, String>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandRegistry")
            .field("count", &self.inner.len())
            .finish()
    }
}

/// Outcome of a single shortcut override from
/// [`CommandRegistry::apply_shortcut_overrides`].
#[derive(Debug, Clone)]
pub struct ShortcutOverrideResult {
    /// Command id the override was targeting.
    pub command_id: String,
    /// `Ok(shortcut)` if the override was applied, `Err(reason)` otherwise.
    pub outcome: std::result::Result<Shortcut, String>,
}

impl CommandRegistry {
    /// Build an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
            by_verb: DashMap::new(),
        }
    }

    /// Register a command and its action factory.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::DuplicateCommand`] if a command with the same id
    /// is already registered.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use orchid_core::{
    ///     CommandCategory, CommandDescriptor, CommandRegistry, ParsedCommand,
    /// };
    /// # use orchid_core::{Action, ActionContext, ActionOutcome, Result};
    /// # use async_trait::async_trait;
    /// # struct Noop;
    /// # #[async_trait]
    /// # impl Action for Noop {
    /// #   fn id(&self) -> &'static str { "demo.noop" }
    /// #   fn display_name_key(&self) -> &'static str { "demo.noop.name" }
    /// #   fn command_text(&self) -> String { "orc demo noop".into() }
    /// #   async fn execute(&self, _: &ActionContext) -> Result<ActionOutcome> {
    /// #       Ok(ActionOutcome::ok())
    /// #   }
    /// # }
    /// let reg = CommandRegistry::new();
    /// reg.register(
    ///     CommandDescriptor {
    ///         id: "demo.noop".into(),
    ///         display_name_key: "demo.noop.name".into(),
    ///         description_key: None,
    ///         category: CommandCategory::Developer,
    ///         default_shortcut: None,
    ///         terminal_invocation: None,
    ///         icon_name: None,
    ///     },
    ///     Arc::new(|_args: ParsedCommand| Ok(Box::new(Noop) as Box<dyn Action>)),
    /// )
    /// .unwrap();
    /// ```
    pub fn register(
        &self,
        descriptor: CommandDescriptor,
        factory: ActionFactory,
    ) -> Result<()> {
        if self.inner.contains_key(&descriptor.id) {
            return Err(CoreError::DuplicateCommand(descriptor.id));
        }
        let default_shortcut = descriptor.default_shortcut.clone();
        let verb = descriptor
            .terminal_invocation
            .as_ref()
            .map(|t| t.verb.clone());
        let id = descriptor.id.clone();

        self.inner.insert(
            id.clone(),
            CommandEntry {
                descriptor,
                factory,
                effective_shortcut: RwLock::new(default_shortcut),
            },
        );
        if let Some(v) = verb {
            self.by_verb.insert(v, id);
        }
        Ok(())
    }

    /// Remove a command by id. Returns `true` if an entry was actually
    /// removed.
    pub fn unregister(&self, id: &str) -> bool {
        if let Some((_, entry)) = self.inner.remove(id) {
            if let Some(t) = &entry.descriptor.terminal_invocation {
                self.by_verb.remove(&t.verb);
            }
            true
        } else {
            false
        }
    }

    /// Fetch a command descriptor by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<CommandDescriptor> {
        self.inner.get(id).map(|e| e.descriptor.clone())
    }

    /// Every registered descriptor, in no particular order.
    #[must_use]
    pub fn list_all(&self) -> Vec<CommandDescriptor> {
        self.inner.iter().map(|e| e.descriptor.clone()).collect()
    }

    /// All descriptors in a given category.
    #[must_use]
    pub fn list_by_category(&self, category: CommandCategory) -> Vec<CommandDescriptor> {
        self.inner
            .iter()
            .filter(|e| e.descriptor.category == category)
            .map(|e| e.descriptor.clone())
            .collect()
    }

    /// Build an action for command `id` from already-parsed arguments.
    ///
    /// # Errors
    ///
    /// * [`CoreError::CommandNotFound`] if `id` is not registered.
    /// * Propagates whatever error the factory returns.
    pub fn build_action(&self, id: &str, args: ParsedCommand) -> Result<Box<dyn Action>> {
        let entry = self
            .inner
            .get(id)
            .ok_or_else(|| CoreError::CommandNotFound(id.into()))?;
        (entry.factory)(args)
    }

    /// Apply a batch of user shortcut overrides.
    ///
    /// Each entry in `overrides` is `command_id -> shortcut_string`. Every
    /// override yields a [`ShortcutOverrideResult`] describing whether it was
    /// applied or rejected (e.g. invalid syntax, reserved shortcut, unknown
    /// command).
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::CommandRegistry;
    /// let reg = CommandRegistry::new();
    /// let results = reg.apply_shortcut_overrides(&std::collections::HashMap::new());
    /// assert!(results.is_empty());
    /// ```
    pub fn apply_shortcut_overrides(
        &self,
        overrides: &HashMap<String, String>,
    ) -> Vec<ShortcutOverrideResult> {
        let mut out = Vec::with_capacity(overrides.len());
        for (id, s) in overrides {
            let result = match Shortcut::parse(s) {
                Ok(sc) => {
                    if let Some(reason) = crate::command::shortcut::is_reserved(&sc) {
                        ShortcutOverrideResult {
                            command_id: id.clone(),
                            outcome: Err(reason.to_string()),
                        }
                    } else if let Some(entry) = self.inner.get(id) {
                        *entry.effective_shortcut.write() = Some(sc.clone());
                        ShortcutOverrideResult {
                            command_id: id.clone(),
                            outcome: Ok(sc),
                        }
                    } else {
                        ShortcutOverrideResult {
                            command_id: id.clone(),
                            outcome: Err(format!("unknown command `{id}`")),
                        }
                    }
                }
                Err(e) => ShortcutOverrideResult {
                    command_id: id.clone(),
                    outcome: Err(e.to_string()),
                },
            };
            out.push(result);
        }
        out
    }

    /// Currently effective shortcut for a command id, accounting for
    /// overrides.
    #[must_use]
    pub fn effective_shortcut(&self, id: &str) -> Option<Shortcut> {
        self.inner.get(id).and_then(|e| e.effective_shortcut.read().clone())
    }

    // ------------------------------------------------------------------
    // Helpers used by the parser to resolve multi-word verbs.
    // ------------------------------------------------------------------

    /// Whether a verb string is registered.
    #[must_use]
    pub fn has_verb(&self, verb: &str) -> bool {
        self.by_verb.contains_key(verb)
    }

    /// Look up a descriptor by its terminal verb.
    #[must_use]
    pub fn get_by_verb(&self, verb: &str) -> Option<CommandDescriptor> {
        let id = self.by_verb.get(verb).map(|e| e.value().clone())?;
        self.get(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, ActionContext, ActionOutcome};
    use async_trait::async_trait;

    struct Noop;
    #[async_trait]
    impl Action for Noop {
        fn id(&self) -> &'static str {
            "t.noop"
        }
        fn display_name_key(&self) -> &'static str {
            "t.noop.name"
        }
        fn command_text(&self) -> String {
            "orc t noop".into()
        }
        async fn execute(&self, _: &ActionContext) -> Result<ActionOutcome> {
            Ok(ActionOutcome::ok())
        }
    }

    fn desc(id: &str, verb: Option<&str>, shortcut: Option<&str>) -> CommandDescriptor {
        CommandDescriptor {
            id: id.into(),
            display_name_key: format!("{id}.name"),
            description_key: None,
            category: CommandCategory::Developer,
            default_shortcut: shortcut.map(|s| Shortcut::parse(s).unwrap()),
            terminal_invocation: verb.map(|v| crate::command::descriptor::TerminalInvocation {
                verb: v.into(),
                args: Vec::new(),
            }),
            icon_name: None,
        }
    }

    fn factory() -> ActionFactory {
        Arc::new(|_| Ok(Box::new(Noop) as Box<dyn Action>))
    }

    #[test]
    fn duplicate_registration_errors() {
        let reg = CommandRegistry::new();
        reg.register(desc("a", None, None), factory()).unwrap();
        let err = reg.register(desc("a", None, None), factory()).unwrap_err();
        assert!(matches!(err, CoreError::DuplicateCommand(_)));
    }

    #[test]
    fn apply_shortcut_overrides_reports_per_entry() {
        let reg = CommandRegistry::new();
        reg.register(desc("a", None, Some("Ctrl+A")), factory()).unwrap();
        reg.register(desc("b", None, None), factory()).unwrap();

        let mut overrides = HashMap::new();
        overrides.insert("a".into(), "Ctrl+Shift+A".into());
        overrides.insert("b".into(), "not-a-shortcut".into());
        overrides.insert("unknown".into(), "Ctrl+U".into());
        overrides.insert("a".into(), "Ctrl+Shift+A".into());

        let results = reg.apply_shortcut_overrides(&overrides);
        assert_eq!(results.len(), 3);

        let by_id: HashMap<_, _> = results.into_iter().map(|r| (r.command_id, r.outcome)).collect();
        assert!(by_id["a"].is_ok());
        assert!(by_id["b"].is_err());
        assert!(by_id["unknown"].is_err());

        assert_eq!(
            reg.effective_shortcut("a"),
            Some(Shortcut::parse("Ctrl+Shift+A").unwrap())
        );
    }

    #[test]
    fn list_by_category_filters() {
        let reg = CommandRegistry::new();
        reg.register(
            CommandDescriptor {
                category: CommandCategory::File,
                ..desc("fs.move", None, None)
            },
            factory(),
        )
        .unwrap();
        reg.register(
            CommandDescriptor {
                category: CommandCategory::Widget,
                ..desc("widget.create", None, None)
            },
            factory(),
        )
        .unwrap();
        assert_eq!(reg.list_by_category(CommandCategory::File).len(), 1);
        assert_eq!(reg.list_by_category(CommandCategory::Widget).len(), 1);
        assert_eq!(reg.list_by_category(CommandCategory::System).len(), 0);
    }

    #[test]
    fn build_action_unknown_id_errors() {
        let reg = CommandRegistry::new();
        let err = match reg.build_action("missing", ParsedCommand::default()) {
            Ok(_) => panic!("expected CommandNotFound"),
            Err(e) => e,
        };
        assert!(matches!(err, CoreError::CommandNotFound(_)));
    }
}
