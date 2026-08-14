//! Write IPTC / XMP / GPS / dates, strip metadata, copy, CSV/XML, templates.

use std::path::{Path, PathBuf};

use chrono::{NaiveDateTime, TimeDelta};

use crate::error::{Result, ViewerError};
use crate::image::metadata::{
    field, inspect_image_file, parse_iptc, parse_xmp, sidecar_xmp_path, GpsFix, ImageInspect,
};

/// Editable descriptive + location + date fields.
#[derive(Debug, Clone, Default, PartialEq)]
#[allow(missing_docs)]
pub struct EditableMeta {
    pub title: Option<String>,
    pub headline: Option<String>,
    pub description: Option<String>,
    pub creator: Option<String>,
    pub copyright: Option<String>,
    pub keywords: Option<String>,
    pub credit: Option<String>,
    /// `None` leave, `Some(None)` clear, `Some(Some(fix))` set.
    pub gps: Option<Option<GpsFix>>,
    /// EXIF-style `YYYY:MM:DD HH:MM:SS`. `Some(None)` clears XMP date.
    pub date: Option<Option<String>>,
    /// Add this many seconds to existing DateTimeOriginal / XMP dates.
    pub date_shift_secs: Option<i64>,
    pub strip_all: bool,
    pub strip_gps: bool,
}

/// Named reusable patch stored as JSON.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct MetaTemplate {
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub copyright: String,
    #[serde(default)]
    pub keywords: String,
    #[serde(default)]
    pub credit: String,
}

impl MetaTemplate {
    /// Convert filled fields into a patch (empty strings are omitted).
    #[must_use]
    pub fn to_edit(&self) -> EditableMeta {
        let mut e = EditableMeta::default();
        set_if(&mut e.title, &self.title);
        set_if(&mut e.headline, &self.headline);
        set_if(&mut e.description, &self.description);
        set_if(&mut e.creator, &self.creator);
        set_if(&mut e.copyright, &self.copyright);
        set_if(&mut e.keywords, &self.keywords);
        set_if(&mut e.credit, &self.credit);
        e
    }
}

fn set_if(slot: &mut Option<String>, v: &str) {
    if !v.is_empty() {
        *slot = Some(v.to_string());
    }
}

/// Apply [`EditableMeta`] to a local image (JPEG rewrite and/or `.xmp` sidecar).
///
/// # Errors
///
/// I/O or an unreadable image.
pub fn apply_editable_meta(path: &Path, edit: &EditableMeta) -> Result<()> {
    let bytes = std::fs::read(path)?;
    if is_jpeg(&bytes) {
        let out = rewrite_jpeg(&bytes, edit, path)?;
        std::fs::write(path, out)?;
    } else {
        write_sidecar_xmp(path, edit)?;
    }
    Ok(())
}

/// Copy IPTC / XMP / EXIF APP segments (or sidecar) from `from` onto `to`.
///
/// # Errors
///
/// I/O.
pub fn copy_image_metadata(from: &Path, to: &Path) -> Result<()> {
    let src = std::fs::read(from)?;
    let dest = std::fs::read(to)?;
    if is_jpeg(&src) && is_jpeg(&dest) {
        let copied = copy_jpeg_meta(&src, &dest)?;
        std::fs::write(to, copied)?;
        return Ok(());
    }
    let inspect = inspect_image_file(from)?;
    let edit = inspect_to_edit(&inspect);
    apply_editable_meta(to, &edit)
}

