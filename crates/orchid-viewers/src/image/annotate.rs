//! Draw, text, privacy, and watermarks. Results are saved as a sibling file.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

use crate::error::{Result, ViewerError};
use crate::image::edit::save_sibling;
use crate::image::loader::{load_image_file, LoadedImage};
use crate::image::metadata::inspect_image_file;

use std::path::{Path, PathBuf};

use parley::layout::PositionedLayoutItem;
use parley::style::{FontFamily, StyleProperty};
use parley::{FontContext, LayoutContext};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::FontRef;

/// One annotation, watermark, or a stack.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotateOp {
    /// Straight stroke; `arrow` adds a head at the end.
    Line {
        /// Start.
        x0: f32,
        /// Start.
        y0: f32,
        /// End.
        x1: f32,
        /// End.
        y1: f32,
        /// Stroke.
        style: DrawStyle,
        /// Draw an arrow head.
        arrow: bool,
    },
    /// Axis-aligned box.
    Rect {
        /// Left.
        x: f32,
        /// Top.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// Stroke / fill.
        style: DrawStyle,
    },
    /// Axis-aligned oval.
    Ellipse {
        /// Left.
        x: f32,
        /// Top.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// Stroke / fill.
        style: DrawStyle,
    },
    /// Closed polyline.
    Polygon {
        /// Vertices.
        points: Vec<(f32, f32)>,
        /// Stroke / fill.
        style: DrawStyle,
    },
    /// Freehand stroke.
    Pen {
        /// Sampled points.
        points: Vec<(f32, f32)>,
        /// Stroke.
        style: DrawStyle,
    },
    /// Text overlay.
    Text {
        /// Anchor X.
        x: f32,
        /// Anchor Y (top of the first line).
        y: f32,
        /// UTF-8.
        text: String,
        /// Font / size / color.
        style: DrawStyle,
    },
    /// Rounded bubble + pointer + text.
    Callout {
        /// Box left.
        x: f32,
        /// Box top.
        y: f32,
        /// Box width.
        w: f32,
        /// Box height.
        h: f32,
        /// Caption.
        text: String,
        /// Stroke / font.
        style: DrawStyle,
    },
    /// Blur or pixelate a rectangle (privacy).
    Privacy {
        /// Left.
        x: f32,
        /// Top.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// `false` = Gaussian-ish blur, `true` = mosaic.
        pixelate: bool,
    },
    /// Translucent highlight wash.
    Highlight {
        /// Left.
        x: f32,
        /// Top.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// Wash color (alpha used).
        color: [u8; 4],
    },
    /// Text watermark at a named slot.
    WatermarkText {
        /// Caption.
        text: String,
        /// Corner / center.
        pos: WatermarkPos,
        /// Font / size / color / opacity.
        style: DrawStyle,
    },
    /// Image watermark at a named slot.
    WatermarkImage {
        /// Source file.
        path: PathBuf,
        /// Corner / center.
        pos: WatermarkPos,
        /// 0–100.
        opacity: f32,
        /// Scale versus the destination short side (0.05–1).
        scale: f32,
    },
    /// EXIF shoot date (or today) as a stamp.
    StampDate {
        /// Corner / center.
        pos: WatermarkPos,
        /// Font / size / color / opacity.
        style: DrawStyle,
    },
    /// Apply left-to-right.
    Stack(Vec<AnnotateOp>),
}

/// Stroke / type style.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawStyle {
    /// RGBA.
    pub color: [u8; 4],
    /// Stroke width in pixels.
    pub width: f32,
    /// Fill the shape (rect / ellipse / polygon).
    pub fill: bool,
    /// Family name (Segoe UI, …).
    pub font: String,
    /// Em size.
    pub size: f32,
    /// 0–100, used by watermarks and washes.
    pub opacity: f32,
}

impl Default for DrawStyle {
    fn default() -> Self {
        Self {
            color: [255, 51, 68, 255],
            width: 3.0,
            fill: false,
            font: "Segoe UI".into(),
            size: 22.0,
            opacity: 45.0,
        }
    }
}

/// Nine-slot watermark placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkPos {
    /// Top-left.
    Tl,
    /// Top-center.
    Tc,
    /// Top-right.
    Tr,
    /// Center-left.
    Cl,
    /// Center.
    Center,
    /// Center-right.
    Cr,
    /// Bottom-left.
    Bl,
    /// Bottom-center.
    Bc,
    /// Bottom-right.
    Br,
}

