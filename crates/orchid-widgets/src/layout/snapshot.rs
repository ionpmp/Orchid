//! Layout snapshot types consumed by the renderer.

use uuid::Uuid;

/// Pixel-space rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelBounds {
    /// Left edge, logical pixels.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// A single placed widget with its computed pixel bounds.
#[derive(Debug, Clone)]
pub struct PlacedWidget {
    /// Widget instance id (or, for groups, the active member's id).
    pub instance_id: Uuid,
    /// Group id if the widget is part of a group.
    pub group_id: Option<Uuid>,
    /// Pixel bounds relative to the workspace content area.
    pub bounds: PixelBounds,
    /// Front-to-back ordering; larger values paint on top.
    pub z_order: u32,
}

/// Viewport in pixels at the moment the snapshot was taken.
#[derive(Debug, Clone, Copy)]
pub struct ViewportSize {
    /// Width in logical pixels.
    pub width_px: f32,
    /// Height in logical pixels.
    pub height_px: f32,
}

/// Snapshot of every widget on a workspace, ready for rendering.
#[derive(Debug, Clone)]
pub struct LayoutSnapshot {
    /// Workspace id.
    pub workspace_id: Uuid,
    /// Layout mode (Grid / Free).
    pub mode: crate::layout::LayoutMode,
    /// Placed widgets in stacking order.
    pub cells: Vec<PlacedWidget>,
    /// Number of grid columns.
    pub grid_columns: u16,
    /// Number of grid rows.
    pub grid_rows: u16,
    /// Effective cell width (includes gutter amortisation).
    pub cell_width_px: f32,
    /// Effective cell height.
    pub cell_height_px: f32,
}
