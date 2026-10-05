//! MIME parse and compose helpers.

use mail_builder::MessageBuilder;
use mail_parser::{MessageParser, MimeHeaders};

use crate::account::{AttachmentMeta, ComposeMessage, MessageBody};
use crate::error::{MailError, Result};
use uuid::Uuid;

/// Parse a raw RFC822 buffer into text/html/attachments.
#[must_use]
pub fn parse_rfc822(account_id: Uuid, folder: &str, uid: u32, raw: &[u8]) -> MessageBody {
    let message = MessageParser::default().parse(raw);
    let Some(message) = message else {
        return MessageBody {
            account_id,
            folder: folder.to_string(),
            uid,
            text: String::from_utf8_lossy(raw).into_owned(),
            html: String::new(),
            attachments: Vec::new(),
            parts: Vec::new(),
        };
    };

    let text = message
        .body_text(0)
        .map(|c| c.into_owned())
        .unwrap_or_default();
    let html = message
        .body_html(0)
        .map(|c| c.into_owned())
        .unwrap_or_default();

    let mut attachments = Vec::new();
    let mut parts = Vec::new();
    for (idx, part) in message.attachments().enumerate() {
        let filename = part
            .attachment_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("attachment-{idx}"));
        let content_type = part
            .content_type()
            .map(|ct| {
                let mut s = ct.ctype().to_string();
                if let Some(st) = ct.subtype() {
                    s.push('/');
                    s.push_str(st);
                }
                s
            })
            .unwrap_or_else(|| "application/octet-stream".into());
        let bytes = part.contents().to_vec();
        let size = bytes.len() as u64;
        attachments.push(AttachmentMeta {
            id: idx.to_string(),
            filename,
            content_type,
            size,
        });
        parts.push(bytes);
    }

    MessageBody {
        account_id,
        folder: folder.to_string(),
        uid,
        text,
        html,
        attachments,
        parts,
    }
}