/// CSV header + one row per path.
///
/// # Errors
///
/// I/O while inspecting.
pub fn export_metadata_csv(paths: &[PathBuf]) -> Result<String> {
    let mut out = String::from(
        "path,title,headline,description,creator,copyright,keywords,credit,lat,lon,date\n",
    );
    for p in paths {
        let ins = inspect_image_file(p).unwrap_or_default();
        let e = inspect_to_edit(&ins);
        out.push_str(&csv_escape(&p.display().to_string()));
        out.push(',');
        out.push_str(&csv_escape(e.title.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_escape(e.headline.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_escape(e.description.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_escape(e.creator.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_escape(e.copyright.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_escape(e.keywords.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_escape(e.credit.as_deref().unwrap_or("")));
        out.push(',');
        if let Some(g) = ins.gps {
            out.push_str(&format!("{:.6},{:.6}", g.lat, g.lon));
        } else {
            out.push(',');
        }
        out.push(',');
        out.push_str(&csv_escape(
            e.date.clone().flatten().as_deref().unwrap_or(""),
        ));
        out.push('\n');
    }
    Ok(out)
}

/// Simple XML dump of the same columns.
///
/// # Errors
///
/// I/O while inspecting.
pub fn export_metadata_xml(paths: &[PathBuf]) -> Result<String> {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<images>\n");
    for p in paths {
        let ins = inspect_image_file(p).unwrap_or_default();
        let e = inspect_to_edit(&ins);
        out.push_str("  <image path=\"");
        out.push_str(&xml_esc(&p.display().to_string()));
        out.push_str("\">\n");
        xml_tag(&mut out, "title", e.title.as_deref());
        xml_tag(&mut out, "headline", e.headline.as_deref());
        xml_tag(&mut out, "description", e.description.as_deref());
        xml_tag(&mut out, "creator", e.creator.as_deref());
        xml_tag(&mut out, "copyright", e.copyright.as_deref());
        xml_tag(&mut out, "keywords", e.keywords.as_deref());
        xml_tag(&mut out, "credit", e.credit.as_deref());
        if let Some(g) = ins.gps {
            xml_tag(&mut out, "lat", Some(&format!("{:.6}", g.lat)));
            xml_tag(&mut out, "lon", Some(&format!("{:.6}", g.lon)));
        }
        xml_tag(&mut out, "date", e.date.clone().flatten().as_deref());
        out.push_str("  </image>\n");
    }
    out.push_str("</images>\n");
    Ok(out)
}

/// Parse a CSV produced by [`export_metadata_csv`] (or the same columns).
///
/// # Errors
///
/// Empty input.
pub fn import_metadata_csv(csv: &str) -> Result<Vec<(String, EditableMeta)>> {
    let mut lines = csv.lines();
    let header = lines
        .next()
        .ok_or_else(|| ViewerError::Metadata("empty CSV".into()))?;
    let _ = header;
    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cols = split_csv(line);
        if cols.is_empty() {
            continue;
        }
        let mut e = EditableMeta::default();
        e.title = nonempty(cols.get(1));
        e.headline = nonempty(cols.get(2));
        e.description = nonempty(cols.get(3));
        e.creator = nonempty(cols.get(4));
        e.copyright = nonempty(cols.get(5));
        e.keywords = nonempty(cols.get(6));
        e.credit = nonempty(cols.get(7));
        let lat = cols.get(8).and_then(|s| s.parse().ok());
        let lon = cols.get(9).and_then(|s| s.parse().ok());
        if let (Some(lat), Some(lon)) = (lat, lon) {
            e.gps = Some(Some(GpsFix { lat, lon }));
        }
        e.date = nonempty(cols.get(10)).map(Some);
        rows.push((cols[0].clone(), e));
    }
    Ok(rows)
}

/// `key=value` lines for the FM prompt.
#[must_use]
pub fn pack_editable_meta(e: &EditableMeta) -> String {
    let mut lines = Vec::new();
    pack_line(&mut lines, "title", e.title.as_deref());
    pack_line(&mut lines, "headline", e.headline.as_deref());
    pack_line(&mut lines, "description", e.description.as_deref());
    pack_line(&mut lines, "creator", e.creator.as_deref());
    pack_line(&mut lines, "copyright", e.copyright.as_deref());
    pack_line(&mut lines, "keywords", e.keywords.as_deref());
    pack_line(&mut lines, "credit", e.credit.as_deref());
    match &e.gps {
        Some(Some(g)) => lines.push(format!("gps={},{}", g.lat, g.lon)),
        Some(None) => lines.push("gps=".into()),
        None => {}
    }
    match &e.date {
        Some(Some(d)) => lines.push(format!("date={d}")),
        Some(None) => lines.push("date=".into()),
        None => {}
    }
    if let Some(s) = e.date_shift_secs {
        lines.push(format!("shift={s}"));
    }
    lines.join("\n")
}

/// Inverse of [`pack_editable_meta`].
#[must_use]
pub fn unpack_editable_meta(raw: &str) -> EditableMeta {
    let mut e = EditableMeta::default();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k.trim().to_ascii_lowercase().as_str() {
            "title" => e.title = Some(v.to_string()),
            "headline" => e.headline = Some(v.to_string()),
            "description" => e.description = Some(v.to_string()),
            "creator" | "author" => e.creator = Some(v.to_string()),
            "copyright" => e.copyright = Some(v.to_string()),
            "keywords" => e.keywords = Some(v.to_string()),
            "credit" => e.credit = Some(v.to_string()),
            "gps" => {
                e.gps = if v.is_empty() {
                    Some(None)
                } else {
                    parse_gps_pair(v).map(Some)
                };
            }
            "date" => {
                e.date = if v.is_empty() {
                    Some(None)
                } else {
                    Some(Some(normalize_date(v).unwrap_or_else(|| v.to_string())))
                };
            }
            "shift" => e.date_shift_secs = parse_shift(v),
            _ => {}
        }
    }
    e
}

/// `+1h`, `-2d`, `+30m`, or raw seconds.
#[must_use]
pub fn parse_shift(raw: &str) -> Option<i64> {
    let s = raw.trim();
    let (sign, rest) = if let Some(r) = s.strip_prefix('+') {
        (1i64, r)
    } else if let Some(r) = s.strip_prefix('-') {
        (-1, r)
    } else {
        (1, s)
    };
    let rest = rest.trim();
    if let Some(n) = rest.strip_suffix(['d', 'D']) {
        return n.trim().parse::<i64>().ok().map(|v| sign * v * 86_400);
    }
    if let Some(n) = rest.strip_suffix(['h', 'H']) {
        return n.trim().parse::<i64>().ok().map(|v| sign * v * 3_600);
    }
    if let Some(n) = rest.strip_suffix(['m', 'M']) {
        return n.trim().parse::<i64>().ok().map(|v| sign * v * 60);
    }
    rest.parse::<i64>().ok().map(|v| sign * v)
}

/// `lat,lon` pair.
#[must_use]
pub fn parse_gps_pair(raw: &str) -> Option<GpsFix> {
    let (a, b) = raw.split_once(',')?;
    let lat = a.trim().parse().ok()?;
    let lon = b.trim().parse().ok()?;
    Some(GpsFix { lat, lon })
}

/// Load templates from `dir/orchid-meta-templates.json`.
#[must_use]
pub fn load_templates(dir: &Path) -> Vec<MetaTemplate> {
    let path = dir.join("orchid-meta-templates.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Insert or replace a template in `dir/orchid-meta-templates.json`.
///
/// # Errors
///
/// I/O.
pub fn save_template(dir: &Path, tmpl: MetaTemplate) -> Result<()> {
    let path = dir.join("orchid-meta-templates.json");
    let mut all = load_templates(dir);
    if let Some(existing) = all.iter_mut().find(|t| t.name == tmpl.name) {
        *existing = tmpl;
    } else {
        all.push(tmpl);
    }
    let json = serde_json::to_vec_pretty(&all).map_err(|e| ViewerError::Metadata(e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Build an edit from the current inspect (for prompts / copy).
#[must_use]
pub fn inspect_to_edit(ins: &ImageInspect) -> EditableMeta {
    let pick = |names: &[&str]| {
        for n in names {
            if let Some(v) = field(&ins.iptc, n)
                .or_else(|| field(&ins.xmp, n))
                .or_else(|| field(&ins.exif, n))
            {
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
        None
    };
    EditableMeta {
        title: pick(&["Title"]),
        headline: pick(&["Headline"]),
        description: pick(&["Description"]),
        creator: pick(&["Creator", "Artist"]),
        copyright: pick(&["Copyright", "CopyrightNotice"]),
        keywords: pick(&["Keywords"]),
        credit: pick(&["Credit"]),
        gps: ins.gps.map(Some),
        date: pick(&["DateTimeOriginal", "DateTime", "Created"]).map(Some),
        date_shift_secs: None,
        strip_all: false,
        strip_gps: false,
    }
}

fn pack_line(lines: &mut Vec<String>, key: &str, v: Option<&str>) {
    if let Some(v) = v {
        lines.push(format!("{key}={v}"));
    }
}

fn nonempty(v: Option<&String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == ',' {
            out.push(cur);
            cur = String::new();
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xml_tag(out: &mut String, name: &str, v: Option<&str>) {
    if let Some(v) = v.filter(|s| !s.is_empty()) {
        out.push_str("    <");
        out.push_str(name);
        out.push('>');
        out.push_str(&xml_esc(v));
        out.push_str("</");
        out.push_str(name);
        out.push_str(">\n");
    }
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8
}

fn rewrite_jpeg(bytes: &[u8], edit: &EditableMeta, path: &Path) -> Result<Vec<u8>> {
    let (mut segs, tail) = split_jpeg(bytes)?;
    if edit.strip_all {
        segs.retain(|s| !is_meta_segment(s));
        write_sidecar_cleared(path)?;
        return Ok(join_jpeg(&segs, &tail));
    }
    if edit.strip_gps {
        for s in &mut segs {
            if s.marker == 0xE1 && s.data.starts_with(b"Exif\0\0") {
                strip_exif_gps_ifd(&mut s.data);
            }
        }
    }
    let current = current_from_jpeg(bytes);
    let merged = merge_edit(&current, edit);
    segs.retain(|s| {
        !(s.marker == 0xE1 && is_xmp_app1(&s.data)) && !(s.marker == 0xED && is_iptc_app13(&s.data))
    });
    if !edit.strip_all {
        segs.insert(
            insert_at(&segs),
            Seg {
                marker: 0xE1,
                data: build_xmp_app1(&merged),
            },
        );
        segs.insert(
            insert_at(&segs),
            Seg {
                marker: 0xED,
                data: build_iptc_app13(&merged),
            },
        );
    }
    if let Some(Some(date)) = &merged.date {
        for s in &mut segs {
            if s.marker == 0xE1 && s.data.starts_with(b"Exif\0\0") {
                patch_exif_datetimes(&mut s.data, date);
            }
        }
    }
    if let Some(delta) = edit.date_shift_secs {
        for s in &mut segs {
            if s.marker == 0xE1 && s.data.starts_with(b"Exif\0\0") {
                shift_exif_datetimes(&mut s.data, delta);
            }
        }
    }
    if matches!(edit.gps, Some(None)) || edit.strip_gps {
        for s in &mut segs {
            if s.marker == 0xE1 && s.data.starts_with(b"Exif\0\0") {
                strip_exif_gps_ifd(&mut s.data);
            }
        }
    }
    Ok(join_jpeg(&segs, &tail))
}

fn copy_jpeg_meta(src: &[u8], dest: &[u8]) -> Result<Vec<u8>> {
    let (src_segs, _) = split_jpeg(src)?;
    let (mut dest_segs, tail) = split_jpeg(dest)?;
    dest_segs.retain(|s| !is_meta_segment(s));
    let meta: Vec<Seg> = src_segs.into_iter().filter(is_meta_segment).collect();
    let at = insert_at(&dest_segs);
    for (i, s) in meta.into_iter().enumerate() {
        dest_segs.insert(at + i, s);
    }
    Ok(join_jpeg(&dest_segs, &tail))
}

fn is_meta_segment(s: &Seg) -> bool {
    (s.marker == 0xE1 && (s.data.starts_with(b"Exif\0\0") || is_xmp_app1(&s.data)))
        || (s.marker == 0xED && is_iptc_app13(&s.data))
}

fn is_xmp_app1(data: &[u8]) -> bool {
    data.starts_with(b"http://ns.adobe.com/xap/1.0/")
}

fn is_iptc_app13(data: &[u8]) -> bool {
    data.starts_with(b"Photoshop 3.0\0")
}

fn insert_at(segs: &[Seg]) -> usize {
    segs.iter()
        .position(|s| s.marker == 0xE0)
        .map(|i| i + 1)
        .unwrap_or(0)
}

struct Seg {
    marker: u8,
    data: Vec<u8>,
}

fn split_jpeg(bytes: &[u8]) -> Result<(Vec<Seg>, Vec<u8>)> {
    if !is_jpeg(bytes) {
        return Err(ViewerError::Metadata("not a JPEG".into()));
    }
    let mut i = 2;
    let mut segs = Vec::new();
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            return Err(ViewerError::Metadata("corrupt JPEG markers".into()));
        }
        let marker = bytes[i + 1];
        if marker == 0xDA {
            return Ok((segs, bytes[i..].to_vec()));
        }
        if marker == 0xD9 {
            return Ok((segs, bytes[i..].to_vec()));
        }
        if marker == 0x00 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if len < 2 || i + 2 + len > bytes.len() {
            return Err(ViewerError::Metadata("truncated JPEG segment".into()));
        }
        let data = bytes[i + 4..i + 2 + len].to_vec();
        segs.push(Seg { marker, data });
        i += 2 + len;
    }
    Ok((segs, Vec::new()))
}

fn join_jpeg(segs: &[Seg], tail: &[u8]) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    for s in segs {
        out.push(0xFF);
        out.push(s.marker);
        let len = (s.data.len() + 2) as u16;
        out.extend(len.to_be_bytes());
        out.extend(&s.data);
    }
    out.extend(tail);
    out
}

fn current_from_jpeg(bytes: &[u8]) -> EditableMeta {
    let iptc = parse_iptc(bytes);
    let xmp = parse_xmp(bytes);
    let pick = |n: &str| {
        field(&iptc, n)
            .or_else(|| field(&xmp, n))
            .map(str::to_string)
    };
    EditableMeta {
        title: pick("Title"),
        headline: pick("Headline"),
        description: pick("Description"),
        creator: pick("Creator"),
        copyright: pick("Copyright"),
        keywords: pick("Keywords"),
        credit: pick("Credit"),
        gps: crate::image::metadata::gps_from_xmp_fields(&xmp).map(Some),
        date: field(&xmp, "DateTimeOriginal")
            .or_else(|| field(&xmp, "Created"))
            .map(|s| Some(s.to_string())),
        ..EditableMeta::default()
    }
}

fn merge_edit(base: &EditableMeta, edit: &EditableMeta) -> EditableMeta {
    let mut m = base.clone();
    if let Some(v) = &edit.title {
        m.title = Some(v.clone());
    }
    if let Some(v) = &edit.headline {
        m.headline = Some(v.clone());
    }
    if let Some(v) = &edit.description {
        m.description = Some(v.clone());
    }
    if let Some(v) = &edit.creator {
        m.creator = Some(v.clone());
    }
    if let Some(v) = &edit.copyright {
        m.copyright = Some(v.clone());
    }
    if let Some(v) = &edit.keywords {
        m.keywords = Some(v.clone());
    }
    if let Some(v) = &edit.credit {
        m.credit = Some(v.clone());
    }
    if let Some(g) = &edit.gps {
        m.gps = Some(*g);
    }
    if edit.strip_gps {
        m.gps = Some(None);
    }
    if let Some(d) = &edit.date {
        m.date = Some(d.clone());
    }
    if let Some(delta) = edit.date_shift_secs {
        if let Some(Some(cur)) = &m.date {
            if let Some(next) = shift_date_str(cur, delta) {
                m.date = Some(Some(next));
            }
        }
    }
    m
}

fn build_xmp_app1(e: &EditableMeta) -> Vec<u8> {
    let mut xml = String::from(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:photoshop=\"http://ns.adobe.com/photoshop/1.0/\" \
         xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" \
         xmlns:exif=\"http://ns.adobe.com/exif/1.0/\">\
         <rdf:Description rdf:about=\"\">",
    );
    bag(&mut xml, "dc:title", e.title.as_deref());
    bag(&mut xml, "dc:description", e.description.as_deref());
    seq(&mut xml, "dc:creator", e.creator.as_deref());
    bag(&mut xml, "dc:rights", e.copyright.as_deref());
    subjects(&mut xml, e.keywords.as_deref());
    simple(&mut xml, "photoshop:Headline", e.headline.as_deref());
    simple(&mut xml, "photoshop:Credit", e.credit.as_deref());
    if let Some(Some(d)) = &e.date {
        simple(&mut xml, "xmp:CreateDate", Some(d));
        simple(&mut xml, "exif:DateTimeOriginal", Some(d));
    }
    if let Some(Some(g)) = e.gps {
        simple(&mut xml, "exif:GPSLatitude", Some(&gps_xmp_lat(g.lat)));
        simple(&mut xml, "exif:GPSLongitude", Some(&gps_xmp_lon(g.lon)));
    }
    xml.push_str("</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>");
    let mut data = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    data.extend(xml.as_bytes());
    data
}

fn bag(xml: &mut String, tag: &str, v: Option<&str>) {
    if let Some(v) = v.filter(|s| !s.is_empty()) {
        xml.push_str(&format!(
            "<{tag}><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></{tag}>",
            xml_esc(v)
        ));
    }
}

fn seq(xml: &mut String, tag: &str, v: Option<&str>) {
    if let Some(v) = v.filter(|s| !s.is_empty()) {
        xml.push_str(&format!(
            "<{tag}><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></{tag}>",
            xml_esc(v)
        ));
    }
}

fn subjects(xml: &mut String, v: Option<&str>) {
    let Some(v) = v.filter(|s| !s.is_empty()) else {
        return;
    };
    xml.push_str("<dc:subject><rdf:Bag>");
    for k in v.split(',') {
        let k = k.trim();
        if !k.is_empty() {
            xml.push_str(&format!("<rdf:li>{}</rdf:li>", xml_esc(k)));
        }
    }
    xml.push_str("</rdf:Bag></dc:subject>");
}

fn simple(xml: &mut String, tag: &str, v: Option<&str>) {
    if let Some(v) = v.filter(|s| !s.is_empty()) {
        xml.push_str(&format!("<{tag}>{}</{tag}>", xml_esc(v)));
    }
}

fn gps_xmp_lat(lat: f64) -> String {
    let hemi = if lat < 0.0 { 'S' } else { 'N' };
    format!("{:.6}{hemi}", lat.abs())
}

fn gps_xmp_lon(lon: f64) -> String {
    let hemi = if lon < 0.0 { 'W' } else { 'E' };
    format!("{:.6}{hemi}", lon.abs())
}

fn build_iptc_app13(e: &EditableMeta) -> Vec<u8> {
    let mut iim = Vec::new();
    iim_ds(&mut iim, 0x05, e.title.as_deref());
    iim_ds(&mut iim, 0x69, e.headline.as_deref());
    iim_ds(&mut iim, 0x78, e.description.as_deref());
    iim_ds(&mut iim, 0x50, e.creator.as_deref());
    iim_ds(&mut iim, 0x74, e.copyright.as_deref());
    iim_ds(&mut iim, 0x6E, e.credit.as_deref());
    if let Some(keys) = e.keywords.as_deref() {
        for k in keys.split(',') {
            iim_ds(&mut iim, 0x19, Some(k.trim()));
        }
    }
    let mut ps = b"Photoshop 3.0\0".to_vec();
    ps.extend(b"8BIM");
    ps.extend([0x04, 0x04, 0x00, 0x00]);
    let size = iim.len() as u32;
    ps.extend(size.to_be_bytes());
    ps.extend(iim);
    if ps.len() % 2 == 1 {
        ps.push(0);
    }
    ps
}

fn iim_ds(out: &mut Vec<u8>, ds: u8, text: Option<&str>) {
    let Some(text) = text.filter(|s| !s.is_empty()) else {
        return;
    };
    let bytes = text.as_bytes();
    if bytes.len() > 0xFFFF {
        return;
    }
    out.extend([0x1C, 0x02, ds]);
    out.extend((bytes.len() as u16).to_be_bytes());
    out.extend(bytes);
}

fn write_sidecar_xmp(path: &Path, edit: &EditableMeta) -> Result<()> {
    let dest =
        sidecar_xmp_path(path).ok_or_else(|| ViewerError::Metadata("invalid path".into()))?;
    if edit.strip_all {
        let _ = std::fs::remove_file(&dest);
        return Ok(());
    }
    let existing = std::fs::read(&dest).unwrap_or_default();
    let mut base = EditableMeta::default();
    if !existing.is_empty() {
        let xmp = parse_xmp(&existing);
        base.title = field(&xmp, "Title").map(str::to_string);
        base.headline = field(&xmp, "Headline").map(str::to_string);
        base.description = field(&xmp, "Description").map(str::to_string);
        base.creator = field(&xmp, "Creator").map(str::to_string);
        base.copyright = field(&xmp, "Copyright").map(str::to_string);
        base.keywords = field(&xmp, "Keywords").map(str::to_string);
        base.credit = field(&xmp, "Credit").map(str::to_string);
        base.gps = crate::image::metadata::gps_from_xmp_fields(&xmp).map(Some);
        base.date = field(&xmp, "DateTimeOriginal")
            .or_else(|| field(&xmp, "Created"))
            .map(|s| Some(s.to_string()));
    }
    let merged = merge_edit(&base, edit);
    let app1 = build_xmp_app1(&merged);
    let xml = app1
        .strip_prefix(b"http://ns.adobe.com/xap/1.0/\0")
        .unwrap_or(&app1);
    std::fs::write(dest, xml)?;
    Ok(())
}

fn write_sidecar_cleared(path: &Path) -> Result<()> {
    if let Some(p) = sidecar_xmp_path(path) {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

fn patch_exif_datetimes(app1: &mut [u8], date: &str) {
    let stamp = match normalize_date(date) {
        Some(s) if s.len() == 19 => s,
        _ => return,
    };
    let newb = stamp.as_bytes();
    let mut i = 0;
    while i + 19 <= app1.len() {
        if looks_like_exif_date(&app1[i..i + 19]) {
            app1[i..i + 19].copy_from_slice(newb);
            i += 19;
        } else {
            i += 1;
        }
    }
}

fn shift_exif_datetimes(app1: &mut [u8], delta: i64) {
    let mut i = 0;
    while i + 19 <= app1.len() {
        if looks_like_exif_date(&app1[i..i + 19]) {
            let cur = String::from_utf8_lossy(&app1[i..i + 19]).into_owned();
            if let Some(next) = shift_date_str(&cur, delta) {
                app1[i..i + 19].copy_from_slice(next.as_bytes());
            }
            i += 19;
        } else {
            i += 1;
        }
    }
}

fn looks_like_exif_date(b: &[u8]) -> bool {
    b.len() == 19
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b' '
        && b[13] == b':'
        && b[16] == b':'
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 | 10 | 13 | 16 => true,
            _ => c.is_ascii_digit(),
        })
}

fn normalize_date(raw: &str) -> Option<String> {
    let t = raw.trim().replace('T', " ").replace('-', ":");
    let t = if t.len() == 10 {
        format!("{t} 00:00:00")
    } else {
        t
    };
    let t = if t.len() >= 19 { &t[..19] } else { t.as_str() };
    NaiveDateTime::parse_from_str(t, "%Y:%m:%d %H:%M:%S")
        .ok()
        .map(|d| d.format("%Y:%m:%d %H:%M:%S").to_string())
}

fn shift_date_str(raw: &str, secs: i64) -> Option<String> {
    let cur = normalize_date(raw)?;
    let dt = NaiveDateTime::parse_from_str(&cur, "%Y:%m:%d %H:%M:%S").ok()?;
    let next = dt.checked_add_signed(TimeDelta::seconds(secs))?;
    Some(next.format("%Y:%m:%d %H:%M:%S").to_string())
}

/// Zero the GPS IFD pointer (tag 0x8825) in IFD0 when present.
fn strip_exif_gps_ifd(app1: &mut [u8]) {
    if app1.len() < 20 || !app1.starts_with(b"Exif\0\0") {
        return;
    }
    let tiff = &app1[6..];
    let le = tiff.starts_with(b"II");
    let be = tiff.starts_with(b"MM");
    if !le && !be {
        return;
    }
    let u16_at = |b: &[u8], o: usize| -> Option<u16> {
        let s = b.get(o..o + 2)?;
        Some(if le {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            u16::from_be_bytes([s[0], s[1]])
        })
    };
    let u32_at = |b: &[u8], o: usize| -> Option<u32> {
        let s = b.get(o..o + 4)?;
        Some(if le {
            u32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            u32::from_be_bytes([s[0], s[1], s[2], s[3]])
        })
    };
    let ifd0 = u32_at(tiff, 4).unwrap_or(8) as usize;
    let count = u16_at(tiff, ifd0).unwrap_or(0) as usize;
    let mut offsets = Vec::new();
    for n in 0..count {
        let e = ifd0 + 2 + n * 12;
        if u16_at(tiff, e) == Some(0x8825) {
            offsets.push(6 + e + 8);
        }
    }
    for abs in offsets {
        if abs + 4 <= app1.len() {
            app1[abs..abs + 4].fill(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    fn tiny_jpeg() -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([20, 40, 80])));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)
            .unwrap();
        buf
    }

    #[test]
    fn pack_roundtrip() {
        let e = EditableMeta {
            title: Some("Sunset".into()),
            creator: Some("Ada".into()),
            gps: Some(Some(GpsFix {
                lat: 51.5,
                lon: -0.1,
            })),
            date: Some(Some("2024:06:01 12:00:00".into())),
            ..EditableMeta::default()
        };
        let back = unpack_editable_meta(&pack_editable_meta(&e));
        assert_eq!(back.title, e.title);
        assert_eq!(back.creator, e.creator);
        assert!(back.gps.unwrap().unwrap().lat > 51.0);
    }

    #[test]
    fn shift_parses_units() {
        assert_eq!(parse_shift("+2h"), Some(7200));
        assert_eq!(parse_shift("-1d"), Some(-86_400));
        assert_eq!(parse_shift("30"), Some(30));
    }

    #[test]
    fn jpeg_iptc_xmp_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.jpg");
        std::fs::write(&path, tiny_jpeg()).unwrap();
        apply_editable_meta(
            &path,
            &EditableMeta {
                title: Some("Hello".into()),
                creator: Some("Ada".into()),
                copyright: Some("(C) Ada".into()),
                keywords: Some("sea, dusk".into()),
                gps: Some(Some(GpsFix {
                    lat: 10.5,
                    lon: 20.25,
                })),
                date: Some(Some("2024:01:02 03:04:05".into())),
                ..EditableMeta::default()
            },
        )
        .unwrap();
        let ins = inspect_image_file(&path).unwrap();
        assert!(ins.iptc.iter().any(|(k, v)| k == "Title" && v == "Hello"));
        assert!(ins.xmp.iter().any(|(k, v)| k == "Creator" && v == "Ada"));
        let g = ins.gps.expect("gps");
        assert!((g.lat - 10.5).abs() < 0.01);
        assert!((g.lon - 20.25).abs() < 0.01);
    }

    #[test]
    fn strip_all_removes_iptc() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.jpg");
        std::fs::write(&path, tiny_jpeg()).unwrap();
        apply_editable_meta(
            &path,
            &EditableMeta {
                title: Some("X".into()),
                ..EditableMeta::default()
            },
        )
        .unwrap();
        apply_editable_meta(
            &path,
            &EditableMeta {
                strip_all: true,
                ..EditableMeta::default()
            },
        )
        .unwrap();
        let ins = inspect_image_file(&path).unwrap();
        assert!(ins.iptc.is_empty());
        assert!(ins.xmp.is_empty());
    }

    #[test]
    fn csv_import_export() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.jpg");
        std::fs::write(&path, tiny_jpeg()).unwrap();
        apply_editable_meta(
            &path,
            &EditableMeta {
                title: Some("CSV".into()),
                creator: Some("Bob".into()),
                ..EditableMeta::default()
            },
        )
        .unwrap();
        let csv = export_metadata_csv(&[path.clone()]).unwrap();
        assert!(csv.contains("CSV"));
        let rows = import_metadata_csv(&csv).unwrap();
        assert_eq!(rows[0].1.title.as_deref(), Some("CSV"));
        let xml = export_metadata_xml(&[path]).unwrap();
        assert!(xml.contains("<title>CSV</title>"));
    }

    #[test]
    fn copy_between_jpegs() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("src.jpg");
        let b = dir.path().join("dst.jpg");
        std::fs::write(&a, tiny_jpeg()).unwrap();
        std::fs::write(&b, tiny_jpeg()).unwrap();
        apply_editable_meta(
            &a,
            &EditableMeta {
                title: Some("Copied".into()),
                ..EditableMeta::default()
            },
        )
        .unwrap();
        copy_image_metadata(&a, &b).unwrap();
        let ins = inspect_image_file(&b).unwrap();
        assert!(ins.iptc.iter().any(|(k, v)| k == "Title" && v == "Copied"));
    }

    #[test]
    fn date_shift_moves_clock() {
        let next = shift_date_str("2024:01:01 00:00:00", 3600).unwrap();
        assert_eq!(next, "2024:01:01 01:00:00");
    }

    #[test]
    fn template_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        save_template(
            dir.path(),
            MetaTemplate {
                name: "press".into(),
                creator: "Desk".into(),
                copyright: "News".into(),
                ..MetaTemplate::default()
            },
        )
        .unwrap();
        let all = load_templates(dir.path());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].creator, "Desk");
    }
}
