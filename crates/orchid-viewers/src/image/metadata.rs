//! EXIF / IPTC / XMP / GPS / hashes / histogram / cursor color for still images.

use std::path::Path;

use md5::Digest;
use md5::Md5;
use sha2::Sha256;

use crate::error::{Result, ViewerError};
use crate::image::exif::read_exif_fields;

/// GPS fix in signed decimal degrees (N/E positive).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsFix {
    /// Latitude, −90…90.
    pub lat: f64,
    /// Longitude, −180…180.
    pub lon: f64,
}

impl GpsFix {
    /// OpenStreetMap pin URL.
    #[must_use]
    pub fn map_url(self) -> String {
        format!(
            "https://www.openstreetmap.org/?mlat={:.6}&mlon={:.6}#map=16/{:.6}/{:.6}",
            self.lat, self.lon, self.lat, self.lon
        )
    }

    /// Short `lat, lon` label.
    #[must_use]
    pub fn label(self) -> String {
        format!("{:.5}, {:.5}", self.lat, self.lon)
    }

    /// Great-circle distance in kilometres (WGS-84 sphere).
    #[must_use]
    pub fn distance_km(self, other: Self) -> f64 {
        const EARTH_KM: f64 = 6371.0;
        let dlat = (other.lat - self.lat).to_radians();
        let dlon = (other.lon - self.lon).to_radians();
        let a = (dlat * 0.5).sin().powi(2)
            + self.lat.to_radians().cos()
                * other.lat.to_radians().cos()
                * (dlon * 0.5).sin().powi(2);
        2.0 * EARTH_KM * a.sqrt().asin()
    }
}

/// File + sidecar metadata gathered without decoding pixels.
#[derive(Debug, Clone, Default)]
pub struct ImageInspect {
    /// Camera / exposure EXIF (already filtered).
    pub exif: Vec<(String, String)>,
    /// IPTC-NAA IIM fields.
    pub iptc: Vec<(String, String)>,
    /// A short XMP / Adobe subset.
    pub xmp: Vec<(String, String)>,
    /// GPS when both latitude and longitude parse.
    pub gps: Option<GpsFix>,
    /// Lowercase hex MD5.
    pub md5: String,
    /// Lowercase hex SHA-256.
    pub sha256: String,
    /// Embedded ICC description, if any.
    pub icc_label: String,
    /// One-line camera overlay (make / exposure / date).
    pub overlay: String,
}

/// 256-bin luma + RGB histograms.
#[derive(Debug, Clone)]
pub struct ChannelHistogram {
    /// Rec. 601 luma.
    pub luma: [u32; 256],
    /// Red channel.
    pub r: [u32; 256],
    /// Green channel.
    pub g: [u32; 256],
    /// Blue channel.
    pub b: [u32; 256],
}

impl Default for ChannelHistogram {
    fn default() -> Self {
        Self {
            luma: [0; 256],
            r: [0; 256],
            g: [0; 256],
            b: [0; 256],
        }
    }
}

/// Which histogram to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum HistMode {
    /// Combined brightness.
    #[default]
    Luma = 0,
    /// Overlaid R+G+B.
    Rgb = 1,
    /// Red only.
    Red = 2,
    /// Green only.
    Green = 3,
    /// Blue only.
    Blue = 4,
}

impl HistMode {
    /// Persist encoding.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Self::as_u8`].
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Rgb,
            2 => Self::Red,
            3 => Self::Green,
            4 => Self::Blue,
            _ => Self::Luma,
        }
    }

    /// Cycle luma → RGB → R → G → B → luma.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Luma => Self::Rgb,
            Self::Rgb => Self::Red,
            Self::Red => Self::Green,
            Self::Green => Self::Blue,
            Self::Blue => Self::Luma,
        }
    }
}

