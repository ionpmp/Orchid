//! Monospace cell size in **logical pixels** for the Slint grid and the PTY `FontMetrics`,
//! derived from a real system font (same path as a serious terminal emulator) instead of
//! multiplying `size_md` by a magic factor.

use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use fontdue::Font;
use fontdue::FontSettings;
use orchid_terminal::FontMetrics;
use tracing::debug;

use crate::theme::TypographyTokens;

const SAMPLE_CHARS: &[char] = &['M', '0', 'm', 'W', ' '];

/// Parse a CSS-style `font-family` list: `"A, B, C"` → `["A","B","C"]`.
fn parse_family_list(spec: &str) -> Vec<String> {
    spec.split(',')
        .map(str::trim)
        .map(|s| s.trim_matches('\'').trim_matches('\"').trim().to_string())
        .filter(|s| !s.is_empty() && s != "monospace" && s != "ui-monospace")
        .chain(std::iter::once("monospace".to_string()))
        .collect()
}

fn load_font_by_family(db: &Database, name: &str) -> Option<Font> {
    let id = db.query(&Query {
        families: &[Family::Name(name)],
        weight: Weight::NORMAL,
        style: Style::Normal,
        stretch: Stretch::Normal,
    })?;
    db.with_face_data(id, |data, index| {
        let settings = FontSettings {
            collection_index: index,
            ..Default::default()
        };
        Font::from_bytes(data, settings).ok()
    })
    .flatten()
}

fn any_mono_font(db: &Database) -> Option<Font> {
    let id = db.query(&Query {
        families: &[Family::Monospace],
        weight: Weight::NORMAL,
        style: Style::Normal,
        stretch: Stretch::Normal,
    })?;
    db.with_face_data(id, |data, index| {
        let settings = FontSettings {
            collection_index: index,
            ..Default::default()
        };
        Font::from_bytes(data, settings).ok()
    })
    .flatten()
}

/// A proportional / symbol-rich system face used only when the monospace
/// `font.rasterize(ch)` returns an empty outline (Nerd, box-drawing, many symbols, CJK, emoji).
const GLYPH_RASTER_FALLBACK_FAMILIES: &[&str] = &[
    "Segoe UI Symbol",
    "Segoe UI",
    "Noto Sans Symbols 2",
    "Noto Sans Symbols",
    "Microsoft YaHei",
    "Noto Sans",
    "Liberation Sans",
    "DejaVu Sans",
    "Apple Symbols",
];

fn load_glyph_raster_fallback(db: &Database) -> Option<Font> {
    for &name in GLYPH_RASTER_FALLBACK_FAMILIES {
        if let Some(f) = load_font_by_family(db, name) {
            debug!(
                target: "orchid_ui::font_metrics",
                family = name,
                "loaded raster glyph fallback (wider coverage than primary mono)"
            );
            return Some(f);
        }
    }
    None
}

/// Return PTY/Slint cell size, the loaded `fontdue` face for the monospace grid, and an optional
/// second face for rastering glyphs the primary does not cover (empty bitmap).
pub fn font_and_metrics_from_typography(
    tokens: &TypographyTokens,
) -> (FontMetrics, Option<Font>, Option<Font>) {
    let size_px = tokens.size_md;
    if !size_px.is_finite() || size_px <= 0.0 {
        return (heuristic_font_metrics(tokens), None, None);
    }
    static DB: std::sync::OnceLock<Database> = std::sync::OnceLock::new();
    let db = DB.get_or_init(|| {
        let mut d = Database::new();
        d.load_system_fonts();
        d
    });

    let names = parse_family_list(&tokens.font_family_mono);
    let mut font: Option<Font> = None;
    for n in &names {
        if let Some(f) = load_font_by_family(db, n) {
            debug!(target: "orchid_ui::font_metrics", family = n.as_str(), "measured from named face");
            font = Some(f);
            break;
        }
    }
    if font.is_none() {
        if let Some(f) = any_mono_font(db) {
            debug!(target: "orchid_ui::font_metrics", "measured from generic Monospace");
            font = Some(f);
        }
    }
    let Some(font) = font else {
        return (heuristic_font_metrics(tokens), None, None);
    };

    let Some(lm) = font.horizontal_line_metrics(size_px) else {
        return (heuristic_font_metrics(tokens), None, None);
    };
    let ch = lm.new_line_size;
    if !ch.is_finite() || ch < 1.0 {
        return (heuristic_font_metrics(tokens), None, None);
    }

    let mut cw: f32 = 0.0;
    for &c in SAMPLE_CHARS {
        let m = font.metrics(c, size_px);
        cw = cw.max(m.advance_width);
    }
    if !cw.is_finite() || cw < 1.0 {
        return (heuristic_font_metrics(tokens), None, None);
    }

    let glyph_raster_fallback = load_glyph_raster_fallback(db);
    let metrics = FontMetrics {
        cell_width_px: cw.max(4.0).round(),
        cell_height_px: ch.max(8.0).round(),
    };
    (metrics, Some(font), glyph_raster_fallback)
}

fn heuristic_font_metrics(tokens: &TypographyTokens) -> FontMetrics {
    let cell_height_px = (tokens.size_md * 1.4).round().max(8.0);
    let cell_width_px = (cell_height_px * 0.5).round().max(4.0);
    debug!(
        target: "orchid_ui::font_metrics",
        w = cell_width_px,
        h = cell_height_px,
        "heuristic fallback (no system mono match)"
    );
    FontMetrics {
        cell_width_px,
        cell_height_px,
    }
}