impl WatermarkPos {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "tl" | "nw" | "top-left" => Some(Self::Tl),
            "tc" | "n" | "top" => Some(Self::Tc),
            "tr" | "ne" | "top-right" => Some(Self::Tr),
            "cl" | "w" | "left" => Some(Self::Cl),
            "c" | "center" | "centre" => Some(Self::Center),
            "cr" | "e" | "right" => Some(Self::Cr),
            "bl" | "sw" | "bottom-left" => Some(Self::Bl),
            "bc" | "s" | "bottom" => Some(Self::Bc),
            "br" | "se" | "bottom-right" => Some(Self::Br),
            _ => None,
        }
    }
}

impl AnnotateOp {
    /// Filename suffix for [`save_sibling`].
    #[must_use]
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Line { arrow: true, .. } => "arrow",
            Self::Line { .. } => "line",
            Self::Rect { .. } => "rect",
            Self::Ellipse { .. } => "ellipse",
            Self::Polygon { .. } => "poly",
            Self::Pen { .. } => "pen",
            Self::Text { .. } => "text",
            Self::Callout { .. } => "callout",
            Self::Privacy { .. } => "privacy",
            Self::Highlight { .. } => "highlight",
            Self::WatermarkText { .. } | Self::WatermarkImage { .. } => "watermark",
            Self::StampDate { .. } => "stamp",
            Self::Stack(_) => "annotate",
        }
    }
}

/// Apply annotations to a decoded image.
///
/// # Errors
///
/// Missing watermark file or corrupt buffer.
pub fn apply_annotate(src: &LoadedImage, op: &AnnotateOp) -> Result<LoadedImage> {
    apply_annotate_in(src, op, None)
}

/// Load, annotate, write a sibling. Resolves the shoot-date stamp from `path`.
///
/// # Errors
///
/// I/O or decode.
pub fn apply_annotate_file(path: &Path, op: &AnnotateOp) -> Result<PathBuf> {
    let src = load_image_file(path)?;
    let out = apply_annotate_in(&src, op, Some(path))?;
    save_sibling(path, &out, op.suffix())
}

/// Tokens, `key=value`, and stacks (` | `).
#[must_use]
pub fn parse_annotate_line(raw: &str) -> Option<AnnotateOp> {
    let chunks = kv_chunks(raw);
    if chunks.is_empty() {
        return None;
    }
    let mut ops = Vec::new();
    for chunk in &chunks {
        if looks_like_style(chunk) {
            continue;
        }
        match parse_one(chunk, &chunks)? {
            AnnotateOp::Stack(inner) => ops.extend(inner),
            op => ops.push(op),
        }
    }
    if ops.is_empty() {
        return None;
    }
    if ops.len() == 1 {
        Some(ops.remove(0))
    } else {
        Some(AnnotateOp::Stack(ops))
    }
}

fn apply_annotate_in(
    src: &LoadedImage,
    op: &AnnotateOp,
    src_path: Option<&Path>,
) -> Result<LoadedImage> {
    if src.width == 0 || src.height == 0 || src.rgba.len() < 4 {
        return Ok(src.clone());
    }
    match op {
        AnnotateOp::Stack(ops) => {
            let mut cur = src.clone();
            for step in ops {
                cur = apply_annotate_in(&cur, step, src_path)?;
            }
            Ok(cur)
        }
        AnnotateOp::WatermarkImage {
            path,
            pos,
            opacity,
            scale,
        } => watermark_image(src, path, *pos, *opacity, *scale),
        AnnotateOp::StampDate { pos, style } => {
            let text = shoot_date(src_path);
            Ok(watermark_text(src, &text, *pos, style))
        }
        other => Ok(apply_draw(src, other)),
    }
}

