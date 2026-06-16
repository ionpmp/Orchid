//! [`orchid_widgets::Widget`] implementation that wraps a live
//! [`orchid_terminal::TerminalSession`].
//!
//! The Slint component that ultimately renders these snapshots will land in
//! a later task; for now the widget is fully functional end-to-end except
//! for the on-screen surface:
//!
//! * `on_create` spawns the underlying PTY session.
//! * `snapshot()` produces a
//!   [`orchid_widgets::WidgetPayload::Terminal`] from the current grid.
//! * `save_state` / `restore_state` round-trip a tiny
//!   [`TerminalWidgetState`] blob so the session can be respawned after an
//!   `Unloaded → Active` transition.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use orchid_core::EventBus;
use orchid_storage::{LifecycleState, StateStore, WidgetSize};
use orchid_terminal::{
    resolve_color, BackendKind, BackendSpec, CellFlags, ColorRole, LayoutRoot, PtySize, Rgba,
    SessionManager, SplitDirection, TerminalPalette,
};
use orchid_widgets::{
    widget::config, Result, TerminalDividerPayload, TerminalPayload, TerminalPayloadCell,
    TerminalPanePayload, TerminalTabPayload, Widget,
    WidgetCapabilities, WidgetContext, WidgetError, WidgetPayload, WidgetSnapshot, WidgetStatus,
};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable type id for the terminal widget.
pub const TERMINAL_TYPE_ID: &str = "terminal";

/// Persistent state carried by a terminal widget between runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalWidgetState {
    /// Which backend to relaunch on restore.
    pub backend: StoredBackend,
    /// Optional working directory hint.
    pub working_directory: Option<String>,
    /// Optional title hint.
    pub title: Option<String>,
}

/// Serializable snapshot of a [`BackendKind`] — kept narrow so v1 of the
/// widget's state format only depends on stable variants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub enum StoredBackend {
    #[cfg_attr(windows, default)]
    PowerShell,
    #[cfg_attr(not(windows), default)]
    Cmd,
    Wsl {
        distro: String,
    },
    Ssh {
        target: String,
    },
}

impl StoredBackend {
    fn to_spec(&self) -> BackendSpec {
        match self {
            Self::PowerShell => BackendSpec::powershell(),
            Self::Cmd => BackendSpec::cmd(),
            Self::Wsl { distro } => BackendSpec::wsl(distro.clone()),
            Self::Ssh { target } => {
                // `BackendSpec::ssh` accepts a parsed target; fall back to a
                // simple host-only parse here to keep restore trivial.
                let parsed = orchid_terminal::SshTarget {
                    host: target.clone(),
                    user: None,
                    port: None,
                    jump_hosts: Vec::new(),
                    identity_file: None,
                    extra_args: Vec::new(),
                };
                BackendSpec {
                    kind: BackendKind::Ssh(parsed),
                    working_directory: None,
                    env: std::collections::BTreeMap::new(),
                    initial_command: None,
                }
            }
        }
    }

    fn from_spec(spec: &BackendSpec) -> Self {
        match &spec.kind {
            BackendKind::PowerShell => Self::PowerShell,
            BackendKind::Cmd => Self::Cmd,
            BackendKind::Wsl(distro) => Self::Wsl {
                distro: distro.clone(),
            },
            BackendKind::Ssh(target) => Self::Ssh {
                target: target.host.clone(),
            },
            BackendKind::Custom { command, .. } => {
                // Custom backends fall back to the platform default on
                // restore; v1 of the state format doesn't serialise the full
                // command/args list.
                let _ = command;
                Self::default()
            }
        }
    }
}

/// Shared dependencies required to spawn a terminal widget.
#[derive(Clone)]
pub struct TerminalWidgetDeps {
    /// Session manager that actually owns the PTY.
    pub sessions: Arc<SessionManager>,
    /// Palette used to resolve cell colours into RGBA.
    pub palette: Arc<RwLock<TerminalPalette>>,
    /// Bus used for diagnostics. Currently unused by the widget itself but
    /// reserved so future widget-level events (e.g. `TerminalWidgetReady`)
    /// can be published.
    pub bus: Arc<EventBus>,
    /// Storage handle for future persistence of advanced state. Reserved;
    /// unused by v1 of this widget.
    pub storage: Arc<StateStore>,
    /// Filled in [`TerminalWidget::on_create`] and cleared on
    /// `on_close` / `on_unload` so the main window can route key events and
    /// pixel resizes to the right PTY session.
    pub session_routing: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    /// Per-widget tab layout (maps instance id → sessions + active tab).
    pub layouts: Arc<Mutex<HashMap<Uuid, LayoutRoot>>>,
}

