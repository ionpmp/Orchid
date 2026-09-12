//! Paragraph layout via `parley` + software rasterisation via `swash`.

#![allow(
    clippy::too_many_arguments,
    clippy::large_enum_variant,
    clippy::needless_range_loop
)]

use std::collections::HashMap;
use std::sync::Arc;

use parley::{FontContext, Layout, LayoutContext};
use swash::scale::ScaleContext;

use crate::document::model::{PageSetup, Paragraph};

mod flow;
mod paint;
mod tables;

pub use paint::render_to_rgba;
use paint::twips_to_css_px;
use tables::LaidBlock;

/// Brush colour for styled runs (RGBA).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorBrush {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha.
    pub a: u8,
    /// When true, paint a yellow highlight behind the glyph run.
    pub highlight: bool,
    /// Extra baseline shift in CSS px (negative raises for superscript).
    pub baseline_shift: f32,
    /// Soft drop-shadow for `w:shadow` runs.
    pub shadow: bool,
    /// Second strike line for `w:dstrike`.
    pub double_strike: bool,
    /// Raised dual-offset for `w:emboss`.
    pub emboss: bool,
    /// Sunken dual-offset for `w:imprint`.
    pub imprint: bool,
}

impl Default for ColorBrush {
    fn default() -> Self {
        Self {
            r: 32,
            g: 32,
            b: 32,
            a: 255,
            highlight: false,
            baseline_shift: 0.0,
            shadow: false,
            double_strike: false,
            emboss: false,
            imprint: false,
        }
    }
}

/// Word-style yellow highlight fill.
pub(super) const HIGHLIGHT_YELLOW: [u8; 4] = [255, 255, 0, 255];

/// Default content width for the document preview (CSS pixels).
pub const DEFAULT_PREVIEW_WIDTH: f32 = 720.0;
/// Extra left inset per list indent level (`w:ilvl`).
pub const LIST_INDENT_PX: f32 = 24.0;
/// Cap rendered page height so huge docs stay interactive.
pub const MAX_PREVIEW_HEIGHT: u32 = 4096;
/// Device-pixel ratio for soft-rendered preview (sharper on HiDPI).
pub const PREVIEW_RENDER_SCALE: f32 = 2.0;

/// CSS-pixel page margins for the preview canvas (from `w:pgMar`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewInsets {
    /// Left margin.
    pub left: f32,
    /// Right margin.
    pub right: f32,
    /// Top margin.
    pub top: f32,
    /// Bottom margin.
    pub bottom: f32,
}

impl PreviewInsets {
    /// Convert [`PageSetup`] twip margins to CSS pixels (96 dpi).
    #[must_use]
    pub fn from_page_setup(ps: &PageSetup) -> Self {
        Self {
            left: twips_to_css_px(ps.margin_left_twips),
            right: twips_to_css_px(ps.margin_right_twips),
            top: twips_to_css_px(ps.margin_top_twips),
            bottom: twips_to_css_px(ps.margin_bottom_twips),
        }
    }

    /// Insets for the default US-Letter 1″ margins.
    #[must_use]
    pub fn default_letter() -> Self {
        Self::from_page_setup(&PageSetup::default())
    }
}

/// Owns parley + swash contexts for laying out and rasterising paragraphs.
pub struct DocumentLayout {
    font_cx: FontContext,
    layout_cx: LayoutContext<ColorBrush>,
    scale_cx: ScaleContext,
    layout_cache: LayoutCache,
    /// Last full raster without caret / selection, reused on selection-only paints.
    scene: Option<RenderScene>,
    /// Open document file name for `FILENAME` field preview.
    field_file_name: Option<String>,
}

impl std::fmt::Debug for DocumentLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentLayout").finish_non_exhaustive()
    }
}

impl Default for DocumentLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache of laid-out paragraphs keyed by block index.
#[derive(Debug, Default)]
pub struct LayoutCache {
    cache: HashMap<usize, CachedLayout>,
    /// Width the cache was built for.
    width: f32,
    /// Parley display scale the cache was built for.
    scale: f32,
}
/// Raster + geometry from the last content-width pass (no caret / selection).
pub(super) struct RenderScene {
    content_width: f32,
    layouts: Vec<LaidBlock>,
    insets: PreviewInsets,
    max_w: f32,
    width: u32,
    height: u32,
    base: Arc<Vec<u8>>,
}
#[derive(Debug)]
pub(super) struct CachedLayout {
    layout: Layout<ColorBrush>,
}

impl LayoutCache {
    /// Empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or compute layout for paragraph `idx`.
    pub fn get_or_layout(
        &mut self,
        idx: usize,
        p: &Paragraph,
        dl: &mut DocumentLayout,
        width: f32,
        scale: f32,
    ) -> &Layout<ColorBrush> {
        if (self.width - width).abs() > 0.5 || (self.scale - scale).abs() > 0.05 {
            self.cache.clear();
            self.width = width;
            self.scale = scale;
        }
        self.cache.entry(idx).or_insert_with(|| CachedLayout {
            layout: dl.layout_paragraph(p, width, scale),
        });
        &self.cache.get(&idx).expect("just inserted").layout
    }

    pub(super) fn prepare(&mut self, width: f32, scale: f32) {
        if (self.width - width).abs() > 0.5 || (self.scale - scale).abs() > 0.05 {
            self.cache.clear();
            self.width = width;
            self.scale = scale;
        }
    }

    pub(super) fn get_clone(&self, idx: usize) -> Option<Layout<ColorBrush>> {
        self.cache.get(&idx).map(|c| c.layout.clone())
    }

    pub(super) fn insert(&mut self, idx: usize, layout: Layout<ColorBrush>) {
        self.cache.insert(idx, CachedLayout { layout });
    }

    /// Invalidate one paragraph.
    pub fn invalidate(&mut self, idx: usize) {
        self.cache.remove(&idx);
    }

    /// Drop the entire cache.
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }

    /// Whether `idx` is cached (tests).
    #[must_use]
    pub fn contains(&self, idx: usize) -> bool {
        self.cache.contains_key(&idx)
    }
}

#[cfg(test)]
mod tests;
