//! Workspace layout engine.

pub mod collision;
pub mod free;
pub mod grid;
pub mod snapshot;

use dashmap::DashMap;
use orchid_storage::{GridPosition, WidgetSize};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, WidgetError};
use crate::layout::collision::{overlaps, CellRect};
use crate::layout::grid::{fits_in_grid, size_in_cells, snap_position};
use crate::widget::instance::SharedInstance;

pub use snapshot::{LayoutSnapshot, PixelBounds, PlacedWidget, ViewportSize};

/// Grid-placed vs free-floating layout.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutMode {
    #[default]
    Grid,
    Free,
}

/// Tunables that apply to every workspace managed by a [`LayoutEngine`].
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    /// Grid or free-floating placement.
    pub mode: LayoutMode,
    /// Logical cells per row.
    pub grid_columns: u16,
    /// Logical cells per column (may exceed [`Self::view_rows`] to allow vertical scrolling).
    pub grid_rows: u16,
    /// Divides the viewport height to compute **cell height in pixels**; independent of
    /// [`Self::grid_rows`] so adding rows grows the canvas downward without shrinking cells.
    pub view_rows: u16,
    /// Snap threshold in cells (reserved for future drag-snap logic).
    pub snap_threshold_cells: u16,
    /// Gap between adjacent widgets, in logical pixels.
    pub gutter_px: f32,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Grid,
            grid_columns: 16,
            grid_rows: 10,
            view_rows: 10,
            snap_threshold_cells: 1,
            gutter_px: 8.0,
        }
    }
}

/// Per-workspace cached state. Not used for much yet — reserved for drag /
/// hover previews in the UI task.
#[derive(Debug, Default)]
struct WorkspaceLayoutState {
    // Intentionally empty; the engine operates statelessly for now.
}

/// Placement engine.
#[derive(Default)]
pub struct LayoutEngine {
    options: parking_lot::RwLock<LayoutOptions>,
    #[allow(dead_code)]
    inner: DashMap<Uuid, WorkspaceLayoutState>,
}

impl std::fmt::Debug for LayoutEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayoutEngine")
            .field("options", &*self.options.read())
            .finish_non_exhaustive()
    }
}

impl LayoutEngine {
    /// Build an engine with explicit options.
    #[must_use]
    pub fn new(options: LayoutOptions) -> Self {
        Self {
            options: parking_lot::RwLock::new(options),
            inner: DashMap::new(),
        }
    }

    /// Replace the current options.
    pub fn set_options(&self, options: LayoutOptions) {
        *self.options.write() = options;
    }

    /// Snapshot of the current options.
    #[must_use]
    pub fn options(&self) -> LayoutOptions {
        self.options.read().clone()
    }

    /// Ensures [`LayoutOptions::grid_columns`] / [`LayoutOptions::grid_rows`] are at least large
    /// enough to contain every instance on `workspace_id` (e.g. after restore from storage when
    /// the grid was previously grown).
    pub fn grow_grid_to_fit_instances(&self, workspace_id: Uuid, instances: &[SharedInstance]) {
        const MAX_GRID_ROWS: u16 = 512;
        const MAX_GRID_COLS: u16 = 128;
        let mut opts = self.options.read().clone();
        let mut max_col_end: u16 = 0;
        let mut max_row_end: u16 = 0;
        for inst in instances.iter().filter(|i| i.workspace_id == workspace_id) {
            let pos = *inst.position.read();
            let size = *inst.size.read();
            let (w, h) = size_in_cells(size);
            max_col_end = max_col_end.max(pos.col.saturating_add(w));
            max_row_end = max_row_end.max(pos.row.saturating_add(h));
        }
        let mut changed = false;
        if max_col_end > opts.grid_columns {
            opts.grid_columns = max_col_end.min(MAX_GRID_COLS);
            changed = true;
        }
        if max_row_end > opts.grid_rows {
            opts.grid_rows = max_row_end.min(MAX_GRID_ROWS);
            changed = true;
        }
        if changed {
            self.set_options(opts);
        }
    }

