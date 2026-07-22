//! Command descriptors + action factories for widget / workspace / group
//! operations. Consumers register the returned pairs into
//! [`orchid_core::CommandRegistry`].

use std::sync::Arc;

use async_trait::async_trait;
use orchid_core::{
    Action, ActionContext, ActionFactory, ActionOutcome, CommandArg, CommandArgKind,
    CommandCategory, CommandDescriptor, ParsedCommand, Shortcut, TerminalInvocation,
};
use orchid_storage::WidgetSize;
use uuid::Uuid;

use crate::group::GroupManager;
use crate::manager::{CreateWidgetRequest, WidgetManager};
use crate::registry::WidgetRegistry;
use crate::workspace::WorkspaceManager;

/// Build the complete set of widget-framework commands. The caller is
/// expected to register every pair into its [`orchid_core::CommandRegistry`].
pub fn build_command_set(
    widget_manager: Arc<WidgetManager>,
    workspace_manager: Arc<WorkspaceManager>,
    group_manager: Arc<GroupManager>,
    _registry: Arc<WidgetRegistry>,
) -> Vec<(CommandDescriptor, ActionFactory)> {
    vec![
        widget_create_command(widget_manager.clone()),
        widget_close_command(widget_manager.clone()),
        widget_move_command(widget_manager.clone()),
        widget_resize_command(widget_manager.clone()),
        widget_focus_next_command(),
        widget_show_all_command(),
        workspace_create_command(workspace_manager.clone()),
        workspace_delete_command(workspace_manager.clone()),
        workspace_switch_to_command(workspace_manager.clone()),
        workspace_switch_next_command(workspace_manager.clone()),
        workspace_switch_previous_command(workspace_manager),
        group_dissolve_command(group_manager),
    ]
}

// ---------------------------------------------------------------------------
// widget.create
// ---------------------------------------------------------------------------

