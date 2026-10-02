//! Selects the built-in grid or the optional Alacritty grid.

use std::path::PathBuf;
use std::sync::Arc;

use uuid::Uuid;

#[cfg(feature = "alacritty-grid")]
use crate::emulator::alacritty_grid::AlacrittyGrid;
use crate::emulator::{GridSnapshot, TerminalEmulator, DEFAULT_SCROLLBACK};
use crate::error::Result;

/// Which VT grid a new session should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridKind {
    /// Orchid's built-in grid. Sixel, Kitty, OSC 7, and OSC 52 live here.
    #[default]
    Orchid,
    /// `alacritty_terminal`, when the crate is built with `alacritty-grid`.
    Alacritty,
}

impl GridKind {
    /// `alacritty` selects [`GridKind::Alacritty`]. Every other string stays
    /// on the built-in grid.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        if value.eq_ignore_ascii_case("alacritty") {
            Self::Alacritty
        } else {
            Self::Orchid
        }
    }
}

/// Grid behind a live session. Methods are inherent so callers do not import
/// a trait.
#[derive(Debug)]
pub enum TerminalGrid {
    /// Built-in emulator.
    Orchid(TerminalEmulator),
    /// Optional Alacritty grid.
    #[cfg(feature = "alacritty-grid")]
    Alacritty(AlacrittyGrid),
}

impl TerminalGrid {
    /// Build the grid selected by `kind`.
    ///
    /// Without the `alacritty-grid` feature, [`GridKind::Alacritty`] is the
    /// built-in emulator and a warning is logged.
    #[must_use]
    pub fn new(
        kind: GridKind,
        cols: u16,
        rows: u16,
        bus: Arc<orchid_core::EventBus>,
        session_id: Uuid,
    ) -> Self {
        match kind {
            GridKind::Orchid => Self::orchid(cols, rows, bus, session_id),
            GridKind::Alacritty => {
                #[cfg(feature = "alacritty-grid")]
                {
                    Self::Alacritty(AlacrittyGrid::new(cols, rows))
                }
                #[cfg(not(feature = "alacritty-grid"))]
                {
                    tracing::warn!(
                        "terminal grid alacritty was requested but this build has no alacritty-grid feature; using the built-in grid"
                    );
                    Self::orchid(cols, rows, bus, session_id)
                }
            }
        }
    }

    fn orchid(cols: u16, rows: u16, bus: Arc<orchid_core::EventBus>, session_id: Uuid) -> Self {
        Self::Orchid(TerminalEmulator::new(
            cols,
            rows,
            DEFAULT_SCROLLBACK,
            bus,
            session_id,
        ))
    }

    /// Feed PTY bytes. Response bytes (DA, DSR, and similar) go back to the PTY.
    pub fn feed(&self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Self::Orchid(emulator) => emulator.feed(bytes),
            #[cfg(feature = "alacritty-grid")]
            Self::Alacritty(grid) => grid.feed(bytes),
        }
    }

    /// Resize the visible grid.
    ///
    /// # Errors
    ///
    /// The built-in grid rejects a zero size. The Alacritty grid clamps it.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        match self {
            Self::Orchid(emulator) => emulator.resize(cols, rows),
            #[cfg(feature = "alacritty-grid")]
            Self::Alacritty(grid) => grid.resize(cols, rows),
        }
    }

    /// Pixel size of one cell. The Alacritty grid stores it for text-area
    /// size replies. Inline images stay on the built-in grid.
    pub fn set_cell_px(&self, width: u16, height: u16) {
        match self {
            Self::Orchid(emulator) => emulator.set_cell_px(width, height),
            #[cfg(feature = "alacritty-grid")]
            Self::Alacritty(grid) => grid.set_cell_px(width, height),
        }
    }

    /// Visible cells, cursor, and scrollback counts.
    #[must_use]
    pub fn snapshot(&self) -> GridSnapshot {
        match self {
            Self::Orchid(emulator) => emulator.snapshot(),
            #[cfg(feature = "alacritty-grid")]
            Self::Alacritty(grid) => grid.snapshot(),
        }
    }

    /// OSC window title. Empty when the shell has not set one.
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Self::Orchid(emulator) => emulator.title(),
            #[cfg(feature = "alacritty-grid")]
            Self::Alacritty(grid) => grid.title(),
        }
    }

    /// OSC 7 directory. The Alacritty grid does not report one.
    #[must_use]
    pub fn working_directory(&self) -> Option<PathBuf> {
        match self {
            Self::Orchid(emulator) => emulator.working_directory(),
            #[cfg(feature = "alacritty-grid")]
            Self::Alacritty(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus() -> Arc<orchid_core::EventBus> {
        Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ))
    }

    #[test]
    fn grid_name_parses_only_alacritty() {
        assert_eq!(GridKind::parse("alacritty"), GridKind::Alacritty);
        assert_eq!(GridKind::parse("ALACRITTY"), GridKind::Alacritty);
        assert_eq!(GridKind::parse("orchid"), GridKind::Orchid);
        assert_eq!(GridKind::parse("other"), GridKind::Orchid);
    }

    #[test]
    fn selected_grid_paints_plain_text() {
        let grid = TerminalGrid::new(GridKind::Alacritty, 20, 6, bus(), Uuid::nil());
        let _ = grid.feed(b"hi");
        let snap = grid.snapshot();
        let text: String = snap.lines[0].cells.iter().take(2).map(|c| c.ch).collect();
        assert_eq!(text, "hi");
        assert_eq!(snap.cursor.col, 2);
        assert_eq!(snap.cursor.row, 0);
    }
}