    /// Grow the logical grid so `position` + `size` fits (e.g. after a free-form drag).
    pub fn grow_grid_to_fit_placement(&self, position: GridPosition, size: WidgetSize) {
        const MAX_GRID_ROWS: u16 = 512;
        const MAX_GRID_COLS: u16 = 128;
        let (w, h) = size_in_cells(size);
        let mut opts = self.options.read().clone();
        let mut changed = false;
        let col_end = position.col.saturating_add(w);
        let row_end = position.row.saturating_add(h);
        if col_end > opts.grid_columns {
            opts.grid_columns = col_end.min(MAX_GRID_COLS);
            changed = true;
        }
        if row_end > opts.grid_rows {
            opts.grid_rows = row_end.min(MAX_GRID_ROWS);
            changed = true;
        }
        if changed {
            self.set_options(opts);
        }
    }

    /// Auto-place a widget of `size` on `workspace_id`.
    ///
    /// # Errors
    ///
    /// * [`WidgetError::Layout`] when no empty slot fits.
    pub fn auto_place(
        &self,
        workspace_id: Uuid,
        size: WidgetSize,
        existing: &[SharedInstance],
    ) -> Result<GridPosition> {
        let opts = self.options.read().clone();
        if !fits_in_grid(GridPosition { col: 0, row: 0 }, size, opts.grid_columns, opts.grid_rows) {
            return Err(WidgetError::Layout(format!(
                "widget of size {size:?} does not fit in {}x{} grid",
                opts.grid_columns, opts.grid_rows
            )));
        }
        let occupied = rects_for(existing, workspace_id);
        match opts.mode {
            LayoutMode::Grid => grid_first_fit(size, &opts, &occupied).ok_or_else(|| {
                WidgetError::Layout(format!(
                    "no free slot on workspace {workspace_id} for size {size:?}"
                ))
            }),
            LayoutMode::Free => free::spiral_place(size, opts.grid_columns, opts.grid_rows, &occupied)
                .ok_or_else(|| {
                    WidgetError::Layout(format!(
                        "free-layout spiral placement failed for size {size:?}"
                    ))
                }),
        }
    }

    /// Like [`LayoutEngine::auto_place`], but **excludes** `ignore_instance` from the occupied set.
    /// Use when a new widget was created at a placeholder position (e.g. `(0,0)`) and must be moved
    /// to the first free cell without colliding with **other** widgets or with a duplicate of itself
    /// at the wrong coordinates.
    pub fn auto_place_excluding(
        &self,
        workspace_id: Uuid,
        size: WidgetSize,
        existing: &[SharedInstance],
        ignore_instance: Uuid,
    ) -> Result<GridPosition> {
        let opts = self.options.read().clone();
        if !fits_in_grid(GridPosition { col: 0, row: 0 }, size, opts.grid_columns, opts.grid_rows) {
            return Err(WidgetError::Layout(format!(
                "widget of size {size:?} does not fit in {}x{} grid",
                opts.grid_columns, opts.grid_rows
            )));
        }
        let occupied = rects_for_excluding(existing, workspace_id, Some(ignore_instance));
        match opts.mode {
            LayoutMode::Grid => grid_first_fit(size, &opts, &occupied).ok_or_else(|| {
                WidgetError::Layout(format!(
                    "no free slot on workspace {workspace_id} for size {size:?}"
                ))
            }),
            LayoutMode::Free => {
                free::spiral_place(size, opts.grid_columns, opts.grid_rows, &occupied).ok_or_else(
                    || {
                        WidgetError::Layout(format!(
                            "free-layout spiral placement failed for size {size:?}"
                        ))
                    },
                )
            }
        }
    }

