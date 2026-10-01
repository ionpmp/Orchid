//! RPM spec extractor.
//!
//! Package names, summaries, descriptions, changelogs, and file lists are
//! indexed. Build scripts are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from RPM spec files.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpecExtractor;

#[async_trait]
impl ContentExtractor for SpecExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| ext.eq_ignore_ascii_case("spec"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(spec_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn spec_text(input: &str) -> String {
    let mut out = String::new();
    let mut mode = Mode::Preamble;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = directive(line) {
            if is_text_section(name) {
                mode = Mode::Keep;
            } else if is_script_section(name) {
                mode = Mode::Skip;
            } else if name.eq_ignore_ascii_case("package") {
                mode = Mode::Preamble;
                push_package(&mut out, line);
            }
            continue;
        }
        match mode {
            Mode::Preamble => push_header(&mut out, line),
            Mode::Keep => push_line(&mut out, line),
            Mode::Skip => {}
        }
    }
    out.trim().to_string()
}

enum Mode {
    Preamble,
    Keep,
    Skip,
}

fn directive(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('%')?;
    if rest.starts_with('%') {
        return None;
    }
    rest.split_whitespace().next()
}

fn is_text_section(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "description" | "changelog" | "files"
    )
}

fn is_script_section(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "prep"
            | "build"
            | "install"
            | "check"
            | "clean"
            | "pre"
            | "post"
            | "preun"
            | "postun"
            | "pretrans"
            | "posttrans"
            | "generate_buildrequires"
    )
}

fn push_package(out: &mut String, line: &str) {
    let mut parts = line.split_whitespace();
    let _ = parts.next();
    let mut name = None;
    while let Some(part) = parts.next() {
        if part == "-n" {
            name = parts.next();
            break;
        }
        if part.starts_with('-') {
            continue;
        }
        name = Some(part);
        break;
    }
    if let Some(name) = name {
        push_line(out, name);
    }
}

fn push_header(out: &mut String, line: &str) {
    let Some((key, value)) = line.split_once(':') else {
        return;
    };
    let key = key.split('(').next().unwrap_or(key).trim();
    if matches!(
        key.to_ascii_lowercase().as_str(),
        "name" | "summary" | "license" | "url" | "group" | "prefix"
    ) {
        push_line(out, value.trim());
    }
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
    fn indexes_summaries_and_skips_build_scripts() {
        let text = spec_text(
            "Name: orchid\n\
             Version: 1.2.3\n\
             Summary: File manager\n\
             # SECRET\n\
             %description\n\
             Opens local folders.\n\
             %package -n orchid-gui\n\
             Summary: Extra tools\n\
             %prep\n\
             SECRET2\n\
             %changelog\n\
             - Fixed search\n\
             %files\n\
             /usr/bin/orchid\n",
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("File manager"), "{text}");
        assert!(text.contains("Opens local folders"), "{text}");
        assert!(text.contains("orchid-gui"), "{text}");
        assert!(text.contains("Extra tools"), "{text}");
        assert!(text.contains("Fixed search"), "{text}");
        assert!(text.contains("/usr/bin/orchid"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
    }
}
