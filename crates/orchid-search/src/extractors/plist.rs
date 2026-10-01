//! Property-list extractor.
//!
//! String values such as display names and usage descriptions are indexed.
//! Version and SDK keys are not. Binary `bplist` files contribute nothing.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable strings from XML property lists.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlistExtractor;

#[async_trait]
impl ContentExtractor for PlistExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| ext.eq_ignore_ascii_case("plist"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        if raw.starts_with(b"bplist") {
            return Ok(String::new());
        }
        Ok(plist_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn plist_text(input: &str) -> String {
    if input.starts_with("bplist") {
        return String::new();
    }
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut mode = Mode::Idle;
    let mut chunk = String::new();
    let mut skip_string = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "key" {
                    mode = Mode::Key;
                    chunk.clear();
                } else if local == "string" {
                    mode = if skip_string { Mode::Idle } else { Mode::Value };
                    chunk.clear();
                }
            }
            Ok(Event::Text(t)) => append_mode(&mut chunk, &mode, t.as_ref()),
            Ok(Event::CData(t)) => append_mode(&mut chunk, &mode, t.as_ref()),
            Ok(Event::GeneralRef(r)) => {
                if mode != Mode::Idle {
                    chunk.push(decode_ref(r.as_ref()));
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "key" && mode == Mode::Key {
                    skip_string = is_skipped_key(chunk.trim());
                    mode = Mode::Idle;
                    chunk.clear();
                } else if local == "string" && mode == Mode::Value {
                    push_line(&mut out, chunk.trim());
                    mode = Mode::Idle;
                    chunk.clear();
                    skip_string = false;
                } else if local == "string" {
                    mode = Mode::Idle;
                    skip_string = false;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    Key,
    Value,
}

fn is_skipped_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("version")
        || key.contains("build")
        || key.contains("sdk")
        || key.contains("platform")
}

fn append_mode(chunk: &mut String, mode: &Mode, text: &str) {
    if *mode == Mode::Idle || text.is_empty() || chunk.len() >= MAX_CONTENT_BYTES {
        return;
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(chunk.len());
    chunk.push_str(&text.chars().take(room).collect::<String>());
}

fn decode_ref(name: &str) -> char {
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
    let name = name.rsplit('}').next().unwrap_or(name);
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
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
    fn indexes_display_strings_and_skips_versions() {
        let text = plist_text(
            r#"<plist>
              <dict>
                <key>CFBundleDisplayName</key>
                <string>Orchid</string>
                <key>CFBundleIdentifier</key>
                <string>com.example.orchid</string>
                <key>NSCameraUsageDescription</key>
                <string>Needed to scan</string>
                <key>CFBundleShortVersionString</key>
                <string>1.2.3</string>
              </dict>
            </plist>"#,
        );
        assert!(text.contains("Orchid"), "{text}");
        assert!(text.contains("com.example.orchid"), "{text}");
        assert!(text.contains("Needed to scan"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("CFBundleDisplayName"), "{text}");
        assert!(plist_text("bplist00junk").is_empty());
    }
}