/// EXIF / IPTC / XMP / GPS without hashing or a full-file read (Find Files).
#[must_use]
pub fn inspect_image_tags(path: &Path) -> ImageInspect {
    const PREFIX: usize = 4 * 1024 * 1024;
    let exif = read_exif_fields(path).unwrap_or_default();
    let bytes = read_prefix_bytes(path, PREFIX);
    let iptc = parse_iptc(&bytes);
    let mut xmp = parse_xmp(&bytes);
    if let Some(side) = sidecar_xmp_path(path).and_then(|p| std::fs::read(p).ok()) {
        merge_fields(&mut xmp, parse_xmp(&side));
    }
    let gps = gps_from_exif(&exif)
        .or_else(|| gps_from_bytes(&bytes))
        .or_else(|| gps_from_xmp_fields(&xmp));
    let overlay = camera_overlay(&exif);
    ImageInspect {
        exif,
        iptc,
        xmp,
        gps,
        md5: String::new(),
        sha256: String::new(),
        icc_label: String::new(),
        overlay,
    }
}

fn read_prefix_bytes(path: &Path, max: usize) -> Vec<u8> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buf = vec![0_u8; max];
    let n = std::io::Read::read(&mut file, &mut buf).unwrap_or(0);
    buf.truncate(n);
    buf
}

/// Read EXIF, IPTC, XMP, GPS, ICC label, and file hashes from `path`.
///
/// # Errors
///
/// I/O on the image file.
pub fn inspect_image_file(path: &Path) -> Result<ImageInspect> {
    let bytes = std::fs::read(path)?;
    let mut inspect = inspect_image_bytes(&bytes, Some(path));
    if let Some(side) = sidecar_xmp_path(path).and_then(|p| std::fs::read(p).ok()) {
        let extra = parse_xmp(&side);
        merge_fields(&mut inspect.xmp, extra);
        if inspect.gps.is_none() {
            inspect.gps = gps_from_xmp_fields(&inspect.xmp);
        }
    }
    Ok(inspect)
}

pub(crate) fn sidecar_xmp_path(path: &Path) -> Option<std::path::PathBuf> {
    let name = path.file_name()?.to_str()?;
    Some(path.with_file_name(format!("{name}.xmp")))
}

fn merge_fields(dest: &mut Vec<(String, String)>, extra: Vec<(String, String)>) {
    for (k, v) in extra {
        if let Some((_, existing)) = dest.iter_mut().find(|(dk, _)| dk == &k) {
            if existing.is_empty() {
                *existing = v;
            }
        } else {
            dest.push((k, v));
        }
    }
}

/// Same as [`inspect_image_file`] from an already-read buffer.
#[must_use]
pub fn inspect_image_bytes(bytes: &[u8], path: Option<&Path>) -> ImageInspect {
    let exif = path
        .and_then(|p| read_exif_fields(p).ok())
        .unwrap_or_else(|| read_exif_from_bytes(bytes));
    let iptc = parse_iptc(bytes);
    let xmp = parse_xmp(bytes);
    let gps = gps_from_exif(&exif)
        .or_else(|| gps_from_bytes(bytes))
        .or_else(|| gps_from_xmp_fields(&xmp));
    let icc_label = crate::image::color::embedded_icc_label(bytes).unwrap_or_default();
    let (md5, sha256) = file_hashes(bytes);
    let overlay = camera_overlay(&exif);
    ImageInspect {
        exif,
        iptc,
        xmp,
        gps,
        md5,
        sha256,
        icc_label,
        overlay,
    }
}

/// IPTC / XMP / GPS only (no hashes) for the FM properties report.
#[must_use]
pub fn format_sidecar_report(path: &Path) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let iptc = parse_iptc(&bytes);
    let xmp = parse_xmp(&bytes);
    let gps = read_exif_fields(path)
        .ok()
        .as_deref()
        .and_then(gps_from_exif)
        .or_else(|| gps_from_bytes(&bytes));
    let mut body = String::new();
    append_section(&mut body, "IPTC", &iptc);
    append_section(&mut body, "XMP", &xmp);
    if let Some(g) = gps {
        body.push_str("GPS: ");
        body.push_str(&g.label());
        body.push('\n');
        body.push_str(&g.map_url());
        body.push('\n');
    }
    body
}