impl std::fmt::Debug for TerminalWidgetDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalWidgetDeps").finish_non_exhaustive()
    }
}

/// Concrete `Widget` implementation for the terminal.
pub struct TerminalWidget {
    instance_id: Uuid,
    deps: TerminalWidgetDeps,
    state: TerminalWidgetState,
    /// Most recent session size. Updated on `on_resize`.
    size_cells: RwLock<(u16, u16)>,
}

impl std::fmt::Debug for TerminalWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl TerminalWidget {
    /// Construct a fresh widget. `on_create` will actually open the PTY.
    #[must_use]
    pub fn new(instance_id: Uuid, deps: TerminalWidgetDeps) -> Self {
        Self {
            instance_id,
            deps,
            state: TerminalWidgetState::default(),
            size_cells: RwLock::new(default_size_cells()),
        }
    }

    fn layout(&self) -> Option<LayoutRoot> {
        self.deps.layouts.lock().get(&self.instance_id).cloned()
    }

    fn set_routing_to_focused(&self) {
        let Some(layout) = self.layout() else {
            return;
        };
        if let Some(sid) = layout.focused_session() {
            self.deps
                .session_routing
                .lock()
                .insert(self.instance_id, sid);
        }
    }

    fn sessions_in_layout(layout: &LayoutRoot) -> Vec<Uuid> {
        let mut out = Vec::new();
        for tab in &layout.tabs.tabs {
            tab.root.leaves(&mut out);
        }
        out
    }

    async fn close_all_sessions(&self) {
        let sessions = self
            .layout()
            .map(|layout| Self::sessions_in_layout(&layout))
            .unwrap_or_default();
        self.deps.layouts.lock().remove(&self.instance_id);
        self.deps.session_routing.lock().remove(&self.instance_id);
        for sid in sessions {
            if let Err(e) = self.deps.sessions.close(sid).await {
                tracing::warn!(session_id = %sid, error = %e, "terminal close session failed");
            }
        }
    }
}

