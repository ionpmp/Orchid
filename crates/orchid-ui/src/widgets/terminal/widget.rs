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

use std::sync::Arc;

use async_trait::async_trait;
use orchid_core::EventBus;
use orchid_storage::{LifecycleState, StateStore, WidgetSize};
use orchid_terminal::{
    resolve_color, BackendKind, BackendSpec, CellFlags, ColorRole, PtySize, Rgba,
    SessionManager, TerminalPalette,
};
use orchid_widgets::{
    widget::config, Result, TerminalPayload, TerminalPayloadCell, Widget, WidgetCapabilities,
    WidgetContext, WidgetError, WidgetPayload, WidgetSnapshot, WidgetStatus,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
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
    /// Live session id once `on_create` runs. `OnceCell` keeps the widget
    /// trivially `Send`/`Sync` without adding a second mutex.
    session_id: OnceCell<Uuid>,
    /// Most recent session size. Updated on `on_resize`.
    size_cells: RwLock<(u16, u16)>,
}

impl std::fmt::Debug for TerminalWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalWidget")
            .field("instance_id", &self.instance_id)
            .field("session_id", &self.session_id.get())
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
            session_id: OnceCell::new(),
            size_cells: RwLock::new(default_size_cells()),
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
        if self.session_id.get().is_some() {
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
        self.session_id
            .set(session_id)
            .map_err(|_| WidgetError::InvalidStateForOperation("session_id already set".into()))?;
        // Record the chosen backend so save_state reflects what is really
        // running.
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
        // Close the underlying session to free memory. `on_create` will
        // respawn on the way back to Active.
        if let Some(sid) = self.session_id.take() {
            if let Err(e) = self.deps.sessions.close(sid).await {
                tracing::warn!(session_id = %sid, error = %e, "terminal on_unload close failed");
            }
        }
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> Result<()> {
        if let Some(sid) = self.session_id.take() {
            if let Err(e) = self.deps.sessions.close(sid).await {
                tracing::warn!(session_id = %sid, error = %e, "terminal on_close failed");
            }
        }
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, size: WidgetSize) -> Result<()> {
        let (cols, rows) = widget_size_to_terminal_grid(size);
        *self.size_cells.write() = (cols, rows);
        if let Some(sid) = self.session_id.get().copied() {
            if let Ok(session) = self.deps.sessions.get(sid) {
                let pty_size = PtySize {
                    cols,
                    rows,
                    pixel_width: 0,
                    pixel_height: 0,
                };
                let _ = session.resize(pty_size);
            }
        }
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let sid = self.session_id.get().copied()?;
        let session = self.deps.sessions.get(sid).ok()?;
        let grid = session.emulator.snapshot();
        let palette = self.deps.palette.read().clone();
        let terminal = grid_to_payload(&grid, &palette);
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
        let state = TerminalWidgetState {
            backend: self.state.backend.clone(),
            working_directory: self
                .session_id
                .get()
                .copied()
                .and_then(|sid| self.deps.sessions.get(sid).ok())
                .and_then(|s| {
                    s.emulator
                        .working_directory()
                        .and_then(|p| p.to_str().map(String::from))
                }),
            title: self
                .session_id
                .get()
                .copied()
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
        display_name_key: "widget-terminal-name",
        description_key: "widget-terminal-desc",
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
    }
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
