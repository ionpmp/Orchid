//! Terminal layout commands for the shared [`orchid_core::CommandRegistry`].
//!
//! Targets the first terminal widget on the active workspace, or the instance
//! passed via `--instance=<uuid>`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use orchid_core::{
    Action, ActionContext, ActionFactory, ActionOutcome, CommandCategory, CommandDescriptor,
    ParsedCommand, Shortcut, TerminalInvocation,
};
use orchid_widgets::{WidgetManager, WorkspaceManager};
use parking_lot::Mutex;
use uuid::Uuid;

use super::widget::{
    add_tab, close_focused_pane_or_tab, focus_next_pane, focus_previous_pane, split_horizontal,
    split_vertical, switch_tab_relative, TerminalWidgetDeps, TERMINAL_TYPE_ID,
};

/// Build terminal layout commands. Register each pair into [`orchid_core::CommandRegistry`].
pub fn build_terminal_command_set(
    deps: TerminalWidgetDeps,
    widget_manager: Arc<WidgetManager>,
    workspace_manager: Arc<WorkspaceManager>,
) -> Vec<(CommandDescriptor, ActionFactory)> {
    vec![
        terminal_command(
            "terminal.split_horizontal",
            "command-terminal-split_horizontal-name",
            "terminal split horizontal",
            Shortcut::parse("Ctrl+Shift+H").ok(),
            deps.clone(),
            widget_manager.clone(),
            workspace_manager.clone(),
            TerminalOp::SplitHorizontal,
        ),
        terminal_command(
            "terminal.split_vertical",
            "command-terminal-split_vertical-name",
            "terminal split vertical",
            Shortcut::parse("Ctrl+Shift+J").ok(),
            deps.clone(),
            widget_manager.clone(),
            workspace_manager.clone(),
            TerminalOp::SplitVertical,
        ),
        terminal_command(
            "terminal.tab_new",
            "command-terminal-tab_new-name",
            "terminal tab new",
            Shortcut::parse("Ctrl+Shift+T").ok(),
            deps.clone(),
            widget_manager.clone(),
            workspace_manager.clone(),
            TerminalOp::TabNew,
        ),
        terminal_command(
            "terminal.close",
            "command-terminal-close-name",
            "terminal close",
            Shortcut::parse("Ctrl+Shift+W").ok(),
            deps.clone(),
            widget_manager.clone(),
            workspace_manager.clone(),
            TerminalOp::Close,
        ),
        terminal_command(
            "terminal.focus_next_pane",
            "command-terminal-focus_next_pane-name",
            "terminal focus next pane",
            Shortcut::parse("Ctrl+Shift+ArrowRight").ok(),
            deps.clone(),
            widget_manager.clone(),
            workspace_manager.clone(),
            TerminalOp::FocusNextPane,
        ),
        terminal_command(
            "terminal.focus_previous_pane",
            "command-terminal-focus_previous_pane-name",
            "terminal focus previous pane",
            Shortcut::parse("Ctrl+Shift+ArrowLeft").ok(),
            deps.clone(),
            widget_manager.clone(),
            workspace_manager.clone(),
            TerminalOp::FocusPreviousPane,
        ),
        terminal_command(
            "terminal.tab_next",
            "command-terminal-tab_next-name",
            "terminal tab next",
            Shortcut::parse("Ctrl+PageDown").ok(),
            deps.clone(),
            widget_manager.clone(),
            workspace_manager.clone(),
            TerminalOp::TabNext,
        ),
        terminal_command(
            "terminal.tab_previous",
            "command-terminal-tab_previous-name",
            "terminal tab previous",
            Shortcut::parse("Ctrl+PageUp").ok(),
            deps,
            widget_manager,
            workspace_manager,
            TerminalOp::TabPrevious,
        ),
    ]
}

#[derive(Clone, Copy)]
enum TerminalOp {
    SplitHorizontal,
    SplitVertical,
    TabNew,
    Close,
    FocusNextPane,
    FocusPreviousPane,
    TabNext,
    TabPrevious,
}

