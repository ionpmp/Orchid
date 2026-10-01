//! Maven POM extractor.
//!
//! Group ids, artifact ids, names, and descriptions are indexed. Versions
//! and `properties` blocks are not. `.xml` dispatch lives in
//! [`super::Extractor::extract`]: only documents that look like a POM are
//! handled here.

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn looks_like_pom(xml: &str) -> bool {
    let probe: String = xml.chars().take(1500).collect();
    let probe = probe.to_ascii_lowercase();
    has_open_tag(&probe, "<project")
        && (has_open_tag(&probe, "<artifactid") || has_open_tag(&probe, "<modelversion"))
}

pub(crate) fn pom_text(xml: &str) -> String {
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
                if skip > 0 || is_hidden(&local) {
                    skip += 1;
                } else if let Some(current) = capture.as_mut() {
                    current.depth += 1;
                } else if is_label(&local) {
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

fn is_label(local: &str) -> bool {
    matches!(
        local,
        "groupid"
            | "artifactid"
            | "name"
            | "description"
            | "url"
            | "packaging"
            | "module"
            | "email"
            | "organization"
            | "connection"
            | "developerconnection"
    )
}

fn is_hidden(local: &str) -> bool {
    matches!(local, "properties" | "configuration")
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
    fn indexes_coordinates_and_skips_versions() {
        let xml = r#"<project>
              <modelVersion>4.0.0</modelVersion>
              <groupId>com.example</groupId>
              <artifactId>orchid</artifactId>
              <version>1.2.3</version>
              <name>Orchid</name>
              <description>File manager</description>
              <properties>
                <secret>SECRET</secret>
              </properties>
              <dependencies>
                <dependency>
                  <groupId>org.apache</groupId>
                  <artifactId>commons-lang</artifactId>
                  <version>3.12.0</version>
                </dependency>
              </dependencies>
            </project>"#;
        assert!(looks_like_pom(xml));
        assert!(!looks_like_pom("<note>hello</note>"));
        let text = pom_text(xml);
        assert!(text.contains("com.example"), "{text}");
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("Orchid"), "{text}");
        assert!(text.contains("File manager"), "{text}");
        assert!(text.contains("commons-lang"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("3.12.0"), "{text}");
        assert!(!text.contains("4.0.0"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
    }
}