#[async_trait]
impl Widget for TerminalWidget {
    fn type_id(&self) -> &'static str {
        TERMINAL_TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> Result<()> {
        if self.deps.layouts.lock().contains_key(&self.instance_id) {
            return Ok(());
        }
        let spec = self.state.backend.to_spec();
        let (cols, rows) = *self.size_cells.read();
        let size = PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        };
        let session_id = self
            .deps
            .sessions
            .open(spec.clone(), size)
            .await
            .map_err(|e| WidgetError::CreationFailed(format!("terminal spawn failed: {e}")))?;
        self.deps
            .layouts
            .lock()
            .insert(self.instance_id, LayoutRoot::new(session_id));
        self.deps
            .session_routing
            .lock()
            .insert(self.instance_id, session_id);
        self.state.backend = StoredBackend::from_spec(&spec);
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> Result<()> {
        // No-op: the PTY keeps running while the widget is resident.
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> Result<()> {
        // Leave the PTY running in sleep; the renderer will simply stop
        // polling it. A future refinement may pause the reader task.
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> Result<()> {
        self.close_all_sessions().await;
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> Result<()> {
        self.close_all_sessions().await;
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, size: WidgetSize) -> Result<()> {
        let (cols, rows) = widget_size_to_terminal_grid(size);
        *self.size_cells.write() = (cols, rows);
        let pty_size = PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        };
        if let Some(layout) = self.layout() {
            for sid in Self::sessions_in_layout(&layout) {
                if let Ok(session) = self.deps.sessions.get(sid) {
                    let _ = session.resize(pty_size);
                }
            }
        }
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let mut layout = self.deps.layouts.lock().get(&self.instance_id)?.clone();
        sync_tab_titles(&mut layout, &self.deps.sessions);
        self.deps
            .layouts
            .lock()
            .insert(self.instance_id, layout.clone());
        let focused = layout.focused_session()?;
        let palette = self.deps.palette.read().clone();
        let snap = layout.snapshot();
        let active_tab = snap
            .tabs
            .get(snap.active_tab)
            .map(|t| t.panes.len())
            .unwrap_or(1);
        let multi_pane = active_tab > 1;
        let mut panes = Vec::new();
        let mut dividers = Vec::new();
        if let Some(tab_snap) = snap.tabs.get(snap.active_tab) {
            for pane_snap in &tab_snap.panes {
                let sid = pane_snap.session;
                if let Ok(session) = self.deps.sessions.get(sid) {
                    let grid = session.emulator.snapshot();
                    let pane_terminal = grid_to_payload(&grid, &palette);
                    panes.push(TerminalPanePayload {
                        session_id: sid.to_string(),
                        left: pane_snap.bounds.left,
                        top: pane_snap.bounds.top,
                        right: pane_snap.bounds.right,
                        bottom: pane_snap.bounds.bottom,
                        is_focused: tab_snap.focused == Some(sid),
                        show_close: multi_pane,
                        cols: pane_terminal.cols,
                        rows: pane_terminal.rows,
                        cells: pane_terminal.cells,
                        cursor_col: pane_terminal.cursor_col,
                        cursor_row: pane_terminal.cursor_row,
                        cursor_visible: pane_terminal.cursor_visible,
                    });
                }
            }
            dividers = tab_snap
                .dividers
                .iter()
                .map(|d| TerminalDividerPayload {
                    first_session_id: d.first_session.to_string(),
                    second_session_id: d.second_session.to_string(),
                    horizontal: d.direction == orchid_terminal::SplitDirection::Horizontal,
                    left: d.bounds.left,
                    top: d.bounds.top,
                    right: d.bounds.right,
                    bottom: d.bounds.bottom,
                    parent_left: d.parent_bounds.left,
                    parent_top: d.parent_bounds.top,
                    parent_right: d.parent_bounds.right,
                    parent_bottom: d.parent_bounds.bottom,
                })
                .collect();
        }
        let Ok(session) = self.deps.sessions.get(focused) else {
            return None;
        };
        let grid = session.emulator.snapshot();
        let mut terminal = grid_to_payload(&grid, &palette);
        terminal.tabs = snap
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| TerminalTabPayload {
                tab_id: t.id.to_string(),
                title: t.title.clone(),
                is_active: i == snap.active_tab,
            })
            .collect();
        terminal.active_tab = snap.active_tab as u32;
        terminal.panes = panes;
        terminal.dividers = dividers;
        let title = {
            let t = session.emulator.title();
            if t.is_empty() {
                session.spec.display_name()
            } else {
                t
            }
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TERMINAL_TYPE_ID,
            title,
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Terminal(terminal),
        })
    }

    fn save_state(&self) -> Result<Vec<u8>> {
        let focused = self.layout().and_then(|l| l.focused_session());
        let state = TerminalWidgetState {
            backend: self.state.backend.clone(),
            working_directory: focused
                .and_then(|sid| self.deps.sessions.get(sid).ok())
                .and_then(|s| {
                    s.emulator
                        .working_directory()
                        .and_then(|p| p.to_str().map(String::from))
                }),
            title: focused
                .and_then(|sid| self.deps.sessions.get(sid).ok())
                .map(|s| s.emulator.title())
                .filter(|t| !t.is_empty()),
        };
        config::save_state(&state)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> Result<()> {
        let state: TerminalWidgetState = config::restore_state(bytes)?;
        self.state = state;
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::ExtraLarge),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: false,
        }
    }
}

/// Build a [`orchid_widgets::WidgetDescriptor`] for the terminal type.
#[must_use]
pub fn terminal_descriptor(
    deps: TerminalWidgetDeps,
) -> orchid_widgets::WidgetDescriptor {
    orchid_widgets::WidgetDescriptor {
        type_id: TERMINAL_TYPE_ID,
        display_name_key: "widget-title-terminal",
        description_key: "widget-title-terminal",
        icon_name: "terminal",
        category: orchid_widgets::WidgetCategory::Developer,
        default_size: WidgetSize::ExtraLarge,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory: Arc::new(move |ctx: WidgetContext, state_bytes: Option<&[u8]>| {
            let mut widget = TerminalWidget::new(ctx.instance_id, deps.clone());
            if let Some(bytes) = state_bytes {
                widget.restore_state(bytes)?;
            }
            Ok(Box::new(widget) as Box<dyn Widget>)
        }),
    }
}