fn apply_draw(src: &LoadedImage, op: &AnnotateOp) -> LoadedImage {
    let mut buf = src.rgba.as_ref().clone();
    let w = src.width;
    let h = src.height;
    match op {
        AnnotateOp::Line {
            x0,
            y0,
            x1,
            y1,
            style,
            arrow,
        } => {
            stroke_line(&mut buf, w, h, *x0, *y0, *x1, *y1, style);
            if *arrow {
                arrow_head(&mut buf, w, h, *x0, *y0, *x1, *y1, style);
            }
        }
        AnnotateOp::Rect {
            x,
            y,
            w: rw,
            h: rh,
            style,
        } => draw_rect(&mut buf, w, h, *x, *y, *rw, *rh, style),
        AnnotateOp::Ellipse {
            x,
            y,
            w: rw,
            h: rh,
            style,
        } => draw_ellipse(&mut buf, w, h, *x, *y, *rw, *rh, style),
        AnnotateOp::Polygon { points, style } => draw_poly(&mut buf, w, h, points, style),
        AnnotateOp::Pen { points, style } => {
            for pair in points.windows(2) {
                stroke_line(
                    &mut buf, w, h, pair[0].0, pair[0].1, pair[1].0, pair[1].1, style,
                );
            }
        }
        AnnotateOp::Text { x, y, text, style } => {
            draw_text(&mut buf, w, h, *x, *y, text, style, None);
        }
        AnnotateOp::Callout {
            x,
            y,
            w: cw,
            h: ch,
            text,
            style,
        } => draw_callout(&mut buf, w, h, *x, *y, *cw, *ch, text, style),
        AnnotateOp::Privacy {
            x,
            y,
            w: rw,
            h: rh,
            pixelate,
        } => privacy(&mut buf, w, h, *x, *y, *rw, *rh, *pixelate),
        AnnotateOp::Highlight {
            x,
            y,
            w: rw,
            h: rh,
            color,
        } => highlight(&mut buf, w, h, *x, *y, *rw, *rh, *color),
        AnnotateOp::WatermarkText { text, pos, style } => {
            return watermark_text(src, text, *pos, style);
        }
        _ => {}
    }
    wrap(src, buf)
}

fn parse_one(raw: &str, all: &[&str]) -> Option<AnnotateOp> {
    let style = style_from(all);
    let (key, val) = split_kv(raw)?;
    match key.as_str() {
        "line" | "arrow" => {
            let (x0, y0, x1, y1) = parse4(val?)?;
            Some(AnnotateOp::Line {
                x0,
                y0,
                x1,
                y1,
                style,
                arrow: key == "arrow" || flag(all, "arrow"),
            })
        }
        "rect" | "rectangle" => {
            let (x, y, w, h) = parse4(val?)?;
            Some(AnnotateOp::Rect { x, y, w, h, style })
        }
        "ellipse" | "oval" => {
            let (x, y, w, h) = parse4(val?)?;
            Some(AnnotateOp::Ellipse { x, y, w, h, style })
        }
        "poly" | "polygon" => Some(AnnotateOp::Polygon {
            points: parse_pts(val?)?,
            style,
        }),
        "pen" | "freehand" => Some(AnnotateOp::Pen {
            points: parse_pts(val?)?,
            style,
        }),
        "text" => Some(AnnotateOp::Text {
            text: val.unwrap_or("").to_string(),
            x: fval(all, "x").unwrap_or(16.0),
            y: fval(all, "y").unwrap_or(16.0),
            style,
        }),
        "callout" | "bubble" => Some(AnnotateOp::Callout {
            text: val.unwrap_or("").to_string(),
            x: fval(all, "x").unwrap_or(16.0),
            y: fval(all, "y").unwrap_or(16.0),
            w: fval(all, "w").unwrap_or(180.0),
            h: fval(all, "h").unwrap_or(72.0),
            style,
        }),
        "privacy" | "redact" => {
            let (x, y, w, h) = parse4(val?)?;
            let mode = sval(all, "mode").unwrap_or_default();
            Some(AnnotateOp::Privacy {
                x,
                y,
                w,
                h,
                pixelate: mode != "blur",
            })
        }
        "highlight" => {
            let (x, y, w, h) = parse4(val?)?;
            let mut color = style.color;
            color[3] = ((style.opacity / 100.0) * 120.0).clamp(20.0, 200.0) as u8;
            Some(AnnotateOp::Highlight { x, y, w, h, color })
        }
        "watermark" | "wm" => Some(AnnotateOp::WatermarkText {
            text: val.unwrap_or("©").to_string(),
            pos: sval(all, "pos")
                .and_then(WatermarkPos::parse)
                .unwrap_or(WatermarkPos::Br),
            style,
        }),
        "wm-image" | "watermark-image" => Some(AnnotateOp::WatermarkImage {
            path: PathBuf::from(val?),
            pos: sval(all, "pos")
                .and_then(WatermarkPos::parse)
                .unwrap_or(WatermarkPos::Br),
            opacity: style.opacity,
            scale: fval(all, "scale").unwrap_or(0.22),
        }),
        "stamp" | "date-stamp" => Some(AnnotateOp::StampDate {
            pos: val
                .and_then(WatermarkPos::parse)
                .or_else(|| sval(all, "pos").and_then(WatermarkPos::parse))
                .unwrap_or(WatermarkPos::Br),
            style,
        }),
        _ => None,
    }
}

