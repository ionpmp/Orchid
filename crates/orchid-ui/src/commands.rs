//! UI-facing commands registered into the shared [`orchid_core::CommandRegistry`].

use std::sync::Arc;

use async_trait::async_trait;
use orchid_core::{
    Action, ActionContext, ActionFactory, ActionOutcome, CommandCategory, CommandDescriptor,
    ParsedCommand, Shortcut, TerminalInvocation,
};

/// Commands handled primarily by the main window (palette + shortcuts).
pub fn build_ui_command_set() -> Vec<(CommandDescriptor, ActionFactory)> {
    vec![settings_open_command()]
}

fn settings_open_command() -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "settings.open".into(),
        display_name_key: "command.settings.open.name".into(),
        description_key: Some("command.settings.open.desc".into()),
        category: CommandCategory::Settings,
        default_shortcut: Shortcut::parse("Ctrl+,").ok(),
        terminal_invocation: Some(TerminalInvocation {
            verb: "settings open".into(),
            args: Vec::new(),
        }),
        icon_name: Some("settings".into()),
    };
    let factory: ActionFactory = Arc::new(|_: ParsedCommand| {
        Ok(Box::new(SettingsOpenAction) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct SettingsOpenAction;

#[async_trait]
impl Action for SettingsOpenAction {
    fn id(&self) -> &'static str {
        "settings.open"
    }
    fn display_name_key(&self) -> &'static str {
        "command.settings.open.name"
    }
    fn command_text(&self) -> String {
        "orc settings open".into()
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        Ok(ActionOutcome::ok())
    }
}