fn default_size_cells() -> (u16, u16) {
    widget_size_to_terminal_grid(WidgetSize::ExtraLarge)
}

/// Approximate PTY grid for a widget size. The widget framework speaks in
/// layout cells (2×2, 4×2, ...) which must be translated to a reasonable
/// terminal column/row count. We pick a conservative 20 cols × 10 rows per
/// layout cell — this is tuned so the default ExtraLarge (8×4) produces
/// ~160×40, which matches the current placeholder terminal view.
fn widget_size_to_terminal_grid(size: WidgetSize) -> (u16, u16) {
    let (w, h) = match size {
        WidgetSize::Small => (2, 2),
        WidgetSize::Medium => (4, 2),
        WidgetSize::Large => (4, 4),
        WidgetSize::ExtraLarge => (8, 4),
        WidgetSize::Free { w, h } => (w.max(1), h.max(1)),
    };
    (w.saturating_mul(20).max(20), h.saturating_mul(10).max(10))
}

fn grid_to_payload(
    grid: &orchid_terminal::GridSnapshot,
    palette: &TerminalPalette,
) -> TerminalPayload {
    let cols = grid.cols;
    let rows = grid.rows;
    let mut cells = Vec::with_capacity((cols as usize) * (rows as usize));
    for line in &grid.lines {
        for cell in &line.cells {
            let mut fg = resolve_color(cell.fg, palette, ColorRole::Foreground);
            let mut bg = resolve_color(cell.bg, palette, ColorRole::Background);
            if cell.flags.contains(CellFlags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags.contains(CellFlags::HIDDEN) {
                fg = bg;
            }
            cells.push(TerminalPayloadCell {
                ch: cell.ch,
                fg_rgba: rgba_to_bytes(fg),
                bg_rgba: rgba_to_bytes(bg),
                bold: cell.flags.contains(CellFlags::BOLD),
                italic: cell.flags.contains(CellFlags::ITALIC),
                underline: cell.flags.contains(CellFlags::UNDERLINE),
            });
        }
        // Pad short rows so row-major indexing remains valid.
        let missing = cols as usize - line.cells.len();
        for _ in 0..missing {
            cells.push(blank_cell(palette));
        }
    }
    while cells.len() < (cols as usize) * (rows as usize) {
        cells.push(blank_cell(palette));
    }
    TerminalPayload {
        cols,
        rows,
        cells,
        cursor_col: grid.cursor.col,
        cursor_row: grid.cursor.row,
        cursor_visible: grid.cursor.visible,
        tabs: Vec::new(),
        active_tab: 0,
        panes: Vec::new(),
        dividers: Vec::new(),
    }
}

fn sync_tab_titles(layout: &mut LayoutRoot, sessions: &SessionManager) {
    for tab in &mut layout.tabs.tabs {
        let Some(focus) = tab.focus else {
            continue;
        };
        if let Ok(session) = sessions.get(focus) {
            let t = session.emulator.title();
            tab.title = if t.is_empty() {
                session.spec.display_name()
            } else {
                t
            };
        }
    }
}

fn backend_for_layout(deps: &TerminalWidgetDeps, layout: &LayoutRoot) -> BackendSpec {
    layout
        .focused_session()
        .and_then(|sid| deps.sessions.get(sid).ok())
        .map(|s| s.spec.clone())
        .unwrap_or_else(BackendSpec::powershell)
}

/// Open a new tab in the terminal widget identified by `instance_id`.
pub async fn add_tab(deps: &TerminalWidgetDeps, instance_id: Uuid) -> Result<()> {
    let mut layouts = deps.layouts.lock();
    let Some(layout) = layouts.get_mut(&instance_id) else {
        return Err(WidgetError::InvalidStateForOperation(
            "terminal layout not found".into(),
        ));
    };
    let spec = backend_for_layout(deps, layout);
    let size = PtySize {
        cols: 80,
        rows: 24,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session_id = deps
        .sessions
        .open(spec, size)
        .await
        .map_err(|e| WidgetError::CreationFailed(format!("terminal tab spawn failed: {e}")))?;
    let idx = layout.add_tab(session_id);
    layout.active_tab = idx;
    drop(layouts);
    deps.session_routing
        .lock()
        .insert(instance_id, session_id);
    Ok(())
}

/// Split the focused pane horizontally (side-by-side).
pub async fn split_horizontal(deps: &TerminalWidgetDeps, instance_id: Uuid) -> Result<()> {
    split_pane(deps, instance_id, SplitDirection::Horizontal).await
}

/// Split the focused pane vertically (stacked).
pub async fn split_vertical(deps: &TerminalWidgetDeps, instance_id: Uuid) -> Result<()> {
    split_pane(deps, instance_id, SplitDirection::Vertical).await
}

async fn split_pane(
    deps: &TerminalWidgetDeps,
    instance_id: Uuid,
    direction: SplitDirection,
) -> Result<()> {
    let mut layouts = deps.layouts.lock();
    let Some(layout) = layouts.get_mut(&instance_id) else {
        return Err(WidgetError::InvalidStateForOperation(
            "terminal layout not found".into(),
        ));
    };
    let spec = backend_for_layout(deps, layout);
    let size = PtySize {
        cols: 80,
        rows: 24,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session_id = deps
        .sessions
        .open(spec, size)
        .await
        .map_err(|e| WidgetError::CreationFailed(format!("terminal split spawn failed: {e}")))?;
    layout
        .split(direction, session_id)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("split: {e}")))?;
    drop(layouts);
    deps.session_routing
        .lock()
        .insert(instance_id, session_id);
    Ok(())
}

/// Focus a pane by session id within the active tab.
pub fn focus_pane(deps: &TerminalWidgetDeps, instance_id: Uuid, session_id: Uuid) -> Result<()> {
    let mut layouts = deps.layouts.lock();
    let Some(layout) = layouts.get_mut(&instance_id) else {
        return Err(WidgetError::InvalidStateForOperation(
            "terminal layout not found".into(),
        ));
    };
    let tab = layout
        .tabs
        .tabs
        .get_mut(layout.active_tab)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("no active tab".into()))?;
    let mut leaves = Vec::new();
    tab.root.leaves(&mut leaves);
    if !leaves.iter().any(|s| *s == session_id) {
        return Err(WidgetError::InvalidStateForOperation(
            "session not in active tab".into(),
        ));
    }
    tab.focus = Some(session_id);
    drop(layouts);
    deps.session_routing.lock().insert(instance_id, session_id);
    Ok(())
}