fn terminal_command(
    id: &'static str,
    name_key: &'static str,
    verb: &'static str,
    default_shortcut: Option<Shortcut>,
    deps: TerminalWidgetDeps,
    widget_manager: Arc<WidgetManager>,
    workspace_manager: Arc<WorkspaceManager>,
    op: TerminalOp,
) -> (CommandDescriptor, ActionFactory) {
    let descriptor = CommandDescriptor {
        id: id.into(),
        display_name_key: name_key.into(),
        description_key: None,
        category: CommandCategory::Terminal,
        default_shortcut,
        terminal_invocation: Some(TerminalInvocation {
            verb: verb.into(),
            args: Vec::new(),
        }),
        icon_name: Some("terminal".into()),
    };
    let factory: ActionFactory = Arc::new(move |args: ParsedCommand| {
        let instance = args
            .options
            .get("instance")
            .and_then(|s| Uuid::parse_str(s).ok());
        Ok(Box::new(TerminalLayoutAction {
            id,
            name_key,
            verb,
            deps: deps.clone(),
            widget_manager: widget_manager.clone(),
            workspace_manager: workspace_manager.clone(),
            op,
            instance,
        }) as Box<dyn Action>)
    });
    (descriptor, factory)
}

struct TerminalLayoutAction {
    id: &'static str,
    name_key: &'static str,
    verb: &'static str,
    deps: TerminalWidgetDeps,
    widget_manager: Arc<WidgetManager>,
    workspace_manager: Arc<WorkspaceManager>,
    op: TerminalOp,
    instance: Option<Uuid>,
}

#[async_trait]
impl Action for TerminalLayoutAction {
    fn id(&self) -> &'static str {
        self.id
    }

    fn display_name_key(&self) -> &'static str {
        self.name_key
    }

    fn command_text(&self) -> String {
        match self.instance {
            Some(inst) => format!("orc {} --instance={inst}", self.verb),
            None => format!("orc {}", self.verb),
        }
    }

    async fn execute(&self, _ctx: &ActionContext) -> orchid_core::Result<ActionOutcome> {
        let inst = match resolve_terminal_instance(
            &self.widget_manager,
            &self.workspace_manager,
            &self.deps.session_routing,
            self.instance,
        ) {
            Ok(id) => id,
            Err(msg) => return Ok(ActionOutcome::failed(msg)),
        };
        let result = match self.op {
            TerminalOp::SplitHorizontal => split_horizontal(&self.deps, inst).await,
            TerminalOp::SplitVertical => split_vertical(&self.deps, inst).await,
            TerminalOp::TabNew => add_tab(&self.deps, inst).await,
            TerminalOp::Close => close_focused_pane_or_tab(&self.deps, inst).await,
            TerminalOp::FocusNextPane => focus_next_pane(&self.deps, inst),
            TerminalOp::FocusPreviousPane => focus_previous_pane(&self.deps, inst),
            TerminalOp::TabNext => switch_tab_relative(&self.deps, inst, 1),
            TerminalOp::TabPrevious => switch_tab_relative(&self.deps, inst, -1),
        };
        match result {
            Ok(()) => Ok(ActionOutcome::ok()),
            Err(e) => Ok(ActionOutcome::failed(e.to_string())),
        }
    }
}

fn resolve_terminal_instance(
    widget_manager: &WidgetManager,
    workspace_manager: &WorkspaceManager,
    session_routing: &Arc<Mutex<HashMap<Uuid, Uuid>>>,
    explicit: Option<Uuid>,
) -> Result<Uuid, String> {
    if let Some(id) = explicit {
        return Ok(id);
    }
    let ws = workspace_manager
        .active()
        .map_err(|e| format!("no active workspace: {e}"))?;
    let instances = widget_manager.instances_for_workspace(ws.id);
    let terminals: Vec<Uuid> = instances
        .iter()
        .filter(|i| i.type_id == TERMINAL_TYPE_ID)
        .map(|i| i.id)
        .collect();
    if terminals.is_empty() {
        return Err("no terminal widget on active workspace".into());
    }
    let routing = session_routing.lock();
    terminals
        .iter()
        .find(|id| routing.contains_key(id))
        .or_else(|| terminals.first())
        .copied()
        .ok_or_else(|| "no terminal widget on active workspace".into())
}
