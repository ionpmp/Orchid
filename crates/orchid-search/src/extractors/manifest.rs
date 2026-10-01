//! Android manifest extractor.
//!
//! Package names, labels, component names, and permissions are indexed.
//! Versions and resource references are not. `.xml` dispatch lives in
//! [`super::Extractor::extract`]: only documents that look like a manifest
//! are handled here.

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn looks_like_manifest(xml: &str) -> bool {
    let probe: String = xml.chars().take(1500).collect();
    let probe = probe.to_ascii_lowercase();
    has_open_tag(&probe, "<manifest")
        && (has_open_tag(&probe, "<application")
            || has_open_tag(&probe, "<uses-permission")
            || probe.contains("android:"))
}

pub(crate) fn manifest_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => take_attrs(&e, &mut out),
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

fn take_attrs(e: &quick_xml::events::BytesStart<'_>, out: &mut String) {
    for attr in e.attributes().flatten() {
        let key = local_name(attr.key.as_ref());
        if !is_label(&key) {
            continue;
        }
        let Ok(value) = attr.normalized_value(XmlVersion::Implicit1_0) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() || value.starts_with('@') {
            continue;
        }
        push_line(out, value);
    }
}

fn is_label(key: &str) -> bool {
    matches!(
        key,
        "package"
            | "label"
            | "name"
            | "authorities"
            | "permission"
            | "host"
            | "scheme"
            | "path"
            | "pathprefix"
            | "pathpattern"
    )
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
    fn indexes_labels_and_skips_versions() {
        let xml = r#"<manifest package="com.example.orchid" android:versionName="1.2.3">
              <uses-permission android:name="android.permission.INTERNET" />
              <application android:label="Orchid" android:icon="@drawable/icon">
                <activity android:name=".MainActivity" android:label="Home" />
              </application>
            </manifest>"#;
        assert!(looks_like_manifest(xml));
        assert!(!looks_like_manifest(
            "<project><artifactId>orchid</artifactId></project>"
        ));
        let text = manifest_text(xml);
        assert!(text.contains("com.example.orchid"), "{text}");
        assert!(text.contains("android.permission.INTERNET"), "{text}");
        assert!(text.contains("Orchid"), "{text}");
        assert!(text.contains(".MainActivity"), "{text}");
        assert!(text.contains("Home"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("@drawable"), "{text}");
        assert!(!text.contains("icon"), "{text}");
    }
}
