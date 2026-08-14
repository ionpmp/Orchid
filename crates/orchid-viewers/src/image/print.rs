//! Raster print pages: size, margins, n-up, contact sheet, header/footer, ICC.

#![allow(clippy::needless_range_loop)]

use crate::error::{Result, ViewerError};
use crate::image::batch::image_date_token;
use crate::image::color::{load_print_icc, transform_srgb_to_icc};
use crate::image::loader::{load_image_file, ImageFormat, LoadedImage};
use crate::image::operations::{resize_filtered, ResizeFilter};

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat as ImgFmt, RgbaImage};
use parley::layout::PositionedLayoutItem;
use parley::style::{FontFamily, StyleProperty};
use parley::{FontContext, LayoutContext};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::FontRef;

/// How the photo sits in its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintFit {
    /// Letterbox.
    Contain,
    /// Fill and crop.
    Cover,
    /// Ignore aspect.
    Stretch,
}

impl PrintFit {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "contain" | "fit" => Some(Self::Contain),
            "cover" | "fill" => Some(Self::Cover),
            "stretch" => Some(Self::Stretch),
            _ => None,
        }
    }
}

/// Paper preset or custom millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaperSize {
    /// Width in millimetres.
    pub w_mm: f32,
    /// Height in millimetres.
    pub h_mm: f32,
}

impl PaperSize {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "a3" => Some(Self {
                w_mm: 297.0,
                h_mm: 420.0,
            }),
            "a4" => Some(Self {
                w_mm: 210.0,
                h_mm: 297.0,
            }),
            "a5" => Some(Self {
                w_mm: 148.0,
                h_mm: 210.0,
            }),
            "letter" => Some(Self {
                w_mm: 215.9,
                h_mm: 279.4,
            }),
            "4x6" | "6x4" => Some(Self {
                w_mm: 152.4,
                h_mm: 101.6,
            }),
            "5x7" | "7x5" => Some(Self {
                w_mm: 177.8,
                h_mm: 127.0,
            }),
            "8x10" | "10x8" => Some(Self {
                w_mm: 254.0,
                h_mm: 203.2,
            }),
            other => {
                let body = other.trim_end_matches("mm");
                let (a, b) = body.split_once('x')?;
                Some(Self {
                    w_mm: a.trim().parse().ok()?,
                    h_mm: b.trim().parse().ok()?,
                })
            }
        }
    }
}

/// Packed print job.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintSpec {
    /// Page size.
    pub paper: PaperSize,
    /// Swap width/height.
    pub landscape: bool,
    /// Margin on every side, millimetres.
    pub margin_mm: f32,
    /// Raster DPI (72–300).
    pub dpi: f32,
    /// Photos per page: 1, 2, 4, 6, or 9.
    pub n_up: u32,
    /// Index / contact sheet (all photos on one page).
    pub sheet: bool,
    /// Contact-sheet columns.
    pub cols: u32,
    /// Cell fit.
    pub fit: PrintFit,
    /// Header template (`{name}` `{date}` `{w}` `{h}` `{page}`).
    pub header: String,
    /// Footer template.
    pub footer: String,
    /// `srgb`, `monitor`, or a `.icc` path.
    pub icc: String,
}

impl Default for PrintSpec {
    fn default() -> Self {
        Self {
            paper: PaperSize {
                w_mm: 210.0,
                h_mm: 297.0,
            },
            landscape: false,
            margin_mm: 12.0,
            dpi: 150.0,
            n_up: 1,
            sheet: false,
            cols: 4,
            fit: PrintFit::Contain,
            header: String::new(),
            footer: "{name}  {date}".into(),
            icc: "srgb".into(),
        }
    }
}

/// One photo plus caption tokens.
#[derive(Debug, Clone)]
pub struct PrintItem {
    /// Decoded pixels.
    pub image: LoadedImage,
    /// File name.
    pub name: String,
    /// Shoot date or today.
    pub date: String,
}