fn looks_like_style(chunk: &str) -> bool {
    let k = chunk.split('=').next().unwrap_or("").trim();
    // `arrow=1,2,8,9` is an op; bare `arrow` / `arrow=true` is a line flag.
    if k.eq_ignore_ascii_case("arrow") {
        return !chunk.contains(',');
    }
    matches!(
        k,
        "color"
            | "width"
            | "fill"
            | "font"
            | "size"
            | "opacity"
            | "pos"
            | "scale"
            | "mode"
            | "x"
            | "y"
            | "w"
            | "h"
    )
}

fn style_from(all: &[&str]) -> DrawStyle {
    let mut s = DrawStyle::default();
    if let Some(c) = sval(all, "color").and_then(parse_color) {
        s.color = c;
    }
    if let Some(v) = fval(all, "width") {
        s.width = v;
    }
    if let Some(v) = fval(all, "size") {
        s.size = v;
    }
    if let Some(v) = fval(all, "opacity") {
        s.opacity = v;
    }
    if let Some(f) = sval(all, "font") {
        s.font = f.to_string();
    }
    s.fill = flag(all, "fill");
    s
}

fn wrap(src: &LoadedImage, rgba: Vec<u8>) -> LoadedImage {
    LoadedImage {
        rgba: std::sync::Arc::new(rgba),
        width: src.width,
        height: src.height,
        format: src.format,
        original_size_bytes: src.original_size_bytes,
        color_source: src.color_source.clone(),
        color_dest: src.color_dest.clone(),
        orientation: src.orientation,
        bit_depth: src.bit_depth,
        color_model: src.color_model.clone(),
    }
}

fn blend(buf: &mut [u8], w: u32, h: u32, x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 || c[3] == 0 {
        return;
    }
    let i = ((y as u32 * w + x as u32) * 4) as usize;
    let a = f32::from(c[3]) / 255.0;
    for n in 0..3 {
        buf[i + n] = (f32::from(buf[i + n]) * (1.0 - a) + f32::from(c[n]) * a).round() as u8;
    }
}

fn stroke_line(
    buf: &mut [u8],
    w: u32,
    h: u32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    style: &DrawStyle,
) {
    let r = (style.width * 0.5).max(0.5);
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = dx.hypot(dy).max(1.0);
    let steps = (len * 2.0).ceil() as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        fill_disk(buf, w, h, x0 + dx * t, y0 + dy * t, r, style.color);
    }
}

fn fill_disk(buf: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, r: f32, c: [u8; 4]) {
    let rr = r.ceil() as i32;
    let r2 = r * r;
    for dy in -rr..=rr {
        for dx in -rr..=rr {
            if (dx * dx + dy * dy) as f32 <= r2 {
                blend(buf, w, h, cx as i32 + dx, cy as i32 + dy, c);
            }
        }
    }
}

fn arrow_head(
    buf: &mut [u8],
    w: u32,
    h: u32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    style: &DrawStyle,
) {
    let ang = (y1 - y0).atan2(x1 - x0);
    let len = (style.width * 4.0).max(10.0);
    let a1 = ang + 2.7;
    let a2 = ang - 2.7;
    stroke_line(
        buf,
        w,
        h,
        x1,
        y1,
        x1 + a1.cos() * len,
        y1 + a1.sin() * len,
        style,
    );
    stroke_line(
        buf,
        w,
        h,
        x1,
        y1,
        x1 + a2.cos() * len,
        y1 + a2.sin() * len,
        style,
    );
}

fn draw_rect(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    x: f32,
    y: f32,
    rw: f32,
    rh: f32,
    style: &DrawStyle,
) {
    let x0 = x.min(x + rw);
    let y0 = y.min(y + rh);
    let x1 = x.max(x + rw);
    let y1 = y.max(y + rh);
    if style.fill {
        fill_rect(buf, bw, bh, x0, y0, x1 - x0, y1 - y0, style.color);
    }
    stroke_line(buf, bw, bh, x0, y0, x1, y0, style);
    stroke_line(buf, bw, bh, x1, y0, x1, y1, style);
    stroke_line(buf, bw, bh, x1, y1, x0, y1, style);
    stroke_line(buf, bw, bh, x0, y1, x0, y0, style);
}

fn fill_rect(buf: &mut [u8], bw: u32, bh: u32, x: f32, y: f32, rw: f32, rh: f32, c: [u8; 4]) {
    let x0 = x.floor().max(0.0) as i32;
    let y0 = y.floor().max(0.0) as i32;
    let x1 = (x + rw).ceil().min(bw as f32) as i32;
    let y1 = (y + rh).ceil().min(bh as f32) as i32;
    for py in y0..y1 {
        for px in x0..x1 {
            blend(buf, bw, bh, px, py, c);
        }
    }
}