    /// Like [`Self::auto_place_excluding`], but grows [`LayoutOptions::grid_rows`] (space at the
    /// bottom) until a slot appears, and widens [`LayoutOptions::grid_columns`] if the widget does
    /// not fit horizontally. Returns the original error if limits are reached.
    pub fn auto_place_excluding_with_growth(
        &self,
        workspace_id: Uuid,
        size: WidgetSize,
        existing: &[SharedInstance],
        ignore_instance: Uuid,
    ) -> Result<GridPosition> {
        const MAX_GRID_ROWS: u16 = 512;
        const MAX_GRID_COLS: u16 = 128;
        const MAX_ITERS: u32 = 1024;
        let mut iters = 0u32;
        loop {
            iters += 1;
            if iters > MAX_ITERS {
                return Err(WidgetError::Layout(
                    "auto_place: exceeded growth iteration cap".into(),
                ));
            }
            match self.auto_place_excluding(workspace_id, size, existing, ignore_instance) {
                Ok(p) => return Ok(p),
                Err(e) => {
                    let mut opts = self.options();
                    let (wc, hc) = size_in_cells(size);
                    let mut grew = false;
                    if wc > opts.grid_columns && opts.grid_columns < MAX_GRID_COLS {
                        opts.grid_columns = wc.min(MAX_GRID_COLS);
                        grew = true;
                    }
                    if hc > opts.grid_rows && opts.grid_rows < MAX_GRID_ROWS {
                        opts.grid_rows = hc.min(MAX_GRID_ROWS);
                        grew = true;
                    }
                    if !grew && opts.grid_rows < MAX_GRID_ROWS {
                        opts.grid_rows = opts.grid_rows.saturating_add(1);
                        grew = true;
                    }
                    if !grew {
                        return Err(e);
                    }
                    opts.grid_columns = opts.grid_columns.min(MAX_GRID_COLS);
                    opts.grid_rows = opts.grid_rows.min(MAX_GRID_ROWS);
                    self.set_options(opts);
                }
            }
        }
    }

    /// Validate a placement request against existing widgets.
    ///
    /// # Errors
    ///
    /// * [`WidgetError::InvalidPosition`] when the rect falls outside the grid.
    /// * [`WidgetError::PositionCollision`] when it overlaps another widget.
    pub fn can_place(
        &self,
        _workspace_id: Uuid,
        instance_id: Uuid,
        position: GridPosition,
        size: WidgetSize,
        existing: &[SharedInstance],
    ) -> Result<()> {
        let opts = self.options.read().clone();
        if !fits_in_grid(position, size, opts.grid_columns, opts.grid_rows) {
            return Err(WidgetError::InvalidPosition {
                col: position.col,
                row: position.row,
            });
        }
        let rect = CellRect::from_widget(position, size);
        for inst in existing {
            if inst.id == instance_id {
                continue;
            }
            let other = CellRect::from_widget(*inst.position.read(), *inst.size.read());
            if overlaps(rect, other) {
                return Err(WidgetError::PositionCollision(inst.id));
            }
        }
        Ok(())
    }

    /// Snap `position` to the grid when the engine is in [`LayoutMode::Grid`];
    /// leave it alone in [`LayoutMode::Free`].
    #[must_use]
    pub fn snap(&self, position: GridPosition, size: WidgetSize) -> (GridPosition, WidgetSize) {
        let opts = self.options.read().clone();
        match opts.mode {
            LayoutMode::Grid => (
                snap_position(position, size, opts.grid_columns, opts.grid_rows),
                size,
            ),
            LayoutMode::Free => (position, size),
        }
    }

    /// Materialise a [`LayoutSnapshot`] for the UI.
    #[must_use]
    pub fn snapshot(
        &self,
        workspace_id: Uuid,
        instances: &[SharedInstance],
        viewport: ViewportSize,
    ) -> LayoutSnapshot {
        let opts = self.options.read().clone();
        let view_rows = opts.view_rows.max(1);
        let cell_w = viewport.width_px / f32::from(opts.grid_columns);
        let cell_h = viewport.height_px / f32::from(view_rows);
        let gutter = opts.gutter_px;
        let mut content_width_px = viewport.width_px;
        let mut content_height_px = f32::from(opts.grid_rows) * cell_h;
        let mut insts: Vec<SharedInstance> = instances
            .iter()
            .filter(|i| i.workspace_id == workspace_id)
            .cloned()
            .collect();
        // Stable paint order (avoids `for` item reuse glitches); bottom-right paints on top.
        insts.sort_by(|a, b| {
            let pa = *a.position.read();
            let pb = *b.position.read();
            pa.row
                .cmp(&pb.row)
                .then_with(|| pa.col.cmp(&pb.col))
                .then_with(|| a.id.cmp(&b.id))
        });
        let mut cells = Vec::with_capacity(insts.len());
        for (idx, inst) in insts.into_iter().enumerate() {
            let position = *inst.position.read();
            let size = *inst.size.read();
            let (w_cells, h_cells) = size_in_cells(size);
            let x = (position.col as f32) * cell_w + gutter * 0.5;
            let y = (position.row as f32) * cell_h + gutter * 0.5;
            let width = (w_cells as f32) * cell_w - gutter;
            let height = (h_cells as f32) * cell_h - gutter;
            let group_id = *inst.group_id.read();
            cells.push(PlacedWidget {
                instance_id: inst.id,
                group_id,
                bounds: PixelBounds {
                    x,
                    y,
                    width: width.max(0.0),
                    height: height.max(0.0),
                },
                z_order: idx as u32,
            });
        }
        for c in &cells {
            let bottom = c.bounds.y + c.bounds.height;
            let right = c.bounds.x + c.bounds.width;
            content_height_px = content_height_px.max(bottom);
            content_width_px = content_width_px.max(right);
        }
        LayoutSnapshot {
            workspace_id,
            mode: opts.mode,
            cells,
            grid_columns: opts.grid_columns,
            grid_rows: opts.grid_rows,
            cell_width_px: cell_w,
            cell_height_px: cell_h,
            content_width_px,
            content_height_px,
        }
    }
}