/// `paper=a4 | margin=12 | nup=4 | header={name} | footer={date} | icc=srgb | sheet`.
#[must_use]
pub fn parse_print_line(raw: &str) -> Option<PrintSpec> {
    let mut spec = PrintSpec::default();
    let mut saw = false;
    for part in raw.split(" | ") {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        saw = true;
        let (k, v) = match part.split_once('=') {
            Some((a, b)) => (a.trim().to_ascii_lowercase(), Some(b.trim())),
            None => (part.to_ascii_lowercase(), None),
        };
        match k.as_str() {
            "paper" => spec.paper = PaperSize::parse(v?)?,
            "margin" | "margins" => spec.margin_mm = v?.parse().ok()?,
            "dpi" => spec.dpi = v?.parse().ok()?,
            "nup" | "n-up" => spec.n_up = v?.parse().ok()?,
            "cols" => spec.cols = v?.parse::<u32>().ok()?.max(1),
            "fit" => spec.fit = PrintFit::parse(v?)?,
            "header" => spec.header = v.unwrap_or("").to_string(),
            "footer" => spec.footer = v.unwrap_or("").to_string(),
            "icc" | "profile" => spec.icc = v.unwrap_or("srgb").to_string(),
            "sheet" | "index" | "contact" => spec.sheet = true,
            "landscape" => spec.landscape = true,
            "folder" | "preview" => {}
            _ => return None,
        }
    }
    saw.then_some(spec)
}

/// Load files and rasterise print pages.
///
/// # Errors
///
/// Decode or empty set.
pub fn render_print_files(paths: &[&Path], spec: &PrintSpec) -> Result<Vec<LoadedImage>> {
    let mut items = Vec::with_capacity(paths.len());
    for p in paths {
        items.push(item_from_path(p)?);
    }
    render_print_pages(&items, spec)
}

/// Write the first page beside `hint` as `{stem}-print.png`.
///
/// # Errors
///
/// Decode or encode.
pub fn write_print_preview(paths: &[&Path], spec: &PrintSpec, hint: &Path) -> Result<PathBuf> {
    let pages = render_print_files(paths, spec)?;
    let page = pages
        .first()
        .ok_or_else(|| ViewerError::Metadata("nothing to print".into()))?;
    save_png(hint, page, "print")
}

/// Rasterise every page to temp PNGs (for the OS print verb).
///
/// # Errors
///
/// Decode, encode, or I/O.
pub fn write_print_temps(paths: &[&Path], spec: &PrintSpec) -> Result<Vec<PathBuf>> {
    let pages = render_print_files(paths, spec)?;
    let mut out = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let name = format!("orchid-print-{i}.png");
        let dest = std::env::temp_dir().join(name);
        let bytes = encode_png(page)?;
        std::fs::write(&dest, bytes)?;
        out.push(dest);
    }
    Ok(out)
}

/// Hand `path` to the OS printer (`Print` verb / `lp`).
///
/// # Errors
///
/// Spawn failure.
pub fn send_to_printer(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let quoted = path.display().to_string().replace('\'', "''");
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &format!("Start-Process -FilePath '{quoted}' -Verb Print"),
            ])
            .spawn()
            .map_err(|e| ViewerError::Metadata(e.to_string()))?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("lp")
            .arg(path)
            .spawn()
            .map_err(|e| ViewerError::Metadata(e.to_string()))?;
    }
    Ok(())
}

/// Lay out `items` onto one or more pages.
///
/// # Errors
///
/// Empty set.
pub fn render_print_pages(items: &[PrintItem], spec: &PrintSpec) -> Result<Vec<LoadedImage>> {
    if items.is_empty() {
        return Err(ViewerError::Metadata("nothing to print".into()));
    }
    let (pw, ph) = page_px(spec);
    let chunks: Vec<&[PrintItem]> = if spec.sheet {
        vec![items]
    } else {
        let n = spec.n_up.clamp(1, 9) as usize;
        items.chunks(n).collect()
    };
    let mut pages = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        pages.push(render_page(chunk, spec, pw, ph, i + 1, chunks.len())?);
    }
    Ok(pages)
}