fn draw_ellipse(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    x: f32,
    y: f32,
    rw: f32,
    rh: f32,
    style: &DrawStyle,
) {
    let cx = x + rw * 0.5;
    let cy = y + rh * 0.5;
    let rx = (rw.abs() * 0.5).max(1.0);
    let ry = (rh.abs() * 0.5).max(1.0);
    if style.fill {
        let x0 = (cx - rx).floor().max(0.0) as i32;
        let y0 = (cy - ry).floor().max(0.0) as i32;
        let x1 = (cx + rx).ceil().min(bw as f32) as i32;
        let y1 = (cy + ry).ceil().min(bh as f32) as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let nx = (px as f32 - cx) / rx;
                let ny = (py as f32 - cy) / ry;
                if nx * nx + ny * ny <= 1.0 {
                    blend(buf, bw, bh, px, py, style.color);
                }
            }
        }
    }
    let steps = ((rx + ry) * 4.0).ceil().max(16.0) as i32;
    let mut prev = (cx + rx, cy);
    for i in 1..=steps {
        let t = i as f32 / steps as f32 * std::f32::consts::TAU;
        let p = (cx + t.cos() * rx, cy + t.sin() * ry);
        stroke_line(buf, bw, bh, prev.0, prev.1, p.0, p.1, style);
        prev = p;
    }
}

fn draw_poly(buf: &mut [u8], bw: u32, bh: u32, pts: &[(f32, f32)], style: &DrawStyle) {
    if pts.len() < 2 {
        return;
    }
    if style.fill && pts.len() >= 3 {
        fill_poly(buf, bw, bh, pts, style.color);
    }
    for pair in pts.windows(2) {
        stroke_line(
            buf, bw, bh, pair[0].0, pair[0].1, pair[1].0, pair[1].1, style,
        );
    }
    if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
        stroke_line(buf, bw, bh, b.0, b.1, a.0, a.1, style);
    }
}

fn fill_poly(buf: &mut [u8], bw: u32, bh: u32, pts: &[(f32, f32)], c: [u8; 4]) {
    let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as i32;
    let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i32;
    let y0 = min_y.max(0);
    let y1 = max_y.min(bh as i32);
    for y in y0..y1 {
        let mut xs = Vec::new();
        let fy = y as f32 + 0.5;
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            if (a.1 <= fy && b.1 > fy) || (b.1 <= fy && a.1 > fy) {
                let t = (fy - a.1) / (b.1 - a.1).max(1e-4);
                xs.push(a.0 + (b.0 - a.0) * t);
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let x0 = pair[0].floor().max(0.0) as i32;
            let x1 = pair[1].ceil().min(bw as f32) as i32;
            for x in x0..x1 {
                blend(buf, bw, bh, x, y, c);
            }
        }
    }
}

fn draw_callout(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    x: f32,
    y: f32,
    cw: f32,
    ch: f32,
    text: &str,
    style: &DrawStyle,
) {
    let mut fill = style.clone();
    fill.fill = true;
    fill.color = [255, 255, 255, 230];
    draw_rect(buf, bw, bh, x, y, cw, ch, &fill);
    draw_rect(buf, bw, bh, x, y, cw, ch, style);
    let tip = [
        (x + 18.0, y + ch),
        (x + 42.0, y + ch),
        (x + 10.0, y + ch + 18.0),
    ];
    fill_poly(buf, bw, bh, &tip, [255, 255, 255, 230]);
    stroke_line(buf, bw, bh, tip[0].0, tip[0].1, tip[2].0, tip[2].1, style);
    stroke_line(buf, bw, bh, tip[1].0, tip[1].1, tip[2].0, tip[2].1, style);
    draw_text(buf, bw, bh, x + 8.0, y + 6.0, text, style, Some(cw - 16.0));
}