/// Build the in-viewer metadata panel text.
#[must_use]
pub fn format_inspect_panel(
    width: u32,
    height: u32,
    size_bytes: u64,
    format: &str,
    bit_depth: u8,
    color_model: &str,
    color_source: &str,
    color_dest: &str,
    inspect: &ImageInspect,
) -> String {
    let mut body = String::new();
    body.push_str("File\n");
    body.push_str(&format!("  Size     {}\n", format_bytes(size_bytes)));
    body.push_str(&format!("  Image    {width} × {height}\n"));
    let bits = if bit_depth == 0 { 8 } else { bit_depth };
    let model = if color_model.is_empty() {
        "RGB"
    } else {
        color_model
    };
    body.push_str(&format!("  Format   {format} · {bits}-bit {model}\n"));
    let icc = if !inspect.icc_label.is_empty() {
        inspect.icc_label.as_str()
    } else if !color_source.is_empty() {
        color_source
    } else {
        "sRGB"
    };
    body.push_str("  ICC      ");
    body.push_str(icc);
    if !color_dest.is_empty() && color_dest != icc && color_dest != color_source {
        body.push_str(" → ");
        body.push_str(color_dest);
    }
    body.push('\n');
    body.push_str("\nHashes\n");
    if !inspect.md5.is_empty() {
        body.push_str("  MD5      ");
        body.push_str(&inspect.md5);
        body.push('\n');
    }
    if !inspect.sha256.is_empty() {
        body.push_str("  SHA-256  ");
        body.push_str(&inspect.sha256);
        body.push('\n');
    }
    append_section(&mut body, "EXIF", &inspect.exif);
    append_section(&mut body, "IPTC", &inspect.iptc);
    append_section(&mut body, "XMP", &inspect.xmp);
    if let Some(g) = inspect.gps {
        body.push_str("\nGPS\n  ");
        body.push_str(&g.label());
        body.push('\n');
    }
    body
}

fn append_section(body: &mut String, title: &str, fields: &[(String, String)]) {
    if fields.is_empty() {
        return;
    }
    body.push('\n');
    body.push_str(title);
    body.push('\n');
    for (k, v) in fields {
        body.push_str("  ");
        body.push_str(k);
        body.push_str("  ");
        body.push_str(v);
        body.push('\n');
    }
}

fn format_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let x = n as f64;
    if x >= GB {
        format!("{:.2} GB", x / GB)
    } else if x >= MB {
        format!("{:.2} MB", x / MB)
    } else if x >= KB {
        format!("{:.1} KB", x / KB)
    } else {
        format!("{n} B")
    }
}

fn file_hashes(bytes: &[u8]) -> (String, String) {
    let md5 = hex::encode(Md5::digest(bytes));
    let sha = hex::encode(Sha256::digest(bytes));
    (md5, sha)
}

fn read_exif_from_bytes(bytes: &[u8]) -> Vec<(String, String)> {
    let mut reader = std::io::Cursor::new(bytes);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for field in exif.fields() {
        let tag = field.tag.to_string();
        if tag.starts_with("Unknown") || tag.contains("MakerNote") {
            continue;
        }
        let value = field.display_value().with_unit(&exif).to_string();
        if value.is_empty() {
            continue;
        }
        out.push((tag, value));
    }
    out
}

fn gps_from_exif(fields: &[(String, String)]) -> Option<GpsFix> {
    let lat = parse_dms(field(fields, "GPSLatitude")?)?;
    let lon = parse_dms(field(fields, "GPSLongitude")?)?;
    let lat_ref = field(fields, "GPSLatitudeRef").unwrap_or("N");
    let lon_ref = field(fields, "GPSLongitudeRef").unwrap_or("E");
    let lat = if lat_ref.starts_with('S') { -lat } else { lat };
    let lon = if lon_ref.starts_with('W') { -lon } else { lon };
    Some(GpsFix { lat, lon })
}