/// Close a pane and its backing session (merges splits when needed).
pub async fn close_pane(
    deps: &TerminalWidgetDeps,
    instance_id: Uuid,
    session_id: Uuid,
) -> Result<()> {
    let closed = {
        let mut layouts = deps.layouts.lock();
        let Some(layout) = layouts.get_mut(&instance_id) else {
            return Err(WidgetError::InvalidStateForOperation(
                "terminal layout not found".into(),
            ));
        };
        let tab = layout
            .tabs
            .tabs
            .get_mut(layout.active_tab)
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no active tab".into()))?;
        if tab.root.leaf_count() <= 1 {
            return Err(WidgetError::InvalidStateForOperation(
                "cannot close last pane in tab".into(),
            ));
        }
        tab.focus = Some(session_id);
        layout
            .close_focus()
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("close pane: {e}")))?;
        session_id
    };
    if let Err(e) = deps.sessions.close(closed).await {
        tracing::warn!(session_id = %closed, error = %e, "terminal close pane session failed");
    }
    if let Some(sid) = deps
        .layouts
        .lock()
        .get(&instance_id)
        .and_then(|l| l.focused_session())
    {
        deps.session_routing.lock().insert(instance_id, sid);
    } else {
        deps.session_routing.lock().remove(&instance_id);
    }
    Ok(())
}