fn privacy(buf: &mut [u8], bw: u32, bh: u32, x: f32, y: f32, rw: f32, rh: f32, pixelate: bool) {
    let x0 = x.min(x + rw).floor().max(0.0) as u32;
    let y0 = y.min(y + rh).floor().max(0.0) as u32;
    let x1 = x.max(x + rw).ceil().min(bw as f32) as u32;
    let y1 = y.max(y + rh).ceil().min(bh as f32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    if pixelate {
        let block = ((x1 - x0).min(y1 - y0) / 8).clamp(6, 24);
        let mut y = y0;
        while y < y1 {
            let mut x = x0;
            while x < x1 {
                let xb = (x + block).min(x1);
                let yb = (y + block).min(y1);
                let mut acc = [0u32; 3];
                let mut n = 0u32;
                for py in y..yb {
                    for px in x..xb {
                        let i = ((py * bw + px) * 4) as usize;
                        acc[0] += u32::from(buf[i]);
                        acc[1] += u32::from(buf[i + 1]);
                        acc[2] += u32::from(buf[i + 2]);
                        n += 1;
                    }
                }
                n = n.max(1);
                let c = [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8];
                for py in y..yb {
                    for px in x..xb {
                        let i = ((py * bw + px) * 4) as usize;
                        buf[i] = c[0];
                        buf[i + 1] = c[1];
                        buf[i + 2] = c[2];
                    }
                }
                x += block;
            }
            y += block;
        }
        return;
    }
    let snap = buf.to_vec();
    let r = 4i32;
    for py in y0..y1 {
        for px in x0..x1 {
            let mut acc = [0u32; 3];
            let mut n = 0u32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let xx = (px as i32 + dx).clamp(0, bw as i32 - 1) as u32;
                    let yy = (py as i32 + dy).clamp(0, bh as i32 - 1) as u32;
                    let i = ((yy * bw + xx) * 4) as usize;
                    acc[0] += u32::from(snap[i]);
                    acc[1] += u32::from(snap[i + 1]);
                    acc[2] += u32::from(snap[i + 2]);
                    n += 1;
                }
            }
            let i = ((py * bw + px) * 4) as usize;
            buf[i] = (acc[0] / n) as u8;
            buf[i + 1] = (acc[1] / n) as u8;
            buf[i + 2] = (acc[2] / n) as u8;
        }
    }
}

fn highlight(buf: &mut [u8], bw: u32, bh: u32, x: f32, y: f32, rw: f32, rh: f32, color: [u8; 4]) {
    fill_rect(buf, bw, bh, x, y, rw, rh, color);
}

fn watermark_text(
    src: &LoadedImage,
    text: &str,
    pos: WatermarkPos,
    style: &DrawStyle,
) -> LoadedImage {
    let mut buf = src.rgba.as_ref().clone();
    let (tw, th) = measure_text(text, style, src.width as f32 * 0.7);
    let (x, y) = slot(src.width, src.height, tw, th, pos);
    let mut s = style.clone();
    s.color[3] = ((style.opacity / 100.0) * 255.0).clamp(20.0, 255.0) as u8;
    draw_text(
        &mut buf,
        src.width,
        src.height,
        x,
        y,
        text,
        &s,
        Some(tw + 8.0),
    );
    wrap(src, buf)
}

fn watermark_image(
    src: &LoadedImage,
    path: &Path,
    pos: WatermarkPos,
    opacity: f32,
    scale: f32,
) -> Result<LoadedImage> {
    let mark = load_image_file(path).map_err(|e| ViewerError::Metadata(e.to_string()))?;
    let short = src.width.min(src.height) as f32;
    let target = (short * scale.clamp(0.04, 1.0)).max(16.0);
    let ratio = target / mark.width.max(1) as f32;
    let dw = (mark.width as f32 * ratio).round().max(1.0) as u32;
    let dh = (mark.height as f32 * ratio).round().max(1.0) as u32;
    let resized = resize_rgba(&mark, dw, dh);
    let (x, y) = slot(src.width, src.height, dw as f32, dh as f32, pos);
    let mut buf = src.rgba.as_ref().clone();
    let a = (opacity / 100.0).clamp(0.05, 1.0);
    for row in 0..dh {
        for col in 0..dw {
            let si = ((row * dw + col) * 4) as usize;
            let mut c = [
                resized[si],
                resized[si + 1],
                resized[si + 2],
                (f32::from(resized[si + 3]) * a) as u8,
            ];
            if c[3] == 0 {
                c[3] = (a * 220.0) as u8;
            }
            blend(
                &mut buf,
                src.width,
                src.height,
                x as i32 + col as i32,
                y as i32 + row as i32,
                c,
            );
        }
    }
    Ok(wrap(src, buf))
}

fn resize_rgba(src: &LoadedImage, dw: u32, dh: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_raw(src.width, src.height, src.rgba.as_ref().clone())
        .unwrap_or_else(|| image::RgbaImage::new(1, 1));
    let out = image::imageops::resize(&img, dw, dh, image::imageops::FilterType::Triangle);
    out.into_raw()
}