fn gps_from_bytes(bytes: &[u8]) -> Option<GpsFix> {
    let mut reader = std::io::Cursor::new(bytes);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    let lat = dms_value(exif.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)?)?;
    let lon = dms_value(exif.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY)?)?;
    let lat_ref = exif
        .get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY)
        .map(|f| f.display_value().to_string())
        .unwrap_or_else(|| "N".into());
    let lon_ref = exif
        .get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY)
        .map(|f| f.display_value().to_string())
        .unwrap_or_else(|| "E".into());
    let lat = if lat_ref.contains('S') { -lat } else { lat };
    let lon = if lon_ref.contains('W') { -lon } else { lon };
    Some(GpsFix { lat, lon })
}

pub(crate) fn gps_from_xmp_fields(fields: &[(String, String)]) -> Option<GpsFix> {
    let lat = parse_gps_component(field(fields, "GPSLatitude")?)?;
    let lon = parse_gps_component(field(fields, "GPSLongitude")?)?;
    Some(GpsFix { lat, lon })
}

fn parse_gps_component(raw: &str) -> Option<f64> {
    let t = raw.trim();
    let sign = if t.contains('S') || t.contains('W') {
        -1.0
    } else {
        1.0
    };
    let cleaned: String = t
        .chars()
        .map(|c| {
            if c.is_ascii_digit() || c == '.' || c == ',' || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect();
    if let Ok(v) = t
        .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.')
        .parse::<f64>()
    {
        if !t.contains(',') {
            return Some(v * sign);
        }
    }
    parse_dms(&cleaned).map(|v| v * sign)
}

fn dms_value(field: &exif::Field) -> Option<f64> {
    match &field.value {
        exif::Value::Rational(rs) if rs.len() >= 3 => {
            let d = rs[0].to_f64();
            let m = rs[1].to_f64();
            let s = rs[2].to_f64();
            Some(d + m / 60.0 + s / 3600.0)
        }
        _ => parse_dms(&field.display_value().to_string()),
    }
}

/// Parse `51 deg 28 min 38.12 sec` or `51, 28, 38.12` or `51/1 28/1 3812/100`.
fn parse_dms(raw: &str) -> Option<f64> {
    let nums: Vec<f64> = raw
        .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .filter_map(|s| s.parse().ok())
        .collect();
    match nums.as_slice() {
        [d] => Some(*d),
        [d, m] => Some(d + m / 60.0),
        [d, m, s, ..] => Some(d + m / 60.0 + s / 3600.0),
        _ => None,
    }
}

pub(crate) fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Short overlay: camera, exposure, date.
#[must_use]
pub fn camera_overlay(exif: &[(String, String)]) -> String {
    let mut parts = Vec::new();
    let make = field(exif, "Make").unwrap_or("");
    let model = field(exif, "Model").unwrap_or("");
    let camera = format!("{make} {model}").trim().to_string();
    if !camera.is_empty() {
        parts.push(camera);
    }
    if let Some(lens) = field(exif, "LensModel").or_else(|| field(exif, "LensMake")) {
        parts.push(lens.to_string());
    }
    let mut exp = String::new();
    if let Some(f) = field(exif, "FNumber") {
        exp.push_str(f);
        exp.push(' ');
    }
    if let Some(t) = field(exif, "ExposureTime") {
        exp.push_str(t);
        exp.push(' ');
    }
    if let Some(iso) =
        field(exif, "PhotographicSensitivity").or_else(|| field(exif, "ISOSpeedRatings"))
    {
        if !iso.contains("ISO") {
            exp.push_str("ISO ");
        }
        exp.push_str(iso);
    }
    let exp = exp.trim();
    if !exp.is_empty() {
        parts.push(exp.to_string());
    }
    if let Some(d) = field(exif, "DateTimeOriginal").or_else(|| field(exif, "DateTime")) {
        parts.push(d.to_string());
    }
    parts.join("\n")
}

pub(crate) fn parse_iptc(bytes: &[u8]) -> Vec<(String, String)> {
    let Some(irb) = find_iptc_irb(bytes) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i + 5 <= irb.len() {
        if irb[i] != 0x1C {
            i += 1;
            continue;
        }
        let rec = irb[i + 1];
        let ds = irb[i + 2];
        let len = u16::from_be_bytes([irb[i + 3], irb[i + 4]]) as usize;
        i += 5;
        if rec != 2 || i + len > irb.len() {
            continue;
        }
        let text = String::from_utf8_lossy(&irb[i..i + len]).trim().to_string();
        i += len;
        if text.is_empty() {
            continue;
        }
        let label = match ds {
            0x05 => "Title",
            0x69 => "Headline",
            0x78 => "Description",
            0x50 => "Creator",
            0x55 => "Credit",
            0x6E => "Credit",
            0x74 => "Copyright",
            0x19 => "Keywords",
            0x0F => "Category",
            0x14 => "SupplementalCategory",
            0x73 => "Source",
            0x67 => "City",
            0x5A => "Country",
            _ => continue,
        };
        if let Some((_, existing)) = out.iter_mut().find(|(k, _)| k == label) {
            if label == "Keywords" && !existing.split(", ").any(|k| k == text) {
                existing.push_str(", ");
                existing.push_str(&text);
            }
        } else {
            out.push((label.to_string(), text));
        }
    }
    out
}

fn find_iptc_irb(bytes: &[u8]) -> Option<&[u8]> {
    let mut i = 0;
    while i + 12 < bytes.len() {
        if bytes[i] == 0xFF && bytes[i + 1] == 0xED && i + 4 < bytes.len() {
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            let start = i + 4;
            let end = start.saturating_add(len.saturating_sub(2)).min(bytes.len());
            if let Some(found) = iptc_from_photoshop8bim(&bytes[start..end]) {
                return Some(found);
            }
            i = end;
            continue;
        }
        i += 1;
    }
    // Non-JPEG: scan for 8BIM + 0x0404.
    iptc_from_photoshop8bim(bytes)
}

fn iptc_from_photoshop8bim(data: &[u8]) -> Option<&[u8]> {
    let mut i = 0;
    while i + 12 < data.len() {
        if &data[i..i + 4] != b"8BIM" {
            i += 1;
            continue;
        }
        i += 4;
        if i + 2 > data.len() {
            break;
        }
        let id = u16::from_be_bytes([data[i], data[i + 1]]);
        i += 2;
        if i >= data.len() {
            break;
        }
        let name_len = data[i] as usize;
        i += 1 + name_len;
        if i % 2 == 1 {
            i += 1;
        }
        if i + 4 > data.len() {
            break;
        }
        let size = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        i += 4;
        if i + size > data.len() {
            break;
        }
        if id == 0x0404 {
            return Some(&data[i..i + size]);
        }
        i += size;
        if i % 2 == 1 {
            i += 1;
        }
    }
    None
}

pub(crate) fn parse_xmp(bytes: &[u8]) -> Vec<(String, String)> {
    let text = xmp_packet(bytes);
    if text.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    push_xmp(&mut out, "Title", xmp_li(&text, "dc:title"));
    push_xmp(&mut out, "Creator", xmp_li(&text, "dc:creator"));
    push_xmp(&mut out, "Description", xmp_li(&text, "dc:description"));
    push_xmp(&mut out, "Copyright", xmp_li(&text, "dc:rights"));
    push_xmp(&mut out, "Keywords", xmp_all_li(&text, "dc:subject"));
    push_xmp(&mut out, "Headline", xmp_tag(&text, "photoshop:Headline"));
    push_xmp(&mut out, "Credit", xmp_tag(&text, "photoshop:Credit"));
    push_xmp(&mut out, "Created", xmp_tag(&text, "xmp:CreateDate"));
    push_xmp(&mut out, "Modified", xmp_tag(&text, "xmp:ModifyDate"));
    push_xmp(
        &mut out,
        "DateTimeOriginal",
        xmp_tag(&text, "exif:DateTimeOriginal"),
    );
    push_xmp(&mut out, "GPSLatitude", xmp_tag(&text, "exif:GPSLatitude"));
    push_xmp(
        &mut out,
        "GPSLongitude",
        xmp_tag(&text, "exif:GPSLongitude"),
    );
    out
}

fn push_xmp(out: &mut Vec<(String, String)>, key: &str, value: Option<String>) {
    if let Some(v) = value {
        let v = v.trim().to_string();
        if !v.is_empty() {
            out.push((key.to_string(), v));
        }
    }
}

pub(crate) fn xmp_packet(bytes: &[u8]) -> String {
    if let Some(i) = find_slice(bytes, b"<x:xmpmeta") {
        let rest = &bytes[i..];
        if let Some(end) = find_slice(rest, b"</x:xmpmeta>") {
            return String::from_utf8_lossy(&rest[..end + 12]).into_owned();
        }
    }
    if let Some(i) = find_slice(bytes, b"http://ns.adobe.com/xap/1.0/") {
        let rest = &bytes[i..];
        if let Some(xml) = find_slice(rest, b"<") {
            let xml = &rest[xml..];
            if let Some(end) = find_slice(xml, b"</x:xmpmeta>") {
                return String::from_utf8_lossy(&xml[..end + 12]).into_owned();
            }
        }
    }
    String::new()
}

fn find_slice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn xmp_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

fn xmp_li(xml: &str, bag: &str) -> Option<String> {
    let open = format!("<{bag}>");
    let start = xml.find(&open)? + open.len();
    let chunk = &xml[start..];
    let close = format!("</{bag}>");
    let end = chunk.find(&close).unwrap_or(chunk.len().min(800));
    li_first(&chunk[..end])
}

fn xmp_all_li(xml: &str, bag: &str) -> Option<String> {
    let open = format!("<{bag}>");
    let start = xml.find(&open)? + open.len();
    let chunk = &xml[start..];
    let close = format!("</{bag}>");
    let end = chunk.find(&close).unwrap_or(chunk.len().min(2000));
    let items = li_all(&chunk[..end]);
    if items.is_empty() {
        None
    } else {
        Some(items.join(", "))
    }
}

fn li_first(chunk: &str) -> Option<String> {
    li_all(chunk).into_iter().next()
}

fn li_all(chunk: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = chunk;
    while let Some(i) = rest.find("<rdf:li") {
        let after = &rest[i + 7..];
        let Some(gt) = after.find('>') else { break };
        let inner = &after[gt + 1..];
        let Some(end) = inner.find("</rdf:li>") else {
            break;
        };
        let t = inner[..end].trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
        rest = &inner[end + 9..];
    }
    out
}

/// Luma + RGB histograms. Large images are sampled (every Nth pixel).
#[must_use]
pub fn compute_histogram(rgba: &[u8], width: u32, height: u32) -> ChannelHistogram {
    let mut h = ChannelHistogram::default();
    if width == 0 || height == 0 || rgba.len() < 4 {
        return h;
    }
    let n = (width as usize).saturating_mul(height as usize);
    let stride = (n / 200_000).max(1);
    for (i, px) in rgba.chunks_exact(4).enumerate() {
        if i % stride != 0 {
            continue;
        }
        let r = px[0] as usize;
        let g = px[1] as usize;
        let b = px[2] as usize;
        h.r[r] += 1;
        h.g[g] += 1;
        h.b[b] += 1;
        let y = (77 * r + 150 * g + 29 * b) / 256;
        h.luma[y.min(255)] += 1;
    }
    h
}

/// RGBA8 chart (`width` × `height`) for the given mode.
#[must_use]
pub fn render_histogram(
    hist: &ChannelHistogram,
    mode: HistMode,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let w = width.max(32) as usize;
    let h = height.max(16) as usize;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            out[i + 3] = 220;
        }
    }
    match mode {
        HistMode::Luma => paint_channel(&mut out, w, h, &hist.luma, [220, 220, 220]),
        HistMode::Rgb => {
            paint_channel(&mut out, w, h, &hist.r, [220, 60, 60]);
            paint_channel(&mut out, w, h, &hist.g, [60, 200, 80]);
            paint_channel(&mut out, w, h, &hist.b, [70, 120, 230]);
        }
        HistMode::Red => paint_channel(&mut out, w, h, &hist.r, [230, 70, 70]),
        HistMode::Green => paint_channel(&mut out, w, h, &hist.g, [70, 210, 90]),
        HistMode::Blue => paint_channel(&mut out, w, h, &hist.b, [80, 130, 240]),
    }
    out
}