/// Set the split ratio between two adjacent panes in the active tab.
pub fn set_split_ratio(
    deps: &TerminalWidgetDeps,
    instance_id: Uuid,
    first_session: Uuid,
    second_session: Uuid,
    ratio: f32,
) -> Result<()> {
    let mut layouts = deps.layouts.lock();
    let Some(layout) = layouts.get_mut(&instance_id) else {
        return Err(WidgetError::InvalidStateForOperation(
            "terminal layout not found".into(),
        ));
    };
    layout
        .set_split_ratio(first_session, second_session, ratio)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("set split ratio: {e}")))?;
    Ok(())
}

/// Switch the active tab in a terminal widget.
pub fn switch_tab(deps: &TerminalWidgetDeps, instance_id: Uuid, tab_index: usize) -> Result<()> {
    let mut layouts = deps.layouts.lock();
    let Some(layout) = layouts.get_mut(&instance_id) else {
        return Err(WidgetError::InvalidStateForOperation(
            "terminal layout not found".into(),
        ));
    };
    if tab_index >= layout.tabs.tabs.len() {
        return Err(WidgetError::InvalidStateForOperation(
            "tab index out of range".into(),
        ));
    }
    layout.active_tab = tab_index;
    let sid = layout
        .focused_session()
        .ok_or_else(|| WidgetError::InvalidStateForOperation("no focused session".into()))?;
    drop(layouts);
    deps.session_routing.lock().insert(instance_id, sid);
    Ok(())
}

/// Close a tab and its backing session.
pub async fn close_tab(
    deps: &TerminalWidgetDeps,
    instance_id: Uuid,
    tab_index: usize,
) -> Result<()> {
    let closed_sessions = {
        let mut layouts = deps.layouts.lock();
        let Some(layout) = layouts.get_mut(&instance_id) else {
            return Err(WidgetError::InvalidStateForOperation(
                "terminal layout not found".into(),
            ));
        };
        let tab = layout
            .tabs
            .tabs
            .get(tab_index)
            .ok_or_else(|| WidgetError::InvalidStateForOperation("tab index out of range".into()))?;
        let mut sessions = Vec::new();
        tab.root.leaves(&mut sessions);
        layout.close_tab(tab_index).map_err(|e| {
            WidgetError::InvalidStateForOperation(format!("close tab: {e}"))
        })?;
        sessions
    };
    for sid in closed_sessions {
        if let Err(e) = deps.sessions.close(sid).await {
            tracing::warn!(session_id = %sid, error = %e, "terminal close tab session failed");
        }
    }
    if let Some(sid) = deps.layouts.lock().get(&instance_id).and_then(|l| l.focused_session()) {
        deps.session_routing.lock().insert(instance_id, sid);
    } else {
        deps.session_routing.lock().remove(&instance_id);
    }
    Ok(())
}

fn rgba_to_bytes(rgba: Rgba) -> [u8; 4] {
    [rgba.r, rgba.g, rgba.b, rgba.a]
}

fn blank_cell(palette: &TerminalPalette) -> TerminalPayloadCell {
    TerminalPayloadCell {
        ch: ' ',
        fg_rgba: rgba_to_bytes(palette.default_fg),
        bg_rgba: rgba_to_bytes(palette.default_bg),
        bold: false,
        italic: false,
        underline: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_size_mapping_scales_monotonically() {
        let small = widget_size_to_terminal_grid(WidgetSize::Small);
        let xl = widget_size_to_terminal_grid(WidgetSize::ExtraLarge);
        assert!(xl.0 > small.0);
        assert!(xl.1 > small.1);
    }

    #[test]
    fn stored_backend_defaults_are_platform_reasonable() {
        let default = StoredBackend::default();
        match default {
            StoredBackend::PowerShell | StoredBackend::Cmd => {}
            other => panic!("unexpected default backend {other:?}"),
        }
    }

    #[test]
    fn state_bincode_roundtrip() {
        let s = TerminalWidgetState {
            backend: StoredBackend::PowerShell,
            working_directory: Some("C:/".into()),
            title: Some("pwsh".into()),
        };
        let bytes = config::save_state(&s).unwrap();
        let back: TerminalWidgetState = config::restore_state(&bytes).unwrap();
        assert!(matches!(back.backend, StoredBackend::PowerShell));
        assert_eq!(back.working_directory.as_deref(), Some("C:/"));
    }
}
