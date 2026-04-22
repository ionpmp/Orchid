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
    /// Logical cells per column.
    pub grid_rows: u16,
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
        let cell_w = viewport.width_px / opts.grid_columns as f32;
        let cell_h = viewport.height_px / opts.grid_rows as f32;
        let gutter = opts.gutter_px;
        let mut cells = Vec::with_capacity(instances.len());
        for (idx, inst) in instances
            .iter()
            .filter(|i| i.workspace_id == workspace_id)
            .enumerate()
        {
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
        LayoutSnapshot {
            workspace_id,
            mode: opts.mode,
            cells,
            grid_columns: opts.grid_columns,
            grid_rows: opts.grid_rows,
            cell_width_px: cell_w,
            cell_height_px: cell_h,
        }
    }
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
    instances
        .iter()
        .filter(|i| i.workspace_id == workspace_id)
        .map(|i| CellRect::from_widget(*i.position.read(), *i.size.read()))
        .collect()
}

#[cfg(test)]
mod tests {
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
