//! systemd unit extractor.
//!
//! Descriptions, documentation, and start commands are indexed.
//! Environment variables and credentials are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from systemd unit files.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnitExtractor;

#[async_trait]
impl ContentExtractor for UnitExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(is_unit_ext)
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(unit_text(&decode_best_effort(&raw)))
    }
}

/// systemd unit suffixes such as `service`, `socket`, and `mount`.
pub(crate) fn is_unit_ext(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "service"
            | "socket"
            | "device"
            | "mount"
            | "automount"
            | "swap"
            | "target"
            | "path"
            | "timer"
            | "slice"
            | "scope"
    )
}

pub(crate) fn unit_text(input: &str) -> String {
    let mut out = String::new();
    let mut carry: Option<Carry> = None;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(Carry::Keep(text)) = carry.take() {
                push_line(&mut out, unquote(text.trim()));
            }
            continue;
        }
        if let Some(pending) = carry.take() {
            match pending {
                Carry::Skip => {
                    if continues(line) {
                        carry = Some(Carry::Skip);
                    }
                }
                Carry::Keep(mut text) => {
                    text.push(' ');
                    text.push_str(without_cont(line));
                    if continues(line) {
                        carry = Some(Carry::Keep(text));
                    } else {
                        push_line(&mut out, unquote(text.trim()));
                    }
                }
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !is_text_key(key) {
            if continues(line) {
                carry = Some(Carry::Skip);
            }
            continue;
        }
        let value = without_cont(value.trim());
        if continues(line) {
            carry = Some(Carry::Keep(value.to_string()));
        } else {
            push_line(&mut out, unquote(value));
        }
    }
    if let Some(Carry::Keep(text)) = carry {
        push_line(&mut out, unquote(text.trim()));
    }
    out.trim().to_string()
}

enum Carry {
    Keep(String),
    Skip,
}

fn is_text_key(key: &str) -> bool {
    matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "description"
            | "documentation"
            | "execstart"
            | "execstop"
            | "execreload"
            | "execstartpre"
            | "execstartpost"
            | "execstoppost"
            | "execcondition"
            | "wantedby"
            | "requiredby"
            | "alias"
            | "also"
            | "partof"
    )
}

fn continues(line: &str) -> bool {
    let slashes = line
        .trim_end()
        .chars()
        .rev()
        .take_while(|c| *c == '\\')
        .count();
    slashes % 2 == 1
}

fn without_cont(line: &str) -> &str {
    let line = line.trim();
    if continues(line) {
        line[..line.len() - 1].trim_end()
    } else {
        line
    }
}

fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if value.len() >= 2
        && ((bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\''))
    {
        return &value[1..value.len() - 1];
    }
    value
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
    fn indexes_descriptions_and_skips_environment() {
        let text = unit_text(
            "# SECRET\n\
             [Unit]\n\
             Description=\"Orchid indexer\"\n\
             Documentation=man:orchid(1)\n\
             [Service]\n\
             ExecStart=/usr/bin/orchid --index\n\
             Environment=API_TOKEN=SECRET2\n\
             Environment=FOO=\\\n\
               SECRET3\n\
             [Install]\n\
             WantedBy=multi-user.target\n\
             Description=Line one \\\n\
               line two\n",
        );
        assert!(text.contains("Orchid indexer"), "{text}");
        assert!(text.contains("man:orchid(1)"), "{text}");
        assert!(text.contains("/usr/bin/orchid"), "{text}");
        assert!(text.contains("multi-user.target"), "{text}");
        assert!(text.contains("Line one line two"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("API_TOKEN"), "{text}");
        assert!(is_unit_ext("service"));
        assert!(!is_unit_ext("conf"));
    }
}
