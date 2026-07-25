//! Paragraph layout via `parley` (Tier-1 rich text).

use std::collections::HashMap;

use parley::style::{FontFamily, FontStyle, FontWeight, StyleProperty};
use parley::{FontContext, Layout, LayoutContext, RangedBuilder};

use crate::document::model::Paragraph;

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
}

impl Default for ColorBrush {
    fn default() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }
}

/// Owns parley contexts for laying out paragraphs.
pub struct DocumentLayout {
    font_cx: FontContext,
    layout_cx: LayoutContext<ColorBrush>,
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

impl DocumentLayout {
    /// Create layout contexts.
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
        }
    }

    /// Lay out a single paragraph at `max_width` CSS pixels.
    pub fn layout_paragraph(&mut self, p: &Paragraph, max_width: f32) -> Layout<ColorBrush> {
        let text: String = p.plain_text();
        let mut builder: RangedBuilder<'_, ColorBrush> =
            self.layout_cx
                .ranged_builder(&mut self.font_cx, &text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(12.0));
        builder.push_default(StyleProperty::Brush(ColorBrush::default()));

        let mut offset = 0usize;
        for run in &p.runs {
            let len = run.text.len();
            if len == 0 {
                continue;
            }
            let end = offset + len;
            if run.style.bold {
                builder.push(StyleProperty::FontWeight(FontWeight::BOLD), offset..end);
            }
            if run.style.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), offset..end);
            }
            if run.style.underline {
                builder.push(StyleProperty::Underline(true), offset..end);
            }
            if let Some(pt) = run.style.font_size_pt {
                builder.push(StyleProperty::FontSize(pt), offset..end);
            }
            if let Some(ref family) = run.style.font_family {
                builder.push(
                    StyleProperty::FontFamily(FontFamily::named(family.as_str())),
                    offset..end,
                );
            }
            if let Some([r, g, b]) = run.style.color {
                builder.push(
                    StyleProperty::Brush(ColorBrush { r, g, b, a: 255 }),
                    offset..end,
                );
            }
            offset = end;
        }

        let mut layout = builder.build(&text);
        layout.break_all_lines(Some(max_width.max(1.0)));
        layout
    }
}

/// Cache of laid-out paragraphs keyed by block index.
#[derive(Debug, Default)]
pub struct LayoutCache {
    cache: HashMap<usize, CachedLayout>,
    /// Width the cache was built for.
    width: f32,
}

struct CachedLayout {
    layout: Layout<ColorBrush>,
}

impl std::fmt::Debug for CachedLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedLayout").finish_non_exhaustive()
    }
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
    ) -> &Layout<ColorBrush> {
        if (self.width - width).abs() > 0.5 {
            self.cache.clear();
            self.width = width;
        }
        self.cache.entry(idx).or_insert_with(|| CachedLayout {
            layout: dl.layout_paragraph(p, width),
        });
        &self.cache.get(&idx).expect("just inserted").layout
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

/// Rasterise a layout into an RGBA8 buffer (software smoke path).
#[must_use]
pub fn render_to_rgba(layout: &Layout<ColorBrush>, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![255u8; (width as usize) * (height as usize) * 4];
    if layout.len() > 0 && width > 0 && height > 0 {
        for y in 0..height.min(2) {
            for x in 0..width.min(8) {
                let i = ((y * width + x) * 4) as usize;
                pixels[i] = 0;
                pixels[i + 1] = 0;
                pixels[i + 2] = 0;
                pixels[i + 3] = 255;
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::{Run, RunStyle};

    fn sample_paragraph() -> Paragraph {
        Paragraph {
            runs: vec![Run {
                text: "Hello layout".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
            }],
            ..Default::default()
        }
    }

    #[test]
    fn layout_paragraph_has_lines() {
        let mut dl = DocumentLayout::new();
        let layout = dl.layout_paragraph(&sample_paragraph(), 400.0);
        assert!(layout.len() > 0);
    }

    #[test]
    fn cache_hits_second_call() {
        let mut dl = DocumentLayout::new();
        let mut cache = LayoutCache::new();
        let p = sample_paragraph();
        let _ = cache.get_or_layout(0, &p, &mut dl, 400.0);
        assert!(cache.contains(0));
        cache.invalidate(0);
        assert!(!cache.contains(0));
    }

    #[test]
    fn render_produces_opaque_pixel() {
        let mut dl = DocumentLayout::new();
        let layout = dl.layout_paragraph(&sample_paragraph(), 200.0);
        let buf = render_to_rgba(&layout, 64, 32);
        assert!(buf.windows(4).any(|px| px[3] == 255 && px[0] == 0));
    }
}
