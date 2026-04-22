//! Convert an emulator [`orchid_terminal::GridSnapshot`] into a renderer-agnostic
//! representation. The Slint-specific adapter layer is built on top of this.

use orchid_terminal::{
    resolve_color, CellFlags, ColorRole, GridSnapshot, Rgba, TerminalPalette,
};

/// Simplified cell tailored to what the Slint view expects. Keeping this
/// type in a pure-Rust module lets us unit-test the conversion without a
/// Slint runtime.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderCell {
    /// Character in the cell.
    pub ch: char,
    /// Foreground RGBA.
    pub fg: Rgba,
    /// Background RGBA.
    pub bg: Rgba,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
}

/// Convert a snapshot into a `Vec<Vec<RenderCell>>`, applying the palette.
///
/// INVERSE is honoured by swapping fg / bg; HIDDEN paints the cell with its
/// bg colour in both slots to match xterm behaviour.
#[must_use]
pub fn snapshot_to_cells(
    snapshot: &GridSnapshot,
    palette: &TerminalPalette,
) -> Vec<Vec<RenderCell>> {
    snapshot
        .lines
        .iter()
        .map(|line| {
            line.cells
                .iter()
                .map(|cell| {
                    let mut fg = resolve_color(cell.fg, palette, ColorRole::Foreground);
                    let mut bg = resolve_color(cell.bg, palette, ColorRole::Background);
                    if cell.flags.contains(CellFlags::INVERSE) {
                        std::mem::swap(&mut fg, &mut bg);
                    }
                    if cell.flags.contains(CellFlags::HIDDEN) {
                        fg = bg;
                    }
                    // Dim → pull foreground toward background by 30 %.
                    if cell.flags.contains(CellFlags::DIM) {
                        fg = blend(fg, bg, 0.3);
                    }
                    RenderCell {
                        ch: cell.ch,
                        fg,
                        bg,
                        bold: cell.flags.contains(CellFlags::BOLD),
                        italic: cell.flags.contains(CellFlags::ITALIC),
                        underline: cell.flags.contains(CellFlags::UNDERLINE),
                    }
                })
                .collect()
        })
        .collect()
}

fn blend(a: Rgba, b: Rgba, t: f32) -> Rgba {
    fn mix(x: u8, y: u8, t: f32) -> u8 {
        ((x as f32) * (1.0 - t) + (y as f32) * t) as u8
    }
    Rgba {
        r: mix(a.r, b.r, t),
        g: mix(a.g, b.g, t),
        b: mix(a.b, b.b, t),
        a: mix(a.a, b.a, t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_terminal::{Cell, CellColor, GridLine, TerminalPalette};

    fn palette() -> TerminalPalette {
        TerminalPalette::default_dark()
    }

    fn snap(cells: Vec<Vec<Cell>>) -> GridSnapshot {
        let rows = cells.len() as u16;
        let cols = cells.first().map(|r| r.len() as u16).unwrap_or(0);
        let lines = cells
            .into_iter()
            .enumerate()
            .map(|(i, row)| GridLine {
                line_number: i as i64,
                cells: row,
            })
            .collect();
        GridSnapshot {
            cols,
            rows,
            scrollback_offset: 0,
            scrollback_total: 0,
            lines,
            cursor: orchid_terminal::CursorState::default(),
        }
    }

    #[test]
    fn two_by_two_grid_converts_cleanly() {
        let snapshot = snap(vec![
            vec![
                Cell {
                    ch: 'A',
                    fg: CellColor::Indexed(1),
                    bg: CellColor::Default,
                    flags: CellFlags::BOLD,
                },
                Cell {
                    ch: 'B',
                    fg: CellColor::Default,
                    bg: CellColor::Default,
                    flags: CellFlags::empty(),
                },
            ],
            vec![
                Cell {
                    ch: 'C',
                    fg: CellColor::Default,
                    bg: CellColor::Default,
                    flags: CellFlags::INVERSE,
                },
                Cell {
                    ch: 'D',
                    fg: CellColor::Default,
                    bg: CellColor::Default,
                    flags: CellFlags::UNDERLINE,
                },
            ],
        ]);
        let rendered = snapshot_to_cells(&snapshot, &palette());
        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0].len(), 2);
        // BOLD propagates.
        assert!(rendered[0][0].bold);
        // INVERSE swaps fg/bg.
        let p = palette();
        assert_eq!(rendered[1][0].fg, p.default_bg);
        assert_eq!(rendered[1][0].bg, p.default_fg);
        assert!(rendered[1][1].underline);
    }

    #[test]
    fn hidden_flag_matches_fg_to_bg() {
        let snapshot = snap(vec![vec![Cell {
            ch: '!',
            fg: CellColor::Indexed(1),
            bg: CellColor::Default,
            flags: CellFlags::HIDDEN,
        }]]);
        let rendered = snapshot_to_cells(&snapshot, &palette());
        assert_eq!(rendered[0][0].fg, rendered[0][0].bg);
    }
}