/// Converts a widget top-left in canvas pixel space to a [`GridPosition`], inverting
/// the mapping used in [`LayoutEngine::snapshot`]:
/// `x = col * cell_w + gutter/2`, `y = row * cell_h + gutter/2`, then applies
/// [`grid::snap_position`] so the top-left is valid for `size` (e.g. 8×4
/// `ExtraLarge` is clamped to `col <= grid_columns - 8`, not `grid_columns - 1`).
#[must_use]
pub fn position_from_content_top_left(
    viewport: ViewportSize,
    opts: &LayoutOptions,
    top_left_x: f32,
    top_left_y: f32,
    size: WidgetSize,
) -> GridPosition {
    let view_rows = opts.view_rows.max(1);
    let cell_w = viewport.width_px / f32::from(opts.grid_columns);
    let cell_h = viewport.height_px / f32::from(view_rows);
    let g = opts.gutter_px;
    let col_f = (top_left_x - g * 0.5) / cell_w;
    let row_f = (top_left_y - g * 0.5) / cell_h;
    let col = (col_f.round() as i64).clamp(0, i64::from(u16::MAX)) as u16;
    let row = (row_f.round() as i64).clamp(0, i64::from(u16::MAX)) as u16;
    snap_position(
        GridPosition { col, row },
        size,
        opts.grid_columns,
        opts.grid_rows,
    )
}

/// Resolves live pixel `bounds` (as produced by [`LayoutEngine::snapshot`]) into a
/// `Free` grid size and a valid top-left [`GridPosition`].
///
/// The stride `cell_w = viewport.width / grid_columns` matches [`LayoutEngine::snapshot`]
/// (not a «pure inner cell» in px); span width/height in px follow
/// `W = w_cells * cell_w - gutter`, so the inverse is `w_cells = (W + gutter) / cell_w`.
/// Top-left is derived via [`position_from_content_top_left`] for that `WidgetSize::Free`
/// so multi-cell [`snap_position`] rules apply, unlike raw float→`u16` casts.
#[must_use]
pub fn free_placement_from_pixel_bounds(
    bounds: &PixelBounds,
    viewport: ViewportSize,
    opts: &LayoutOptions,
) -> (GridPosition, WidgetSize) {
    let g = opts.gutter_px;
    let view_rows = opts.view_rows.max(1);
    let cell_w = viewport.width_px / f32::from(opts.grid_columns);
    let cell_h = viewport.height_px / f32::from(view_rows);
    // Inverse of snapshot: width = w*cell_w - g, height = h*cell_h - g
    let w = (bounds.width + g) / cell_w;
    let h = (bounds.height + g) / cell_h;
    let w = w
        .round()
        .max(1.0)
        .min(f32::from(opts.grid_columns)) as u16;
    let h = h
        .round()
        .max(1.0)
        .min(f32::from(opts.grid_rows)) as u16;
    let size = WidgetSize::Free { w, h };
    let pos = position_from_content_top_left(
        viewport,
        opts,
        bounds.x,
        bounds.y,
        size,
    );
    (pos, size)
}

fn grid_first_fit(
    size: WidgetSize,
    opts: &LayoutOptions,
    occupied: &[CellRect],
) -> Option<GridPosition> {
    let (w, h) = size_in_cells(size);
    for row in 0..=opts.grid_rows.saturating_sub(h) {
        for col in 0..=opts.grid_columns.saturating_sub(w) {
            let pos = GridPosition { col, row };
            let rect = CellRect::from_widget(pos, size);
            if !occupied.iter().any(|other| overlaps(rect, *other)) {
                return Some(pos);
            }
        }
    }
    None
}