fn item_from_path(path: &Path) -> Result<PrintItem> {
    let image = load_image_file(path)?;
    Ok(PrintItem {
        name: path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string(),
        date: image_date_token(path),
        image,
    })
}

fn page_px(spec: &PrintSpec) -> (u32, u32) {
    let dpi = spec.dpi.clamp(72.0, 300.0);
    let (mut w, mut h) = (spec.paper.w_mm, spec.paper.h_mm);
    if spec.landscape {
        std::mem::swap(&mut w, &mut h);
    }
    let to_px = |mm: f32| ((mm / 25.4) * dpi).round().max(32.0) as u32;
    (to_px(w), to_px(h))
}

fn render_page(
    items: &[PrintItem],
    spec: &PrintSpec,
    pw: u32,
    ph: u32,
    page: usize,
    pages: usize,
) -> Result<LoadedImage> {
    let dpi = spec.dpi.clamp(72.0, 300.0);
    let margin = ((spec.margin_mm / 25.4) * dpi).round().max(0.0) as u32;
    let mut buf = vec![255u8; (pw * ph * 4) as usize];
    let head_h = if spec.header.is_empty() { 0 } else { 28 };
    let foot_h = if spec.footer.is_empty() { 0 } else { 28 };
    let x0 = margin.min(pw / 4);
    let y0 = margin.min(ph / 4);
    let x1 = pw.saturating_sub(margin).max(x0 + 8);
    let y1 = ph.saturating_sub(margin).max(y0 + 8);
    if head_h > 0 {
        let text = expand_tokens(&spec.header, items.first(), page, pages);
        draw_caption(
            &mut buf,
            pw,
            ph,
            x0 as f32,
            y0 as f32,
            x1.saturating_sub(x0) as f32,
            &text,
        );
    }
    if foot_h > 0 {
        let text = expand_tokens(&spec.footer, items.first(), page, pages);
        draw_caption(
            &mut buf,
            pw,
            ph,
            x0 as f32,
            (y1.saturating_sub(foot_h)) as f32,
            x1.saturating_sub(x0) as f32,
            &text,
        );
    }
    let cx0 = x0;
    let cy0 = y0 + head_h;
    let cw = x1.saturating_sub(cx0).max(8);
    let ch = y1.saturating_sub(cy0 + foot_h).max(8);
    let (cols, rows) = grid_for(items.len() as u32, spec);
    let gap = 8u32;
    let cell_w = cw.saturating_sub(gap * cols.saturating_sub(1)) / cols.max(1);
    let cell_h = ch.saturating_sub(gap * rows.saturating_sub(1)) / rows.max(1);
    for (i, item) in items.iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;
        let x = cx0 + col * (cell_w + gap);
        let y = cy0 + row * (cell_h + gap);
        blit_fit(
            &mut buf,
            pw,
            ph,
            x,
            y,
            cell_w,
            cell_h,
            &item.image,
            spec.fit,
        )?;
        if spec.sheet {
            draw_caption(
                &mut buf,
                pw,
                ph,
                x as f32,
                (y + cell_h.saturating_sub(16)) as f32,
                cell_w as f32,
                &item.name,
            );
        }
    }
    if let Some(icc) = load_print_icc(&spec.icc) {
        let _ = transform_srgb_to_icc(&mut buf, &icc);
    }
    Ok(LoadedImage {
        rgba: std::sync::Arc::new(buf),
        width: pw,
        height: ph,
        format: ImageFormat::Png,
        original_size_bytes: 0,
        ..LoadedImage::meta_defaults()
    })
}

fn grid_for(count: u32, spec: &PrintSpec) -> (u32, u32) {
    if spec.sheet {
        let cols = spec.cols.max(1);
        return (cols, count.div_ceil(cols).max(1));
    }
    match spec.n_up.clamp(1, 9) {
        1 => (1, 1),
        2 => {
            if spec.landscape {
                (2, 1)
            } else {
                (1, 2)
            }
        }
        4 => (2, 2),
        6 => {
            if spec.landscape {
                (3, 2)
            } else {
                (2, 3)
            }
        }
        _ => (3, 3),
    }
}

