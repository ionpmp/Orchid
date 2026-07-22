//! UI-facing commands registered into the shared [`orchid_core::CommandRegistry`].

use std::sync::Arc;

use async_trait::async_trait;
use orchid_core::{
    Action, ActionContext, ActionFactory, ActionOutcome, CommandCategory, CommandDescriptor,
    ParsedCommand, Shortcut, TerminalInvocation,
};

/// Commands handled primarily by the main window (palette + shortcuts + gestures).
pub fn build_ui_command_set() -> Vec<(CommandDescriptor, ActionFactory)> {
    vec![
        settings_open_command(),
        settings_open_config_file_command(),
        password_lock_command(),
        navigation_show_workspace_panel_command(),
        notification_show_center_command(),
        dock_show_command(),
        search_show_universal_command(),
        onboarding_toggle_hint_mode_command(),
    ]
}

fn settings_open_command() -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "settings.open".into(),
        display_name_key: "command-settings-open-name".into(),
        description_key: Some("command-settings-open-desc".into()),
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

fn settings_open_config_file_command() -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "settings.open_config_file".into(),
        display_name_key: "command-settings-open_config_file-name".into(),
        description_key: Some("command-settings-open_config_file-desc".into()),
        category: CommandCategory::Settings,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "settings open config file".into(),
            args: Vec::new(),
        }),
        icon_name: Some("settings".into()),
    };
    let factory: ActionFactory = Arc::new(|_: ParsedCommand| {
        Ok(Box::new(SettingsOpenConfigFileAction) as Box<dyn Action>)
    });
    (descriptor, factory)
}

fn password_lock_command() -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "password.lock".into(),
        display_name_key: "command-password-lock-name".into(),
        description_key: Some("command-password-lock-desc".into()),
        category: CommandCategory::Settings,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "password lock".into(),
            args: Vec::new(),
        }),
        icon_name: Some("password".into()),
    };
    let factory: ActionFactory = Arc::new(|_: ParsedCommand| {
        Ok(Box::new(PasswordLockAction) as Box<dyn Action>)
    });
    (descriptor, factory)
}

fn navigation_show_workspace_panel_command() -> (CommandDescriptor, ActionFactory) {
    panel_toggle_command(
        "navigation.show_workspace_panel",
        "command-navigation-show_workspace_panel-name",
        Some("command-navigation-show_workspace_panel-desc"),
        CommandCategory::Navigation,
        "navigation show workspace panel",
    )
}

fn notification_show_center_command() -> (CommandDescriptor, ActionFactory) {
    panel_toggle_command(
        "notification.show_center",
        "command-notification-show_center-name",
        Some("command-notification-show_center-desc"),
        CommandCategory::Navigation,
        "notification show center",
    )
}

fn dock_show_command() -> (CommandDescriptor, ActionFactory) {
    panel_toggle_command(
        "dock.show",
        "command-dock-show-name",
        Some("command-dock-show-desc"),
        CommandCategory::Navigation,
        "dock show",
    )
}

fn search_show_universal_command() -> (CommandDescriptor, ActionFactory) {
    panel_toggle_command(
        "search.show_universal",
        "command-search-show_universal-name",
        Some("command-search-show_universal-desc"),
        CommandCategory::Search,
        "search show universal",
    )
}

fn onboarding_toggle_hint_mode_command() -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "onboarding.toggle_hint_mode".into(),
        display_name_key: "command-onboarding-toggle_hint_mode-name".into(),
        description_key: Some("command-onboarding-toggle_hint_mode-desc".into()),
        category: CommandCategory::Settings,
        default_shortcut: Shortcut::parse("Win+?").ok(),
        terminal_invocation: Some(TerminalInvocation {
            verb: "onboarding toggle hint mode".into(),
            args: Vec::new(),
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(|_: ParsedCommand| {
        Ok(Box::new(PanelToggleAction {
            id: "onboarding.toggle_hint_mode",
            name_key: "command-onboarding-toggle_hint_mode-name",
            verb: "onboarding toggle hint mode",
        }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

fn panel_toggle_command(
    id: &'static str,
    name_key: &'static str,
    description_key: Option<&'static str>,
    category: CommandCategory,
    verb: &'static str,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: id.into(),
        display_name_key: name_key.into(),
        description_key: description_key.map(str::to_string),
        category,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: verb.into(),
            args: Vec::new(),
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(move |_: ParsedCommand| {
        Ok(Box::new(PanelToggleAction { id, name_key, verb }) as Box<dyn Action>)
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
        "command-settings-open-name"
    }
    fn command_text(&self) -> String {
        "orc settings open".into()
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        Ok(ActionOutcome::ok())
    }
}

struct SettingsOpenConfigFileAction;

#[async_trait]
impl Action for SettingsOpenConfigFileAction {
    fn id(&self) -> &'static str {
        "settings.open_config_file"
    }
    fn display_name_key(&self) -> &'static str {
        "command-settings-open_config_file-name"
    }
    fn command_text(&self) -> String {
        "orc settings open config file".into()
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        Ok(ActionOutcome::ok())
    }
}

struct PasswordLockAction;

#[async_trait]
impl Action for PasswordLockAction {
    fn id(&self) -> &'static str {
        "password.lock"
    }
    fn display_name_key(&self) -> &'static str {
        "command-password-lock-name"
    }
    fn command_text(&self) -> String {
        "orc password lock".into()
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        Ok(ActionOutcome::ok())
    }
}

/// Registered for palette / terminal parity; the main window wires UI behavior.
struct PanelToggleAction {
    id: &'static str,
    name_key: &'static str,
    verb: &'static str,
}

#[async_trait]
impl Action for PanelToggleAction {
    fn id(&self) -> &'static str {
        self.id
    }
    fn display_name_key(&self) -> &'static str {
        self.name_key
    }
    fn command_text(&self) -> String {
        format!("orc {}", self.verb)
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        Ok(ActionOutcome::ok())
    }
}