fn slot(bw: u32, bh: u32, tw: f32, th: f32, pos: WatermarkPos) -> (f32, f32) {
    let m = 16.0;
    let max_x = (bw as f32 - tw - m).max(m);
    let max_y = (bh as f32 - th - m).max(m);
    let cx = ((bw as f32 - tw) * 0.5).max(m);
    let cy = ((bh as f32 - th) * 0.5).max(m);
    match pos {
        WatermarkPos::Tl => (m, m),
        WatermarkPos::Tc => (cx, m),
        WatermarkPos::Tr => (max_x, m),
        WatermarkPos::Cl => (m, cy),
        WatermarkPos::Center => (cx, cy),
        WatermarkPos::Cr => (max_x, cy),
        WatermarkPos::Bl => (m, max_y),
        WatermarkPos::Bc => (cx, max_y),
        WatermarkPos::Br => (max_x, max_y),
    }
}

fn shoot_date(path: Option<&Path>) -> String {
    if let Some(p) = path {
        if let Ok(ins) = inspect_image_file(p) {
            for (k, v) in ins.exif.iter().chain(ins.xmp.iter()) {
                if k.eq_ignore_ascii_case("DateTimeOriginal") || k.eq_ignore_ascii_case("DateTime")
                {
                    return stamp_label(v);
                }
            }
        }
    }
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn stamp_label(raw: &str) -> String {
    if raw.len() >= 10 {
        raw[..10].replace(':', "-")
    } else {
        raw.to_string()
    }
}

fn measure_text(text: &str, style: &DrawStyle, max_w: f32) -> (f32, f32) {
    let mut font_cx = FontContext::new();
    let mut layout_cx = LayoutContext::<[u8; 4]>::new();
    let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(style.size.clamp(8.0, 128.0)));
    builder.push_default(StyleProperty::Brush(style.color));
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(
        style.font.as_str(),
    )));
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(max_w.max(8.0)));
    (layout.width().max(8.0), layout.height().max(style.size))
}

fn draw_text(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    x: f32,
    y: f32,
    text: &str,
    style: &DrawStyle,
    max_w: Option<f32>,
) {
    if text.is_empty() {
        return;
    }
    let mut font_cx = FontContext::new();
    let mut layout_cx = LayoutContext::<[u8; 4]>::new();
    let mut scale_cx = ScaleContext::new();
    let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(style.size.clamp(8.0, 128.0)));
    builder.push_default(StyleProperty::Brush(style.color));
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(
        style.font.as_str(),
    )));
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(max_w.unwrap_or(bw as f32).max(8.0)));
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                render_run(&mut scale_cx, &glyph_run, buf, bw, bh, x, y, style.color);
            }
        }
    }
}

fn render_run(
    scale_cx: &mut ScaleContext,
    glyph_run: &parley::layout::GlyphRun<'_, [u8; 4]>,
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    origin_x: f32,
    origin_y: f32,
    color: [u8; 4],
) {
    let mut run_x = glyph_run.offset();
    let run_y = glyph_run.baseline();
    let run = glyph_run.run();
    let font = run.font();
    let Some(font_ref) = FontRef::from_index(font.data.as_ref(), font.index as usize) else {
        return;
    };
    let mut scaler = scale_cx
        .builder(font_ref)
        .size(run.font_size())
        .hint(true)
        .normalized_coords(run.normalized_coords())
        .build();
    for glyph in glyph_run.glyphs() {
        let gx = origin_x + run_x + glyph.x;
        let gy = origin_y + run_y + glyph.y;
        run_x += glyph.advance;
        let offset = Vector::new(gx.fract(), gy.fract());
        let Some(rendered) = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ])
        .format(Format::Alpha)
        .offset(offset)
        .render(&mut scaler, glyph.id as u16) else {
            continue;
        };
        if rendered.content != Content::Mask {
            continue;
        }
        let base_x = gx.floor() as i32 + rendered.placement.left;
        let base_y = gy.floor() as i32 - rendered.placement.top;
        let mut i = 0usize;
        for row in 0..rendered.placement.height {
            for col in 0..rendered.placement.width {
                let a = rendered.data[i];
                i += 1;
                if a == 0 {
                    continue;
                }
                let mut c = color;
                c[3] = ((u16::from(c[3]) * u16::from(a)) / 255) as u8;
                blend(buf, bw, bh, base_x + col as i32, base_y + row as i32, c);
            }
        }
    }
}

fn kv_chunks(raw: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in raw.lines() {
        for part in line.split(" | ") {
            let part = part.trim();
            if !part.is_empty() {
                out.push(part);
            }
        }
    }
    out
}

