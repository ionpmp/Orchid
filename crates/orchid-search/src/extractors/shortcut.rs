//! Shortcut extractor for `.url`, `.desktop`, and `.webloc`.
//!
//! Names, comments, and URLs are indexed. Launch commands (`Exec`) and icon
//! paths are not.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

const DESKTOP_KEYS: &[&str] = &[
    "Name",
    "GenericName",
    "Comment",
    "Keywords",
    "Categories",
    "URL",
];
const URL_KEYS: &[&str] = &["URL", "BaseURL"];

/// Extract searchable titles and URLs from shortcut files.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShortcutExtractor;

#[async_trait]
impl ContentExtractor for ShortcutExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/x-desktop")
        }) || extension.is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "url" | "desktop" | "webloc"
            )
        })
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        let text = decode_best_effort(&raw);
        let ext = path.extension().unwrap_or("");
        Ok(shortcut_text(&text, ext))
    }
}

pub(crate) fn shortcut_text(input: &str, extension: &str) -> String {
    match extension.to_ascii_lowercase().as_str() {
        "webloc" => webloc_text(input),
        "desktop" => ini_values(input, DESKTOP_KEYS),
        _ => ini_values(input, URL_KEYS),
    }
}

fn ini_values(input: &str, keys: &[&str]) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with(';')
            || line.starts_with('[')
        {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let base = key.split('[').next().unwrap_or(key).trim();
        if keys.iter().any(|k| base.eq_ignore_ascii_case(k)) {
            push_line(&mut out, &unescape_value(value.trim()));
        }
    }
    out.trim().to_string()
}

fn unescape_value(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn webloc_text(xml: &str) -> String {
    if xml.starts_with("bplist") {
        return String::new();
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_key = false;
    let mut want_string = false;
    let mut in_string = false;
    let mut current = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "key" {
                    in_key = true;
                    current.clear();
                } else if local == "string" && want_string {
                    in_string = true;
                    current.clear();
                }
            }
            Ok(Event::Text(t)) => {
                if in_key || in_string {
                    current.push_str(t.as_ref());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if in_key || in_string {
                    current.push(decode_ref(r.as_ref()));
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "key" && in_key {
                    in_key = false;
                    want_string = current.trim() == "URL";
                } else if local == "string" && in_string {
                    in_string = false;
                    want_string = false;
                    push_line(&mut out, current.trim());
                    current.clear();
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
    let value = value.trim();
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
    fn url_and_desktop_keep_names_and_skip_launch_lines() {
        let url = shortcut_text(
            "[InternetShortcut]\n\
             URL=https://example.com/page\n\
             IconFile=C:/icon.ico\n",
            "url",
        );
        assert!(url.contains("https://example.com/page"), "{url}");
        assert!(!url.contains("icon.ico"), "{url}");

        let desktop = shortcut_text(
            "[Desktop Entry]\n\
             Name=Nightly\n\
             Name[ru]=Ночной\n\
             Comment=Builds\\sthe tree\n\
             Exec=app --flag\n\
             Categories=Development;\n",
            "desktop",
        );
        assert!(desktop.contains("Nightly"), "{desktop}");
        assert!(desktop.contains("Ночной"), "{desktop}");
        assert!(desktop.contains("Builds the tree"), "{desktop}");
        assert!(desktop.contains("Development"), "{desktop}");
        assert!(!desktop.contains("--flag"), "{desktop}");
        assert!(!desktop.contains("Exec"), "{desktop}");
    }

    #[test]
    fn webloc_reads_the_url_string() {
        let text = shortcut_text(
            r#"<?xml version="1.0"?>
            <plist version="1.0"><dict>
              <key>URL</key>
              <string>https://example.com/a&amp;b</string>
            </dict></plist>"#,
            "webloc",
        );
        assert!(text.contains("https://example.com/a&b"), "{text}");
        assert!(!text.contains("plist"), "{text}");
    }
}