fn blit_fit(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    x: u32,
    y: u32,
    cw: u32,
    ch: u32,
    src: &LoadedImage,
    fit: PrintFit,
) -> Result<()> {
    if cw == 0 || ch == 0 || src.width == 0 || src.height == 0 {
        return Ok(());
    }
    let (tw, th) = match fit {
        PrintFit::Stretch => (cw, ch),
        PrintFit::Contain => {
            let s = (cw as f32 / src.width as f32).min(ch as f32 / src.height as f32);
            (
                (src.width as f32 * s).round().max(1.0) as u32,
                (src.height as f32 * s).round().max(1.0) as u32,
            )
        }
        PrintFit::Cover => {
            let s = (cw as f32 / src.width as f32).max(ch as f32 / src.height as f32);
            (
                (src.width as f32 * s).round().max(1.0) as u32,
                (src.height as f32 * s).round().max(1.0) as u32,
            )
        }
    };
    let scaled = resize_filtered(src, tw, th, ResizeFilter::Bilinear)?;
    let ox = x + cw.saturating_sub(tw.min(cw)) / 2;
    let oy = y + ch.saturating_sub(th.min(ch)) / 2;
    let copy_w = tw.min(cw);
    let copy_h = th.min(ch);
    let sx0 = tw.saturating_sub(copy_w) / 2;
    let sy0 = th.saturating_sub(copy_h) / 2;
    for row in 0..copy_h {
        let dy = oy + row;
        if dy >= bh {
            break;
        }
        for col in 0..copy_w {
            let dx = ox + col;
            if dx >= bw {
                break;
            }
            let di = ((dy * bw + dx) * 4) as usize;
            let si = (((sy0 + row) * tw + (sx0 + col)) * 4) as usize;
            if di + 3 < buf.len() && si + 3 < scaled.rgba.len() {
                buf[di..di + 4].copy_from_slice(&scaled.rgba[si..si + 4]);
            }
        }
    }
    Ok(())
}

fn expand_tokens(tpl: &str, item: Option<&PrintItem>, page: usize, pages: usize) -> String {
    let (name, date, w, h) = match item {
        Some(it) => (
            it.name.as_str(),
            it.date.as_str(),
            it.image.width,
            it.image.height,
        ),
        None => ("", "", 0, 0),
    };
    tpl.replace("{name}", name)
        .replace("{date}", date)
        .replace("{w}", &w.to_string())
        .replace("{h}", &h.to_string())
        .replace("{wxh}", &format!("{w}×{h}"))
        .replace("{page}", &page.to_string())
        .replace("{pages}", &pages.to_string())
}

