//! Terminal widget Slint model builders.

use slint::{Color, ModelRc, VecModel};

use crate::slint_generated::{
    TerminalCellModel, TerminalDividerModel, TerminalPaneModel, TerminalTabModel,
};
use orchid_widgets::{TerminalPanePayload, TerminalPayload};

/// Empty cell model — `TerminalView` paints via `pixels` only; the Slint
/// `cells` property is unused for drawing. Avoid allocating cols×rows models.
pub(crate) fn blank_terminal(_cols: u16, _rows: u16) -> ModelRc<ModelRc<TerminalCellModel>> {
    empty_terminal_cells()
}

/// Shared empty `[[TerminalCellModel]]` for frame/pane fields that still exist
/// in the Slint schema but are not bound for painting.
pub(crate) fn empty_terminal_cells() -> ModelRc<ModelRc<TerminalCellModel>> {
    ModelRc::new(VecModel::<ModelRc<TerminalCellModel>>::default())
}

fn char_to_cell(ch: char) -> TerminalCellModel {
    TerminalCellModel {
        ch: ch.to_string().into(),
        fg: Color::from_argb_u8(0xFF, 0xE6, 0xEB, 0xF0),
        bg: Color::from_argb_u8(0xFF, 0x12, 0x14, 0x18),
        bold: false,
    }
}

#[allow(dead_code)] // Kept for debugging / future cell-based paint paths.
pub(crate) fn build_terminal_model(t: &TerminalPayload) -> ModelRc<ModelRc<TerminalCellModel>> {
    let mut rows = Vec::with_capacity(t.rows as usize);
    for r in 0..t.rows {
        let mut rowv = Vec::with_capacity(t.cols as usize);
        for c in 0..t.cols {
            let idx = (r as usize) * (t.cols as usize) + (c as usize);
            let cell = t.cells.get(idx).map_or_else(
                || char_to_cell(' '),
                |cell| TerminalCellModel {
                    ch: if cell.ch == '\0' {
                        " ".into()
                    } else {
                        cell.ch.to_string().into()
                    },
                    fg: Color::from_argb_u8(
                        cell.fg_rgba[3],
                        cell.fg_rgba[0],
                        cell.fg_rgba[1],
                        cell.fg_rgba[2],
                    ),
                    bg: Color::from_argb_u8(
                        cell.bg_rgba[3],
                        cell.bg_rgba[0],
                        cell.bg_rgba[1],
                        cell.bg_rgba[2],
                    ),
                    bold: cell.bold,
                },
            );
            rowv.push(cell);
        }
        rows.push(ModelRc::new(VecModel::from(rowv)));
    }
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn build_terminal_tab_models(t: &TerminalPayload) -> (ModelRc<TerminalTabModel>, i32) {
    let tabs: Vec<TerminalTabModel> = t
        .tabs
        .iter()
        .map(|tab| TerminalTabModel {
            tab_id: tab.tab_id.clone().into(),
            title: tab.title.clone().into(),
            is_active: tab.is_active,
        })
        .collect();
    (ModelRc::new(VecModel::from(tabs)), t.active_tab as i32)
}

pub(crate) fn default_terminal_tab_models() -> (ModelRc<TerminalTabModel>, i32) {
    (ModelRc::new(VecModel::default()), 0)
}

pub(crate) fn default_terminal_pane_models() -> ModelRc<TerminalPaneModel> {
    ModelRc::new(VecModel::default())
}

pub(crate) fn build_terminal_divider_models(t: &TerminalPayload) -> ModelRc<TerminalDividerModel> {
    let dividers: Vec<TerminalDividerModel> = t
        .dividers
        .iter()
        .map(|d| TerminalDividerModel {
            first_session_id: d.first_session_id.clone().into(),
            second_session_id: d.second_session_id.clone().into(),
            horizontal: d.horizontal,
            left: d.left,
            top: d.top,
            right: d.right,
            bottom: d.bottom,
            parent_left: d.parent_left,
            parent_top: d.parent_top,
            parent_right: d.parent_right,
            parent_bottom: d.parent_bottom,
        })
        .collect();
    ModelRc::new(VecModel::from(dividers))
}

pub(crate) fn default_terminal_divider_models() -> ModelRc<TerminalDividerModel> {
    ModelRc::new(VecModel::default())
}

/// Build a root [`TerminalPayload`] view of a pane without cloning cell data when
/// the caller only needs metadata (rasterization should use `&p.cells` directly).
#[allow(dead_code)] // retained for debugging / future cell-based paint paths
pub(crate) fn pane_payload_to_terminal(p: &TerminalPanePayload) -> TerminalPayload {
    TerminalPayload {
        cols: p.cols,
        rows: p.rows,
        cells: p.cells.clone(),
        cursor_col: p.cursor_col,
        cursor_row: p.cursor_row,
        cursor_visible: p.cursor_visible,
        content_generation: p.content_generation,
        dirty_lines: p.dirty_lines.clone(),
        full_redraw: p.full_redraw,
        tabs: Vec::new(),
        active_tab: 0,
        panes: Vec::new(),
        dividers: Vec::new(),
    }
}
