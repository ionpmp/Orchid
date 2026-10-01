//! OPML outline extractor.
//!
//! Feed lists and nested outlines contribute their titles and URLs. The
//! XML markup itself is not indexed.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract titles and URLs from `.opml` subscription lists.
#[derive(Debug, Default, Clone, Copy)]
pub struct OpmlExtractor;

#[async_trait]
impl ContentExtractor for OpmlExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("text/x-opml")
                || base.eq_ignore_ascii_case("application/xml+opml")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("opml"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(opml_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn opml_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_title = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "title" {
                    in_title = true;
                } else if local == "outline" {
                    push_outline(&mut out, &e);
                }
            }
            Ok(Event::Empty(e)) => {
                if local_name(e.name().as_ref()) == "outline" {
                    push_outline(&mut out, &e);
                }
            }
            Ok(Event::Text(t)) if in_title => push_line(&mut out, t.as_ref()),
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == "title" {
                    in_title = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
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

fn push_outline(out: &mut String, e: &quick_xml::events::BytesStart<'_>) {
    for key in ["text", "title", "description", "xmlUrl", "htmlUrl"] {
        if let Some(value) = attr(e, key) {
            push_line(out, &value);
        }
    }
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    let value = e
        .try_get_attribute(key)
        .ok()
        .flatten()
        .map(|a| a.value.into_owned())?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
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
    fn reads_head_title_and_outline_urls() {
        let text = opml_text(
            r#"<?xml version="1.0"?>
            <opml version="2.0"><head><title>Morning reads</title></head>
            <body>
              <outline text="News">
                <outline text="Rust blog" title="Rust" xmlUrl="https://example.com/rust.xml" htmlUrl="https://example.com/rust"/>
              </outline>
            </body></opml>"#,
        );
        assert!(text.contains("Morning reads"));
        assert!(text.contains("News"));
        assert!(text.contains("Rust blog"));
        assert!(text.contains("https://example.com/rust.xml"));
        assert!(!text.contains("version"));
    }
}