fn draw_caption(buf: &mut [u8], bw: u32, bh: u32, x: f32, y: f32, max_w: f32, text: &str) {
    if text.is_empty() {
        return;
    }
    let mut font_cx = FontContext::new();
    let mut layout_cx = LayoutContext::<[u8; 4]>::new();
    let mut scale_cx = ScaleContext::new();
    let color = [40u8, 40, 40, 255];
    let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(14.0));
    builder.push_default(StyleProperty::Brush(color));
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Segoe UI")));
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(max_w.max(8.0)));
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                let mut run_x = glyph_run.offset();
                let run_y = glyph_run.baseline();
                let run = glyph_run.run();
                let font = run.font();
                let Some(font_ref) = FontRef::from_index(font.data.as_ref(), font.index as usize)
                else {
                    continue;
                };
                let mut scaler = scale_cx
                    .builder(font_ref)
                    .size(run.font_size())
                    .hint(true)
                    .normalized_coords(run.normalized_coords())
                    .build();
                for glyph in glyph_run.glyphs() {
                    let gx = x + run_x + glyph.x;
                    let gy = y + run_y + glyph.y;
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
                            let px = base_x + col as i32;
                            let py = base_y + row as i32;
                            if px < 0 || py < 0 || px >= bw as i32 || py >= bh as i32 {
                                continue;
                            }
                            let di = ((py as u32 * bw + px as u32) * 4) as usize;
                            let aa = f32::from(a) / 255.0;
                            for n in 0..3 {
                                buf[di + n] = (f32::from(buf[di + n]) * (1.0 - aa)
                                    + f32::from(color[n]) * aa)
                                    .round() as u8;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn save_png(hint: &Path, img: &LoadedImage, suffix: &str) -> Result<PathBuf> {
    let stem = hint.file_stem().and_then(|s| s.to_str()).unwrap_or("print");
    let dir = hint.parent().unwrap_or_else(|| Path::new("."));
    let mut dest = dir.join(format!("{stem}-{suffix}.png"));
    let mut n = 2u32;
    while dest.exists() {
        dest = dir.join(format!("{stem}-{suffix}-{n}.png"));
        n += 1;
    }
    std::fs::write(&dest, encode_png(img)?)?;
    Ok(dest)
}

fn encode_png(img: &LoadedImage) -> Result<Vec<u8>> {
    let rgba = RgbaImage::from_raw(img.width, img.height, img.rgba.to_vec())
        .ok_or_else(|| ViewerError::ImageDecode("print encode".into()))?;
    let mut buf = Vec::new();
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut Cursor::new(&mut buf), ImgFmt::Png)
        .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(w: u32, h: u32, pix: [u8; 3]) -> PrintItem {
        let mut rgba = Vec::new();
        for _ in 0..(w * h) {
            rgba.extend_from_slice(&[pix[0], pix[1], pix[2], 255]);
        }
        PrintItem {
            image: LoadedImage {
                rgba: std::sync::Arc::new(rgba),
                width: w,
                height: h,
                format: ImageFormat::Png,
                original_size_bytes: 0,
                ..LoadedImage::meta_defaults()
            },
            name: "shot.jpg".into(),
            date: "2024-05-01".into(),
        }
    }

    #[test]
    fn parse_nup_and_paper() {
        let spec =
            parse_print_line("paper=a5 | margin=8 | nup=4 | header={name} | icc=srgb").unwrap();
        assert_eq!(spec.paper.w_mm, 148.0);
        assert_eq!(spec.n_up, 4);
        assert_eq!(spec.header, "{name}");
        assert!(parse_print_line("paper=nope").is_none());
    }

    #[test]
    fn single_page_uses_paper_pixels() {
        let spec = PrintSpec {
            paper: PaperSize {
                w_mm: 50.0,
                h_mm: 40.0,
            },
            dpi: 72.0,
            margin_mm: 2.0,
            header: String::new(),
            footer: String::new(),
            ..PrintSpec::default()
        };
        let pages = render_print_pages(&[rgb(8, 8, [200, 10, 10])], &spec).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].width, ((50.0_f32 / 25.4) * 72.0).round() as u32);
        assert!(pages[0]
            .rgba
            .chunks_exact(4)
            .any(|p| p[0] > 180 && p[1] < 40));
    }

    #[test]
    fn nup_four_makes_two_pages_for_five() {
        let spec = PrintSpec {
            paper: PaperSize {
                w_mm: 60.0,
                h_mm: 60.0,
            },
            dpi: 72.0,
            n_up: 4,
            header: String::new(),
            footer: String::new(),
            ..PrintSpec::default()
        };
        let items = vec![
            rgb(6, 6, [255, 0, 0]),
            rgb(6, 6, [0, 255, 0]),
            rgb(6, 6, [0, 0, 255]),
            rgb(6, 6, [255, 255, 0]),
            rgb(6, 6, [0, 255, 255]),
        ];
        let pages = render_print_pages(&items, &spec).unwrap();
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn header_writes_pixels() {
        let spec = PrintSpec {
            paper: PaperSize {
                w_mm: 80.0,
                h_mm: 50.0,
            },
            dpi: 72.0,
            header: "{name} {date}".into(),
            footer: String::new(),
            ..PrintSpec::default()
        };
        let blank = rgb(4, 4, [255, 255, 255]);
        let pages = render_print_pages(&[blank], &spec).unwrap();
        assert!(pages[0].rgba.iter().any(|&b| b < 200));
    }
}