fn rects_for(instances: &[SharedInstance], workspace_id: Uuid) -> Vec<CellRect> {
    rects_for_excluding(instances, workspace_id, None)
}

fn rects_for_excluding(
    instances: &[SharedInstance],
    workspace_id: Uuid,
    ignore: Option<Uuid>,
) -> Vec<CellRect> {
    instances
        .iter()
        .filter(|i| {
            i.workspace_id == workspace_id && ignore.is_none_or(|e| i.id != e)
        })
        .map(|i| CellRect::from_widget(*i.position.read(), *i.size.read()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::grid::{fits_in_grid, size_in_cells};
    use super::*;
    use std::sync::Arc;

    fn fake_instance(workspace: Uuid, position: GridPosition, size: WidgetSize) -> SharedInstance {
        use chrono::Utc;
        use parking_lot::RwLock;
        use tokio::sync::Mutex;
        struct Stub;
        #[async_trait::async_trait]
        impl crate::widget::Widget for Stub {
            fn type_id(&self) -> &'static str {
                "stub"
            }
            fn instance_id(&self) -> Uuid {
                Uuid::nil()
            }
            async fn on_create(&mut self, _: &crate::WidgetContext) -> crate::Result<()> {
                Ok(())
            }
            async fn on_activate(&mut self, _: &crate::WidgetContext) -> crate::Result<()> {
                Ok(())
            }
            async fn on_sleep(&mut self, _: &crate::WidgetContext) -> crate::Result<()> {
                Ok(())
            }
            async fn on_unload(&mut self, _: &crate::WidgetContext) -> crate::Result<()> {
                Ok(())
            }
            async fn on_close(&mut self, _: &crate::WidgetContext) -> crate::Result<()> {
                Ok(())
            }
            async fn on_resize(
                &mut self,
                _: &crate::WidgetContext,
                _: WidgetSize,
            ) -> crate::Result<()> {
                Ok(())
            }
            fn snapshot(&self) -> Option<crate::WidgetSnapshot> {
                None
            }
            fn save_state(&self) -> crate::Result<Vec<u8>> {
                Ok(Vec::new())
            }
            fn restore_state(&mut self, _: &[u8]) -> crate::Result<()> {
                Ok(())
            }
        }
        let now = Utc::now();
        Arc::new(crate::WidgetInstanceRuntime {
            id: Uuid::new_v4(),
            workspace_id: workspace,
            type_id: "stub".into(),
            position: RwLock::new(position),
            size: RwLock::new(size),
            lifecycle: RwLock::new(orchid_storage::LifecycleState::Active),
            group_id: RwLock::new(None),
            created_at: now,
            updated_at: RwLock::new(now),
            widget: Mutex::new(Box::new(Stub)),
            last_snapshot: RwLock::new(None),
            last_touched: RwLock::new(now),
        })
    }

    #[test]
    fn auto_place_excluding_ignores_new_instance_at_origin() {
        let engine = LayoutEngine::new(LayoutOptions::default());
        let ws = Uuid::new_v4();
        let a = fake_instance(ws, GridPosition { col: 0, row: 0 }, WidgetSize::Small);
        let b = fake_instance(ws, GridPosition { col: 0, row: 0 }, WidgetSize::Small);
        let c = fake_instance(ws, GridPosition { col: 0, row: 0 }, WidgetSize::Small);
        let c_id = c.id;
        let all = [a, b, c];
        let pos = engine
            .auto_place_excluding(ws, WidgetSize::Small, &all, c_id)
            .expect("free slot for third 2x2 when ignoring c's placeholder");
        assert_ne!(
            (pos.col, pos.row),
            (0, 0),
            "with a and b at (0,0), c should not stay on the same cell"
        );
    }

    #[test]
    fn auto_place_first_fit_in_grid_mode() {
        let engine = LayoutEngine::new(LayoutOptions::default());
        let ws = Uuid::new_v4();
        let pos = engine.auto_place(ws, WidgetSize::Small, &[]).unwrap();
        assert_eq!(pos, GridPosition { col: 0, row: 0 });

        // One at origin; the next auto-placed widget should land to the right.
        let first = fake_instance(ws, pos, WidgetSize::Small);
        let second = engine
            .auto_place(ws, WidgetSize::Small, &[Arc::clone(&first)])
            .unwrap();
        assert!(second.col >= 2 || second.row > 0);
    }

    #[test]
    fn can_place_rejects_overlap_but_allows_self_move() {
        let engine = LayoutEngine::new(LayoutOptions::default());
        let ws = Uuid::new_v4();
        let inst = fake_instance(ws, GridPosition { col: 0, row: 0 }, WidgetSize::Small);
        let id = inst.id;
        let others = vec![Arc::clone(&inst)];

        // A different widget trying to overlap fails.
        let err = engine
            .can_place(ws, Uuid::new_v4(), GridPosition { col: 0, row: 0 }, WidgetSize::Small, &others)
            .unwrap_err();
        assert!(matches!(err, WidgetError::PositionCollision(_)));

        // The same widget "moving" in place succeeds.
        engine
            .can_place(ws, id, GridPosition { col: 0, row: 0 }, WidgetSize::Small, &others)
            .unwrap();

        // Moving somewhere free succeeds.
        engine
            .can_place(ws, id, GridPosition { col: 5, row: 5 }, WidgetSize::Small, &others)
            .unwrap();
    }

    #[test]
    fn snapshot_pixel_bounds_for_small_widget_at_origin() {
        // 16x10 grid, 1600x1000 viewport → 100x100 cells. A 2x2 Small widget
        // at (0,0) occupies (0,0)..(200,200) minus the 8 px gutter.
        let engine = LayoutEngine::new(LayoutOptions::default());
        let ws = Uuid::new_v4();
        let inst = fake_instance(ws, GridPosition { col: 0, row: 0 }, WidgetSize::Small);
        let snap = engine.snapshot(
            ws,
            &[Arc::clone(&inst)],
            ViewportSize {
                width_px: 1600.0,
                height_px: 1000.0,
            },
        );
        let placed = &snap.cells[0];
        assert!((placed.bounds.x - 4.0).abs() < 0.1);
        assert!((placed.bounds.y - 4.0).abs() < 0.1);
        assert!((placed.bounds.width - 192.0).abs() < 0.1);
        assert!((placed.bounds.height - 192.0).abs() < 0.1);
        assert!((snap.content_width_px - 1600.0).abs() < 0.1);
        assert!((snap.content_height_px - 1000.0).abs() < 0.1);
    }

    #[test]
    fn grow_grid_to_fit_placement_extends_rows() {
        let engine = LayoutEngine::new(LayoutOptions::default());
        let pos = GridPosition { col: 0, row: 18 };
        engine.grow_grid_to_fit_placement(pos, WidgetSize::Small);
        assert!(engine.options().grid_rows >= 20);
    }

    #[test]
    fn snapshot_content_grows_with_extra_rows() {
        let opts = LayoutOptions {
            grid_rows: 20,
            view_rows: 10,
            ..LayoutOptions::default()
        };
        let engine = LayoutEngine::new(opts);
        let ws = Uuid::new_v4();
        let snap = engine.snapshot(ws, &[], ViewportSize {
            width_px: 1600.0,
            height_px: 1000.0,
        });
        assert!((snap.cell_height_px - 100.0).abs() < 0.1);
        assert!((snap.content_height_px - 2000.0).abs() < 0.1);
    }

    #[test]
    fn position_from_content_top_left_inverts_snapshot_for_exlarge() {
        let opts = LayoutOptions {
            grid_columns: 16,
            grid_rows: 10,
            gutter_px: 8.0,
            ..LayoutOptions::default()
        };
        let viewport = ViewportSize {
            width_px: 1600.0,
            height_px: 1000.0,
        };
        // ExtraLarge: top-left (8,0) => last col 16 for base row (fits 16x10).
        let pos = GridPosition { col: 8, row: 0 };
        let (w, h) = size_in_cells(WidgetSize::ExtraLarge);
        assert_eq!((w, h), (8, 4));
        let cell_w = viewport.width_px / f32::from(opts.grid_columns);
        let cell_h = viewport.height_px / f32::from(opts.view_rows.max(1));
        let g = opts.gutter_px;
        let x = pos.col as f32 * cell_w + g * 0.5;
        let y = pos.row as f32 * cell_h + g * 0.5;
        let back = position_from_content_top_left(
            viewport,
            &opts,
            x,
            y,
            WidgetSize::ExtraLarge,
        );
        assert_eq!(back, pos);

        // Example: vw=1280, 16 cols, g=8 → cell_w=80, 4 cell span: 4*80-8=312, inverse (312+8)/80=4
        {
            let vw = 1280.0_f32;
            let cell = vw / f32::from(opts.grid_columns);
            let four = 4.0_f32;
            let wpx = four * cell - g;
            assert!((wpx - 312.0).abs() < 0.01);
            let wcells = ((wpx + g) / cell).round() as u16;
            assert_eq!(wcells, 4);
        }

        // Old bug: anchoring to max single-cell index (15) is invalid for 8 wide.
        let invalid_anchor = (15, 0);
        let p = GridPosition {
            col: invalid_anchor.0,
            row: invalid_anchor.1,
        };
        let ok = fits_in_grid(p, WidgetSize::ExtraLarge, opts.grid_columns, opts.grid_rows);
        assert!(!ok, "8-wide widget cannot start at col 15 in a 16-col grid");
    }

    #[test]
    fn free_placement_inverts_snapshot_free_bounds() {
        let opts = LayoutOptions {
            grid_columns: 16,
            grid_rows: 10,
            gutter_px: 8.0,
            ..LayoutOptions::default()
        };
        let viewport = ViewportSize {
            width_px: 1280.0,
            height_px: 1000.0,
        };
        let g = opts.gutter_px;
        let cell_w = viewport.width_px / f32::from(opts.grid_columns);
        let cell_h = viewport.height_px / f32::from(opts.view_rows.max(1));
        let wcells: u16 = 4;
        let hcells: u16 = 3;
        let pb = PixelBounds {
            x: g * 0.5,
            y: g * 0.5,
            width: wcells as f32 * cell_w - g,
            height: hcells as f32 * cell_h - g,
        };
        let (p, s) = free_placement_from_pixel_bounds(&pb, viewport, &opts);
        assert_eq!(p, GridPosition { col: 0, row: 0 });
        assert_eq!(s, WidgetSize::Free { w: 4, h: 3 });
    }

    #[test]
    fn grow_grid_to_fit_instances_expands_for_widgets_below_default_rows() {
        let engine = LayoutEngine::new(LayoutOptions::default());
        let ws = Uuid::new_v4();
        let inst = fake_instance(ws, GridPosition { col: 0, row: 15 }, WidgetSize::Small);
        engine.grow_grid_to_fit_instances(ws, &[inst]);
        assert!(engine.options().grid_rows >= 17);
    }

    #[test]
    fn auto_place_excluding_with_growth_extends_grid_rows() {
        let opts = LayoutOptions {
            grid_columns: 4,
            grid_rows: 2,
            view_rows: 2,
            ..LayoutOptions::default()
        };
        let engine = LayoutEngine::new(opts);
        let ws = Uuid::new_v4();
        let mut all: Vec<SharedInstance> = Vec::new();
        for col in [0u16, 2u16] {
            for row in [0u16, 1u16] {
                all.push(fake_instance(
                    ws,
                    GridPosition { col, row },
                    WidgetSize::Small,
                ));
            }
        }
        let extra = fake_instance(ws, GridPosition { col: 0, row: 0 }, WidgetSize::Small);
        let extra_id = extra.id;
        all.push(extra);
        let pos = engine
            .auto_place_excluding_with_growth(ws, WidgetSize::Small, &all, extra_id)
            .expect("fifth 2×2 fits after growing rows");
        assert!(engine.options().grid_rows > 2);
        assert!(pos.row >= 2, "new slot should be on an added row: {pos:?}");
    }

    #[test]
    fn invalid_position_rejected() {
        let engine = LayoutEngine::new(LayoutOptions::default());
        let ws = Uuid::new_v4();
        let err = engine
            .can_place(
                ws,
                Uuid::new_v4(),
                GridPosition { col: 20, row: 20 },
                WidgetSize::Small,
                &[],
            )
            .unwrap_err();
        assert!(matches!(err, WidgetError::InvalidPosition { .. }));
    }
}