fn widget_create_command(
    widget_manager: Arc<WidgetManager>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "widget.create".into(),
        display_name_key: "command-widget-create-name".into(),
        description_key: Some("command-widget-create-desc".into()),
        category: CommandCategory::Widget,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "widget create".into(),
            args: vec![
                CommandArg {
                    name: "type".into(),
                    description_key: Some("command-widget-create-arg-type".into()),
                    required: true,
                    kind: CommandArgKind::String,
                },
                CommandArg {
                    name: "workspace".into(),
                    description_key: None,
                    required: false,
                    kind: CommandArgKind::String,
                },
            ],
        }),
        icon_name: Some("widget-add".into()),
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let manager = widget_manager.clone();
        Ok(Box::new(WidgetCreateAction {
            manager,
            type_id: args
                .positional
                .first()
                .cloned()
                .unwrap_or_else(|| "terminal".into()),
            workspace_id: args.options.get("workspace").and_then(|s| Uuid::parse_str(s).ok()),
        }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct WidgetCreateAction {
    manager: Arc<WidgetManager>,
    type_id: String,
    workspace_id: Option<Uuid>,
}

#[async_trait]
impl Action for WidgetCreateAction {
    fn id(&self) -> &'static str {
        "widget.create"
    }
    fn display_name_key(&self) -> &'static str {
        "command-widget-create-name"
    }
    fn command_text(&self) -> String {
        match self.workspace_id {
            Some(ws) => format!("orc widget create {} --workspace={}", self.type_id, ws),
            None => format!("orc widget create {}", self.type_id),
        }
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        let workspace_id = self.workspace_id.unwrap_or_else(Uuid::nil);
        let req = CreateWidgetRequest {
            type_id: self.type_id.clone(),
            workspace_id,
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        };
        match self.manager.create(req).await {
            Ok(id) => Ok(ActionOutcome::ok_with_message(format!(
                "created {} ({})",
                self.type_id, id
            ))),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
    fn target(&self) -> Option<String> {
        Some(self.type_id.clone())
    }
}

// ---------------------------------------------------------------------------
// widget.close
// ---------------------------------------------------------------------------

fn widget_close_command(
    widget_manager: Arc<WidgetManager>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "widget.close".into(),
        display_name_key: "command-widget-close-name".into(),
        description_key: Some("command-widget-close-desc".into()),
        category: CommandCategory::Widget,
        default_shortcut: orchid_core::Shortcut::parse("Ctrl+W").ok(),
        terminal_invocation: Some(TerminalInvocation {
            verb: "widget close".into(),
            args: vec![CommandArg {
                name: "id".into(),
                description_key: None,
                required: true,
                kind: CommandArgKind::String,
            }],
        }),
        icon_name: Some("widget-close".into()),
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let manager = widget_manager.clone();
        let id = args
            .positional
            .first()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(Uuid::nil);
        Ok(Box::new(WidgetCloseAction { manager, id }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct WidgetCloseAction {
    manager: Arc<WidgetManager>,
    id: Uuid,
}

#[async_trait]
impl Action for WidgetCloseAction {
    fn id(&self) -> &'static str {
        "widget.close"
    }
    fn display_name_key(&self) -> &'static str {
        "command-widget-close-name"
    }
    fn command_text(&self) -> String {
        format!("orc widget close {}", self.id)
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        match self.manager.close(self.id).await {
            Ok(()) => Ok(ActionOutcome::ok()),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
    fn target(&self) -> Option<String> {
        Some(self.id.to_string())
    }
}

// ---------------------------------------------------------------------------
// widget.move / widget.resize
// ---------------------------------------------------------------------------

fn widget_move_command(
    widget_manager: Arc<WidgetManager>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "widget.move".into(),
        display_name_key: "command-widget-move-name".into(),
        description_key: None,
        category: CommandCategory::Widget,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "widget move".into(),
            args: Vec::new(),
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let manager = widget_manager.clone();
        let id = parse_uuid_positional(&args, 0);
        let col = parse_u16_positional(&args, 1);
        let row = parse_u16_positional(&args, 2);
        Ok(Box::new(WidgetMoveAction { manager, id, col, row }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct WidgetMoveAction {
    manager: Arc<WidgetManager>,
    id: Uuid,
    col: u16,
    row: u16,
}

#[async_trait]
impl Action for WidgetMoveAction {
    fn id(&self) -> &'static str {
        "widget.move"
    }
    fn display_name_key(&self) -> &'static str {
        "command-widget-move-name"
    }
    fn command_text(&self) -> String {
        format!("orc widget move {} {} {}", self.id, self.col, self.row)
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        match self
            .manager
            .move_to(self.id, orchid_storage::GridPosition { col: self.col, row: self.row })
            .await
        {
            Ok(()) => Ok(ActionOutcome::ok()),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
}

fn widget_resize_command(
    widget_manager: Arc<WidgetManager>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "widget.resize".into(),
        display_name_key: "command-widget-resize-name".into(),
        description_key: None,
        category: CommandCategory::Widget,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "widget resize".into(),
            args: Vec::new(),
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let manager = widget_manager.clone();
        let id = parse_uuid_positional(&args, 0);
        let size = args
            .positional
            .get(1)
            .map(|s| parse_widget_size(s))
            .unwrap_or(WidgetSize::Medium);
        Ok(Box::new(WidgetResizeAction { manager, id, size }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct WidgetResizeAction {
    manager: Arc<WidgetManager>,
    id: Uuid,
    size: WidgetSize,
}

#[async_trait]
impl Action for WidgetResizeAction {
    fn id(&self) -> &'static str {
        "widget.resize"
    }
    fn display_name_key(&self) -> &'static str {
        "command-widget-resize-name"
    }
    fn command_text(&self) -> String {
        format!("orc widget resize {} {:?}", self.id, self.size)
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        match self.manager.resize(self.id, self.size).await {
            Ok(()) => Ok(ActionOutcome::ok()),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// widget.focus_next / widget.show_all (no-ops at framework level; UI wires them)
// ---------------------------------------------------------------------------

fn widget_focus_next_command() -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "widget.focus_next".into(),
        display_name_key: "command-widget-focus_next-name".into(),
        description_key: None,
        category: CommandCategory::Widget,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "widget focus next".into(),
            args: Vec::new(),
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(|_: ParsedCommand| {
        Ok(Box::new(NoopAction {
            id: "widget.focus_next",
            name: "command-widget-focus_next-name",
            text: "orc widget focus next",
        }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

fn widget_show_all_command() -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "widget.show_all".into(),
        display_name_key: "command-widget-show_all-name".into(),
        description_key: None,
        category: CommandCategory::Widget,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "widget show all".into(),
            args: Vec::new(),
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(|_: ParsedCommand| {
        Ok(Box::new(NoopAction {
            id: "widget.show_all",
            name: "command-widget-show_all-name",
            text: "orc widget show all",
        }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

// ---------------------------------------------------------------------------
// workspace commands
// ---------------------------------------------------------------------------

fn workspace_create_command(
    workspace_manager: Arc<WorkspaceManager>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "workspace.create".into(),
        display_name_key: "command-workspace-create-name".into(),
        description_key: None,
        category: CommandCategory::View,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "workspace create".into(),
            args: vec![CommandArg {
                name: "name".into(),
                description_key: None,
                required: false,
                kind: CommandArgKind::String,
            }],
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let manager = workspace_manager.clone();
        let name = args
            .positional
            .first()
            .cloned()
            .unwrap_or_else(|| "Workspace".into());
        Ok(Box::new(WorkspaceCreateAction { manager, name }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct WorkspaceCreateAction {
    manager: Arc<WorkspaceManager>,
    name: String,
}

#[async_trait]
impl Action for WorkspaceCreateAction {
    fn id(&self) -> &'static str {
        "workspace.create"
    }
    fn display_name_key(&self) -> &'static str {
        "command-workspace-create-name"
    }
    fn command_text(&self) -> String {
        format!("orc workspace create {:?}", self.name)
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        match self.manager.create(self.name.clone()).await {
            Ok(id) => Ok(ActionOutcome::ok_with_message(id.to_string())),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
}

fn workspace_delete_command(
    workspace_manager: Arc<WorkspaceManager>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "workspace.delete".into(),
        display_name_key: "command-workspace-delete-name".into(),
        description_key: None,
        category: CommandCategory::View,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "workspace delete".into(),
            args: vec![CommandArg {
                name: "id".into(),
                description_key: None,
                required: true,
                kind: CommandArgKind::String,
            }],
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let manager = workspace_manager.clone();
        let id = parse_uuid_positional(&args, 0);
        Ok(Box::new(WorkspaceDeleteAction { manager, id }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct WorkspaceDeleteAction {
    manager: Arc<WorkspaceManager>,
    id: Uuid,
}

#[async_trait]
impl Action for WorkspaceDeleteAction {
    fn id(&self) -> &'static str {
        "workspace.delete"
    }
    fn display_name_key(&self) -> &'static str {
        "command-workspace-delete-name"
    }
    fn command_text(&self) -> String {
        format!("orc workspace delete {}", self.id)
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        match self.manager.delete(self.id).await {
            Ok(()) => Ok(ActionOutcome::ok()),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
}

fn workspace_switch_to_command(
    workspace_manager: Arc<WorkspaceManager>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "workspace.switch_to".into(),
        display_name_key: "command-workspace-switch_to-name".into(),
        description_key: None,
        category: CommandCategory::View,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "workspace switch".into(),
            args: vec![CommandArg {
                name: "ordinal".into(),
                description_key: None,
                required: true,
                kind: CommandArgKind::Integer,
            }],
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let manager = workspace_manager.clone();
        let ordinal: u8 = args
            .positional
            .first()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(1);
        Ok(Box::new(WorkspaceSwitchToAction { manager, ordinal }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct WorkspaceSwitchToAction {
    manager: Arc<WorkspaceManager>,
    ordinal: u8,
}

#[async_trait]
impl Action for WorkspaceSwitchToAction {
    fn id(&self) -> &'static str {
        "workspace.switch_to"
    }
    fn display_name_key(&self) -> &'static str {
        "command-workspace-switch_to-name"
    }
    fn command_text(&self) -> String {
        format!("orc workspace switch {}", self.ordinal)
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        match self.manager.switch_by_ordinal(self.ordinal).await {
            Ok(()) => Ok(ActionOutcome::ok()),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
}

fn workspace_switch_next_command(
    workspace_manager: Arc<WorkspaceManager>,
) -> (CommandDescriptor, ActionFactory) {
    simple_workspace_command(
        "workspace.switch_next",
        "command-workspace-switch_next-name",
        "workspace switch next",
        workspace_manager,
        WorkspaceSwitchDirection::Next,
        Shortcut::parse("Ctrl+Alt+ArrowRight").ok(),
    )
}

fn workspace_switch_previous_command(
    workspace_manager: Arc<WorkspaceManager>,
) -> (CommandDescriptor, ActionFactory) {
    simple_workspace_command(
        "workspace.switch_previous",
        "command-workspace-switch_previous-name",
        "workspace switch previous",
        workspace_manager,
        WorkspaceSwitchDirection::Previous,
        Shortcut::parse("Ctrl+Alt+ArrowLeft").ok(),
    )
}

#[derive(Clone, Copy)]
enum WorkspaceSwitchDirection {
    Next,
    Previous,
}

fn simple_workspace_command(
    id: &'static str,
    name_key: &'static str,
    verb: &'static str,
    manager: Arc<WorkspaceManager>,
    dir: WorkspaceSwitchDirection,
    default_shortcut: Option<Shortcut>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: id.into(),
        display_name_key: name_key.into(),
        description_key: None,
        category: CommandCategory::View,
        default_shortcut,
        terminal_invocation: Some(TerminalInvocation {
            verb: verb.into(),
            args: Vec::new(),
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(move |_: ParsedCommand| {
        Ok(Box::new(WorkspaceRelativeSwitchAction {
            id,
            name_key,
            manager: manager.clone(),
            dir,
        }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct WorkspaceRelativeSwitchAction {
    id: &'static str,
    name_key: &'static str,
    manager: Arc<WorkspaceManager>,
    dir: WorkspaceSwitchDirection,
}

#[async_trait]
impl Action for WorkspaceRelativeSwitchAction {
    fn id(&self) -> &'static str {
        self.id
    }
    fn display_name_key(&self) -> &'static str {
        self.name_key
    }
    fn command_text(&self) -> String {
        match self.dir {
            WorkspaceSwitchDirection::Next => "orc workspace switch next".into(),
            WorkspaceSwitchDirection::Previous => "orc workspace switch previous".into(),
        }
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        let res = match self.dir {
            WorkspaceSwitchDirection::Next => self.manager.switch_next().await,
            WorkspaceSwitchDirection::Previous => self.manager.switch_previous().await,
        };
        match res {
            Ok(()) => Ok(ActionOutcome::ok()),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// Group dissolve
// ---------------------------------------------------------------------------

fn group_dissolve_command(
    group_manager: Arc<GroupManager>,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: "widget.group.dissolve".into(),
        display_name_key: "command-widget-group-dissolve-name".into(),
        description_key: None,
        category: CommandCategory::Widget,
        default_shortcut: None,
        terminal_invocation: Some(TerminalInvocation {
            verb: "widget group dissolve".into(),
            args: Vec::new(),
        }),
        icon_name: None,
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let manager = group_manager.clone();
        let id = parse_uuid_positional(&args, 0);
        Ok(Box::new(GroupDissolveAction { manager, id }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct GroupDissolveAction {
    manager: Arc<GroupManager>,
    id: Uuid,
}

#[async_trait]
impl Action for GroupDissolveAction {
    fn id(&self) -> &'static str {
        "widget.group.dissolve"
    }
    fn display_name_key(&self) -> &'static str {
        "command-widget-group-dissolve-name"
    }
    fn command_text(&self) -> String {
        format!("orc widget group dissolve {}", self.id)
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        match self.manager.dissolve_group(self.id).await {
            Ok(members) => Ok(ActionOutcome::ok_with_message(format!(
                "{} members released",
                members.len()
            ))),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

struct NoopAction {
    id: &'static str,
    name: &'static str,
    text: &'static str,
}

#[async_trait]
impl Action for NoopAction {
    fn id(&self) -> &'static str {
        self.id
    }
    fn display_name_key(&self) -> &'static str {
        self.name
    }
    fn command_text(&self) -> String {
        self.text.to_string()
    }
    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        Ok(ActionOutcome::ok())
    }
}

fn parse_uuid_positional(args: &ParsedCommand, idx: usize) -> Uuid {
    args.positional
        .get(idx)
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil)
}

fn parse_u16_positional(args: &ParsedCommand, idx: usize) -> u16 {
    args.positional
        .get(idx)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0)
}

fn parse_widget_size(s: &str) -> WidgetSize {
    match s.to_ascii_lowercase().as_str() {
        "small" => WidgetSize::Small,
        "medium" => WidgetSize::Medium,
        "large" => WidgetSize::Large,
        "extra-large" | "extralarge" | "xl" => WidgetSize::ExtraLarge,
        _ => WidgetSize::Medium,
    }
}
