//! XSPF playlist extractor.
//!
//! Titles, creators, albums, annotations, and locations are indexed.
//! Durations and vendor `extension` blocks are not.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable titles and paths from XSPF playlists.
#[derive(Debug, Default, Clone, Copy)]
pub struct XspfExtractor;

#[async_trait]
impl ContentExtractor for XspfExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/xspf+xml")
        }) || extension.is_some_and(|ext| ext.eq_ignore_ascii_case("xspf"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(xspf_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn xspf_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip = 0i32;
    let mut capture: Option<Capture> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 {
                    skip += 1;
                } else if is_hidden(&local) {
                    skip = 1;
                } else if let Some(current) = capture.as_mut() {
                    current.depth += 1;
                } else if is_wanted(&local) {
                    capture = Some(Capture {
                        depth: 1,
                        text: String::new(),
                    });
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        append(&mut current.text, t.as_ref());
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        append(&mut current.text, t.as_ref());
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        current.text.push(decode_ref(r.as_ref()));
                    }
                }
            }
            Ok(Event::End(_)) => {
                if skip > 0 {
                    skip -= 1;
                } else {
                    let flush = capture.as_ref().is_some_and(|current| current.depth == 1);
                    if let Some(current) = capture.as_mut() {
                        current.depth -= 1;
                    }
                    if flush {
                        if let Some(current) = capture.take() {
                            push_line(&mut out, current.text.trim());
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
        if out.len() >= MAX_CONTENT_BYTES {
            break;
        }
    }
    out.trim().to_string()
}

struct Capture {
    depth: i32,
    text: String,
}

fn is_wanted(local: &str) -> bool {
    matches!(
        local,
        "title" | "creator" | "annotation" | "album" | "location" | "info" | "identifier"
    )
}

fn is_hidden(local: &str) -> bool {
    matches!(local, "extension" | "meta")
}

fn local_name(name: &str) -> String {
    let name = name.rsplit('}').next().unwrap_or(name);
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

fn append(buf: &mut String, chunk: &str) {
    if chunk.is_empty() || buf.len() >= MAX_CONTENT_BYTES {
        return;
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(buf.len());
    buf.push_str(&chunk.chars().take(room).collect::<String>());
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
    fn keeps_titles_and_skips_duration_and_extensions() {
        let text = xspf_text(
            r#"<playlist xmlns="http://xspf.org/ns/0/">
              <title>Morning Mix</title>
              <trackList>
                <track>
                  <location>file:///music/song.mp3</location>
                  <title>Song &amp; Dance</title>
                  <creator>Artist</creator>
                  <duration>180000</duration>
                  <extension>SECRET</extension>
                </track>
              </trackList>
            </playlist>"#,
        );
        assert!(text.contains("Morning Mix"), "{text}");
        assert!(text.contains("song.mp3"), "{text}");
        assert!(text.contains("Song & Dance"), "{text}");
        assert!(text.contains("Artist"), "{text}");
        assert!(!text.contains("180000"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
    }
}