fn split_kv(raw: &str) -> Option<(String, Option<&str>)> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    match t.split_once('=') {
        Some((k, v)) => Some((k.trim().to_ascii_lowercase(), Some(v.trim()))),
        None => Some((t.to_ascii_lowercase(), None)),
    }
}

fn sval<'a>(all: &'a [&str], key: &str) -> Option<&'a str> {
    for chunk in all {
        if let Some((k, v)) = chunk.split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                return Some(v.trim());
            }
        }
    }
    None
}

fn fval(all: &[&str], key: &str) -> Option<f32> {
    sval(all, key)?.parse().ok()
}

fn flag(all: &[&str], key: &str) -> bool {
    match sval(all, key) {
        Some(v) => matches!(v, "1" | "true" | "yes" | "on"),
        None => all.iter().any(|c| c.eq_ignore_ascii_case(key)),
    }
}

fn parse4(raw: &str) -> Option<(f32, f32, f32, f32)> {
    let mut it = raw.split(',');
    Some((
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
    ))
}

fn parse_pts(raw: &str) -> Option<Vec<(f32, f32)>> {
    let mut out = Vec::new();
    for pair in raw.split([';', ' ']) {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (a, b) = pair.split_once(',')?;
        out.push((a.trim().parse().ok()?, b.trim().parse().ok()?));
    }
    (out.len() >= 2).then_some(out)
}

fn parse_color(raw: &str) -> Option<[u8; 4]> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return match hex.len() {
            3 => Some([
                u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
                255,
            ]),
            6 => Some([
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
                255,
            ]),
            8 => Some([
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
                u8::from_str_radix(&hex[6..8], 16).ok()?,
            ]),
            _ => None,
        };
    }
    let mut it = s.split(',');
    Some([
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
        it.next().and_then(|v| v.trim().parse().ok()).unwrap_or(255),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::loader::ImageFormat;

    fn rgb(w: u32, h: u32, pix: &[[u8; 3]]) -> LoadedImage {
        let mut rgba = Vec::new();
        for p in pix {
            rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
        }
        LoadedImage {
            rgba: std::sync::Arc::new(rgba),
            width: w,
            height: h,
            format: ImageFormat::Png,
            original_size_bytes: 0,
            ..LoadedImage::meta_defaults()
        }
    }

    #[test]
    fn line_paints_pixels() {
        let src = rgb(8, 8, &[[0, 0, 0]; 64]);
        let out = apply_annotate(
            &src,
            &AnnotateOp::Line {
                x0: 0.0,
                y0: 0.0,
                x1: 7.0,
                y1: 7.0,
                style: DrawStyle {
                    color: [255, 0, 0, 255],
                    width: 1.5,
                    ..DrawStyle::default()
                },
                arrow: true,
            },
        )
        .unwrap();
        assert_ne!(&out.rgba[..], &src.rgba[..]);
    }

    #[test]
    fn privacy_pixelates() {
        let mut pix = [[10u8, 10, 10]; 64];
        pix[0] = [255, 0, 0];
        pix[9] = [0, 255, 0];
        let src = rgb(8, 8, &pix);
        let out = apply_annotate(
            &src,
            &AnnotateOp::Privacy {
                x: 0.0,
                y: 0.0,
                w: 8.0,
                h: 8.0,
                pixelate: true,
            },
        )
        .unwrap();
        assert_ne!(out.rgba[0], 255);
    }

    #[test]
    fn parse_watermark_and_stack() {
        let op = parse_annotate_line("watermark=© Orchid | pos=br | opacity=40 | size=18").unwrap();
        assert!(matches!(
            op,
            AnnotateOp::WatermarkText {
                pos: WatermarkPos::Br,
                ..
            }
        ));
        let line = parse_annotate_line("arrow=1,2,8,9 | color=#00ff00 | width=2").unwrap();
        assert!(matches!(line, AnnotateOp::Line { arrow: true, .. }));
        assert!(parse_annotate_line("nope").is_none());
    }

    #[test]
    fn stamp_writes_text() {
        let src = rgb(64, 32, &[[255, 255, 255]; 64 * 32]);
        let out = apply_annotate(
            &src,
            &AnnotateOp::StampDate {
                pos: WatermarkPos::Br,
                style: DrawStyle {
                    color: [0, 0, 0, 255],
                    size: 14.0,
                    opacity: 90.0,
                    ..DrawStyle::default()
                },
            },
        )
        .unwrap();
        assert_ne!(&out.rgba[..], &src.rgba[..]);
    }
}