/// Extract list-row fields from raw RFC822 (or ENVELOPE-like headers).
#[must_use]
pub fn header_fields(raw: &[u8]) -> (String, String, String, String, i64, Option<String>, bool) {
    let message = MessageParser::default().parse(raw);
    let Some(message) = message else {
        return (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            0,
            None,
            false,
        );
    };
    let from = message
        .from()
        .and_then(|list| list.first())
        .map(|addr| {
            addr.name()
                .map(|n| n.to_string())
                .or_else(|| addr.address().map(|a| a.to_string()))
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let to = message
        .to()
        .map(|list| {
            list.iter()
                .filter_map(|a| a.address().map(|s| s.to_string()))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let subject = message.subject().unwrap_or("").to_string();
    let message_id = message.message_id().map(|s| s.to_string());
    let (date, date_unix) = message
        .date()
        .map(|d| {
            let s = d.to_rfc3339();
            (s, d.to_timestamp())
        })
        .unwrap_or_else(|| (String::new(), 0));
    let has_attachment = message.attachment_count() > 0;
    (
        from,
        to,
        subject,
        date,
        date_unix,
        message_id,
        has_attachment,
    )
}

/// Build an RFC822 message for SMTP.
pub fn build_rfc822(
    from_name: &str,
    from_email: &str,
    compose: &ComposeMessage,
) -> Result<Vec<u8>> {
    let mut builder = MessageBuilder::new()
        .from((from_name, from_email))
        .subject(compose.subject.as_str())
        .text_body(compose.body.as_str());

    for addr in split_addrs(&compose.to) {
        builder = builder.to(addr);
    }
    for addr in split_addrs(&compose.cc) {
        builder = builder.cc(addr);
    }
    for addr in split_addrs(&compose.bcc) {
        builder = builder.bcc(addr);
    }
    if let Some(irt) = &compose.in_reply_to {
        builder = builder.in_reply_to(irt.as_str());
    }
    if let Some(refs) = &compose.references {
        builder = builder.references(refs.as_str());
    }

    for path in &compose.attachments {
        let data = std::fs::read(path).map_err(|e| MailError::Mime(e.to_string()))?;
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("attachment");
        builder = builder.attachment(content_type_for(name), name, data);
    }

    builder
        .write_to_vec()
        .map_err(|e| MailError::Mime(e.to_string()))
}

/// A few extensions get a specific type. Everything else is sent as bytes.
fn content_type_for(name: &str) -> &'static str {
    match std::path::Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("txt") => "text/plain",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("zip") => "application/zip",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

fn split_addrs(raw: &str) -> Vec<String> {
    raw.split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Strip tags for a plain reading pane when only HTML is present.
#[must_use]
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut last_space = true;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            '&' => {
                // Keep entities as-is for simplicity; UI can show them.
                out.push(ch);
                last_space = false;
            }
            c if c.is_whitespace() => {
                if !last_space {
                    out.push(' ');
                    last_space = true;
                }
            }
            c => {
                out.push(c);
                last_space = false;
            }
        }
    }
    out.trim().to_string()
}

/// Wrap HTML for WebView2 with a CSP that blocks remote images until allowed.
#[must_use]
pub fn sanitize_html_for_webview(html: &str, allow_remote_images: bool) -> String {
    let img_src = if allow_remote_images {
        "img-src https: http: data: cid:;"
    } else {
        "img-src data: cid:;"
    };
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"/>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; {img_src} style-src 'unsafe-inline' data:; font-src data:;"/>
</head><body>{html}</body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_parse_roundtrip() {
        let compose = ComposeMessage {
            to: "a@b.com".into(),
            subject: "Hello".into(),
            body: "Hi there".into(),
            ..Default::default()
        };
        let raw = build_rfc822("Me", "me@example.com", &compose).expect("build");
        let body = parse_rfc822(Uuid::nil(), "INBOX", 1, &raw);
        assert!(body.text.contains("Hi there"));
        assert!(body.attachments.is_empty());
        assert!(body.parts.is_empty());
        let (from, _to, subject, ..) = header_fields(&raw);
        assert!(from.contains("Me") || from.contains("me@"));
        assert_eq!(subject, "Hello");
    }

    #[test]
    fn html_strip() {
        assert_eq!(html_to_text("<b>Hi</b> there"), "Hi there");
    }

    #[test]
    fn parse_keeps_attachment_bytes() {
        let dir = std::env::temp_dir().join(format!("orchid-mail-parse-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.txt");
        std::fs::write(&path, b"note").unwrap();
        let compose = ComposeMessage {
            to: "a@b.com".into(),
            subject: "Files".into(),
            body: "See attached".into(),
            attachments: vec![path.to_string_lossy().into_owned()],
            ..Default::default()
        };
        let raw = build_rfc822("Me", "me@example.com", &compose).expect("build");
        let body = parse_rfc822(Uuid::nil(), "INBOX", 3, &raw);
        assert_eq!(body.attachments.len(), 1);
        assert_eq!(body.attachments[0].filename, "note.txt");
        assert_eq!(body.parts, vec![b"note".to_vec()]);
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(raw_text.contains("text/plain"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_extension_is_octet_stream_and_a_missing_file_fails() {
        let dir = std::env::temp_dir().join(format!("orchid-mail-bin-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blob.bin");
        std::fs::write(&path, b"xyz").unwrap();
        let compose = ComposeMessage {
            to: "a@b.com".into(),
            attachments: vec![path.to_string_lossy().into_owned()],
            ..Default::default()
        };
        let raw = build_rfc822("Me", "me@example.com", &compose).expect("build");
        assert!(String::from_utf8_lossy(&raw).contains("application/octet-stream"));
        let missing = ComposeMessage {
            attachments: vec![dir.join("nope.bin").to_string_lossy().into_owned()],
            ..Default::default()
        };
        assert!(build_rfc822("Me", "me@example.com", &missing).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