fn paint_channel(out: &mut [u8], w: usize, h: usize, bins: &[u32; 256], rgb: [u8; 3]) {
    let peak = bins.iter().copied().max().unwrap_or(1).max(1);
    for x in 0..w {
        let bin = x * 255 / (w - 1).max(1);
        let t = bins[bin] as f32 / peak as f32;
        let bar = ((h as f32) * t).round() as usize;
        for y in (h.saturating_sub(bar))..h {
            let i = (y * w + x) * 4;
            out[i] = out[i].saturating_add(rgb[0] / 2).max(rgb[0] / 2);
            out[i + 1] = out[i + 1].saturating_add(rgb[1] / 2).max(rgb[1] / 2);
            out[i + 2] = out[i + 2].saturating_add(rgb[2] / 2).max(rgb[2] / 2);
            out[i + 3] = 230;
        }
    }
}

/// Sample one pixel; returns RGB / HEX / HSL / CMYK.
#[must_use]
pub fn describe_pixel(rgba: &[u8], width: u32, height: u32, x: i32, y: i32) -> String {
    if width == 0 || height == 0 {
        return String::new();
    }
    let x = x.clamp(0, width as i32 - 1) as u32;
    let y = y.clamp(0, height as i32 - 1) as u32;
    let i = ((y * width + x) * 4) as usize;
    if i + 3 >= rgba.len() {
        return String::new();
    }
    let r = rgba[i];
    let g = rgba[i + 1];
    let b = rgba[i + 2];
    let a = rgba[i + 3];
    let (h, s, l) = rgb_to_hsl(r, g, b);
    let (c, m, yk, k) = rgb_to_cmyk(r, g, b);
    format!(
        "RGB {r} {g} {b}  A {a}\nHEX #{r:02X}{g:02X}{b:02X}\nHSL {h:.0}° {s:.0}% {l:.0}%\nCMYK {c:.0} {m:.0} {yk:.0} {k:.0}"
    )
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < f32::EPSILON {
        return (0.0, 0.0, l * 100.0);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f32::EPSILON {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s * 100.0, l * 100.0)
}

fn rgb_to_cmyk(r: u8, g: u8, b: u8) -> (f32, f32, f32, f32) {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let k = 1.0 - r.max(g).max(b);
    if (1.0 - k) < f32::EPSILON {
        return (0.0, 0.0, 0.0, 100.0);
    }
    let c = (1.0 - r - k) / (1.0 - k);
    let m = (1.0 - g - k) / (1.0 - k);
    let y = (1.0 - b - k) / (1.0 - k);
    (c * 100.0, m * 100.0, y * 100.0, k * 100.0)
}

/// Bits-per-channel and a short model label from a decoded buffer.
#[must_use]
pub fn color_type_label(img: &image::DynamicImage) -> (u8, &'static str) {
    use image::ColorType::*;
    match img.color() {
        L8 => (8, "L"),
        La8 => (8, "LA"),
        Rgb8 => (8, "RGB"),
        Rgba8 => (8, "RGBA"),
        L16 => (16, "L"),
        La16 => (16, "LA"),
        Rgb16 => (16, "RGB"),
        Rgba16 => (16, "RGBA"),
        Rgb32F => (32, "RGB"),
        Rgba32F => (32, "RGBA"),
        _ => (8, "RGB"),
    }
}

/// # Errors
///
/// Never — kept for call-site symmetry with other report helpers.
pub fn format_inspect_report(path: &Path) -> Result<String> {
    let inspect = inspect_image_file(path)?;
    if inspect.exif.is_empty() && inspect.iptc.is_empty() && inspect.xmp.is_empty() {
        return Err(ViewerError::Metadata("no image metadata".into()));
    }
    Ok(format_inspect_panel(
        0,
        0,
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        "",
        8,
        "",
        &inspect.icc_label,
        "",
        &inspect,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_of_known_bytes() {
        let (md5, sha) = file_hashes(b"abc");
        assert_eq!(md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            sha,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn dms_parses_exif_style() {
        let v = parse_dms("51 deg 28 min 38.00 sec").unwrap();
        assert!((v - 51.47722).abs() < 0.001);
    }

    #[test]
    fn gps_url_contains_coords() {
        let g = GpsFix {
            lat: 51.477,
            lon: -0.0015,
        };
        let url = g.map_url();
        assert!(url.contains("51.477"));
        assert!(url.contains("openstreetmap.org"));
    }

    #[test]
    fn haversine_one_degree_latitude() {
        let a = GpsFix { lat: 0.0, lon: 0.0 };
        let b = GpsFix { lat: 1.0, lon: 0.0 };
        let km = a.distance_km(b);
        assert!((km - 111.2).abs() < 0.5, "{km}");
        assert!(a.distance_km(a) < 1e-9);
    }

    #[test]
    fn overlay_joins_camera_and_exposure() {
        let fields = vec![
            ("Make".into(), "Canon".into()),
            ("Model".into(), "EOS R5".into()),
            ("FNumber".into(), "f/2.8".into()),
            ("ExposureTime".into(), "1/200".into()),
            ("PhotographicSensitivity".into(), "200".into()),
        ];
        let text = camera_overlay(&fields);
        assert!(text.contains("Canon EOS R5"));
        assert!(text.contains("f/2.8"));
        assert!(text.contains("ISO 200"));
    }

    #[test]
    fn iptc_from_synthetic_iim() {
        let mut iim = vec![0x1C, 0x02, 0x05, 0x00, 0x05];
        iim.extend(b"Hello");
        iim.extend([0x1C, 0x02, 0x74, 0x00, 0x03]);
        iim.extend(b"(C)");
        let mut ps = b"8BIM".to_vec();
        ps.extend([0x04, 0x04, 0x00]); // id + empty name
                                       // name pad to even: name_len 0, already even after +1? 4+2+1=7, pad 1
        ps.push(0x00);
        let size = iim.len() as u32;
        ps.extend(size.to_be_bytes());
        ps.extend(iim);
        let fields = parse_iptc(&ps);
        assert!(fields.iter().any(|(k, v)| k == "Title" && v == "Hello"));
        assert!(fields.iter().any(|(k, v)| k == "Copyright" && v == "(C)"));
    }

    #[test]
    fn xmp_extracts_dc_title() {
        let xml = br#"<x:xmpmeta><rdf:RDF>
            <dc:title><rdf:Alt><rdf:li>Sunset</rdf:li></rdf:Alt></dc:title>
            <dc:creator><rdf:Seq><rdf:li>Ada</rdf:li></rdf:Seq></dc:creator>
            </rdf:RDF></x:xmpmeta>"#;
        let fields = parse_xmp(xml);
        assert!(fields.iter().any(|(k, v)| k == "Title" && v == "Sunset"));
        assert!(fields.iter().any(|(k, v)| k == "Creator" && v == "Ada"));
    }

    #[test]
    fn histogram_peaks_on_red() {
        let px = [255u8, 0, 0, 255, 255, 0, 0, 255];
        let h = compute_histogram(&px, 2, 1);
        assert!(h.r[255] >= 2);
        assert_eq!(h.g[0], 2);
        let img = render_histogram(&h, HistMode::Red, 64, 32);
        assert_eq!(img.len(), 64 * 32 * 4);
    }

    #[test]
    fn pixel_formats_include_hex_and_cmyk() {
        let px = [255u8, 0, 0, 255];
        let t = describe_pixel(&px, 1, 1, 0, 0);
        assert!(t.contains("#FF0000"));
        assert!(t.contains("CMYK"));
        assert!(t.contains("HSL"));
    }

    #[test]
    fn hist_mode_cycles_five() {
        let mut m = HistMode::Luma;
        let mut n = 0;
        for _ in 0..5 {
            n += 1;
            m = m.cycle();
        }
        assert_eq!(n, 5);
        assert_eq!(m, HistMode::Luma);
    }
}
