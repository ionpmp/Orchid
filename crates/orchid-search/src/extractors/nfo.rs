//! Media NFO extractor.
//!
//! Kodi-style XML contributes titles, plots, and people. Poster URLs and
//! stream details are skipped. A plain-text scene NFO is indexed as text.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::epub_odf::html_text;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from `.nfo` media descriptions.
#[derive(Debug, Default, Clone, Copy)]
pub struct NfoExtractor;

#[async_trait]
impl ContentExtractor for NfoExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e.eq_ignore_ascii_case("nfo"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(nfo_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn nfo_text(input: &str) -> String {
    if looks_like_kodi(input) {
        kodi_text(input)
    } else {
        plain_nfo(input)
    }
}

fn looks_like_kodi(input: &str) -> bool {
    let probe: String = input.chars().take(2500).collect();
    let probe = probe.to_ascii_lowercase();
    [
        "<movie",
        "<tvshow",
        "<episodedetails",
        "<musicvideo",
        "<artist",
        "<album",
    ]
    .into_iter()
    .any(|tag| has_open_tag(&probe, tag))
}

fn has_open_tag(probe: &str, tag: &str) -> bool {
    let mut rest = probe;
    while let Some(index) = rest.find(tag) {
        let after = index + tag.len();
        let boundary = rest[after..].chars().next();
        if matches!(boundary, None | Some('>' | ' ' | '\t' | '\n' | '\r' | '/')) {
            return true;
        }
        rest = &rest[after..];
    }
    false
}

fn kodi_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut capture: Option<Capture> = None;
    let mut skip = 0i32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 || (capture.is_some() && (local == "script" || local == "style")) {
                    skip += 1;
                } else if let Some(current) = capture.as_mut() {
                    if local == current.tag {
                        current.depth += 1;
                    }
                } else if is_text_tag(&local) {
                    capture = Some(Capture::new(local));
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        current.buf.push_str(t.as_ref());
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        current.buf.push_str(t.as_ref());
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        current.buf.push(decode_ref(r.as_ref()));
                    }
                }
            }
            Ok(Event::End(e)) => {
                if skip > 0 {
                    skip -= 1;
                } else {
                    let local = local_name(e.name().as_ref());
                    let done = capture
                        .as_ref()
                        .is_some_and(|current| current.tag == local && current.depth == 0);
                    if done {
                        if let Some(finished) = capture.take() {
                            push_captured(&mut out, &finished);
                        }
                    } else if let Some(current) = capture.as_mut() {
                        if current.tag == local && current.depth > 0 {
                            current.depth -= 1;
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
        if out.len() >= MAX_CONTENT_BYTES {
            out.truncate(MAX_CONTENT_BYTES);
            break;
        }
    }
    out.trim().to_string()
}

fn plain_nfo(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        push_line(&mut out, raw.trim());
    }
    out.trim().to_string()
}

struct Capture {
    tag: String,
    depth: i32,
    buf: String,
}

impl Capture {
    fn new(tag: String) -> Self {
        Self {
            tag,
            depth: 0,
            buf: String::new(),
        }
    }
}

fn is_text_tag(local: &str) -> bool {
    matches!(
        local,
        "title"
            | "originaltitle"
            | "showtitle"
            | "sorttitle"
            | "plot"
            | "outline"
            | "tagline"
            | "name"
            | "role"
            | "genre"
            | "director"
            | "credits"
            | "studio"
            | "album"
            | "artist"
            | "review"
            | "biography"
            | "style"
            | "mood"
            | "country"
            | "network"
            | "status"
            | "mpaa"
            | "year"
    )
}

fn push_captured(out: &mut String, capture: &Capture) {
    let raw = capture.buf.trim();
    if raw.is_empty() {
        return;
    }
    let rich = matches!(
        capture.tag.as_str(),
        "plot" | "outline" | "review" | "biography" | "tagline"
    );
    if rich && raw.contains('<') {
        push_line(out, &html_text(&raw.replace('&', "&amp;")));
    } else {
        push_line(out, raw);
    }
}

fn decode_ref(name: &str) -> char {
    if let Some(num) = name.strip_prefix('#') {
        let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            num.parse().ok()
        };
        if let Some(ch) = code.and_then(char::from_u32) {
            return ch;
        }
    }
    match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => ' ',
    }
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn push_line(out: &mut String, value: &str) {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() || out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    out.push_str(&value.chars().take(room).collect::<String>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kodi_keeps_plot_and_people_and_skips_artwork() {
        let text = nfo_text(
            r#"<movie>
              <title>Night Drive</title>
              <year>1999</year>
              <plot>A &amp; B race. <b>Visible</b><script>SECRET</script></plot>
              <actor><name>Ada</name><role>Lead</role></actor>
              <thumb>https://example.com/poster.jpg</thumb>
            </movie>"#,
        );
        assert!(text.contains("Night Drive"), "{text}");
        assert!(text.contains("1999"), "{text}");
        assert!(text.contains("A & B race"), "{text}");
        assert!(text.contains("Visible"), "{text}");
        assert!(text.contains("Ada"), "{text}");
        assert!(text.contains("Lead"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("poster.jpg"), "{text}");
    }

    #[test]
    fn plain_scene_nfo_is_indexed_as_text() {
        let text = nfo_text("Night.Drive.1999.1080p\nAda Lovelace\n");
        assert!(text.contains("Night.Drive.1999.1080p"), "{text}");
        assert!(text.contains("Ada Lovelace"), "{text}");
    }
}
