//! RSS and Atom feed extractor.
//!
//! Channel and entry titles, authors, links, and article text are indexed.
//! Markup inside descriptions is stripped. Dates and ids are left out.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::epub_odf::html_text;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from `.rss` and `.atom` feeds.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeedExtractor;

#[async_trait]
impl ContentExtractor for FeedExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/rss+xml")
                || base.eq_ignore_ascii_case("application/atom+xml")
                || base.eq_ignore_ascii_case("application/rdf+xml")
        }) || extension
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "rss" | "atom" | "rdf"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(feed_text(&decode_best_effort(&raw)))
    }
}

/// True when an `.xml` file opens as an RSS, Atom, or RSS 1.0 document.
pub(crate) fn looks_like_feed(xml: &str) -> bool {
    let probe: String = xml.chars().take(800).collect();
    let probe = probe.trim_start().to_ascii_lowercase();
    has_open_tag(&probe, "<rss")
        || has_open_tag(&probe, "<feed")
        || has_open_tag(&probe, "<rdf:rdf")
}

pub(crate) fn feed_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut capture: Option<Capture> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if let Some(current) = capture.as_mut() {
                    if local == current.tag {
                        current.depth += 1;
                    }
                } else if local == "link" {
                    if let Some(href) = attr(&e, "href") {
                        push_line(&mut out, &href);
                    } else {
                        capture = Some(Capture::new(local));
                    }
                } else if is_text_tag(&local) {
                    capture = Some(Capture::new(local));
                }
            }
            Ok(Event::Empty(e)) => {
                if capture.is_none() && local_name(e.name().as_ref()) == "link" {
                    if let Some(href) = attr(&e, "href") {
                        push_line(&mut out, &href);
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(current) = capture.as_mut() {
                    current.buf.push_str(t.as_ref());
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(current) = capture.as_mut() {
                    current.buf.push_str(t.as_ref());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some(current) = capture.as_mut() {
                    current.buf.push(decode_ref(r.as_ref()));
                }
            }
            Ok(Event::End(e)) => {
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

fn is_text_tag(local: &str) -> bool {
    matches!(
        local,
        "title"
            | "description"
            | "subtitle"
            | "summary"
            | "content"
            | "encoded"
            | "creator"
            | "name"
            | "author"
            | "category"
            | "subject"
    )
}

fn push_captured(out: &mut String, capture: &Capture) {
    let raw = capture.buf.trim();
    if raw.is_empty() {
        return;
    }
    let rich = matches!(
        capture.tag.as_str(),
        "description" | "subtitle" | "summary" | "content" | "encoded"
    );
    if rich && raw.contains('<') {
        push_line(out, &html_text(raw));
    } else {
        push_line(out, raw);
    }
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
    fn rss_keeps_titles_and_strips_html() {
        let text = feed_text(
            r#"<?xml version="1.0"?>
            <rss version="2.0"><channel>
              <title>Feed Title</title>
              <link>https://example.com</link>
              <description>Channel &amp; about</description>
              <pubDate>Mon, 01 Jan 2020 00:00:00 GMT</pubDate>
              <item>
                <title>Post One</title>
                <author>Ada</author>
                <link>https://example.com/one</link>
                <description><![CDATA[<p>Visible body</p><script>SECRET</script>]]></description>
              </item>
            </channel></rss>"#,
        );
        assert!(text.contains("Feed Title"), "{text}");
        assert!(text.contains("Channel & about"), "{text}");
        assert!(text.contains("Post One"), "{text}");
        assert!(text.contains("Ada"), "{text}");
        assert!(text.contains("https://example.com/one"), "{text}");
        assert!(text.contains("Visible body"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("Jan 2020"), "{text}");
        assert!(!text.contains("version"), "{text}");
    }

    #[test]
    fn atom_reads_entry_links_and_html_content() {
        let text = feed_text(
            r#"<?xml version="1.0"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
              <title>Atom Feed</title>
              <link href="https://example.com/atom"/>
              <entry>
                <title>Entry</title>
                <author><name>Grace</name></author>
                <link href="https://example.com/entry"/>
                <content type="html">&lt;p&gt;Entry body&lt;/p&gt;</content>
              </entry>
            </feed>"#,
        );
        assert!(text.contains("Atom Feed"), "{text}");
        assert!(text.contains("https://example.com/atom"), "{text}");
        assert!(text.contains("Entry"), "{text}");
        assert!(text.contains("Grace"), "{text}");
        assert!(text.contains("https://example.com/entry"), "{text}");
        assert!(text.contains("Entry body"), "{text}");
        assert!(!text.contains("<p>"), "{text}");
    }

    #[test]
    fn xml_sniff_rejects_lookalikes() {
        assert!(looks_like_feed(
            r#"<?xml version="1.0"?><rss version="2.0">"#
        ));
        assert!(looks_like_feed(
            "<feed xmlns=\"http://www.w3.org/2005/Atom\">"
        ));
        assert!(!looks_like_feed("<feedback>not a feed</feedback>"));
        assert!(!looks_like_feed("<root><item>nope</item></root>"));
    }
}
