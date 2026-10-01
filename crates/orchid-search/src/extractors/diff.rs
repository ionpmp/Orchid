//! Unified diff extractor.
//!
//! Changed paths and line text are indexed. Git blob hashes, hunk line
//! numbers, and binary patch bodies are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from `.diff` and `.patch` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiffExtractor;

#[async_trait]
impl ContentExtractor for DiffExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            matches!(
                base.to_ascii_lowercase().as_str(),
                "text/x-diff" | "text/x-patch" | "application/x-patch"
            )
        }) || extension
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "diff" | "patch"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(diff_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn diff_text(input: &str) -> String {
    let mut out = String::new();
    let mut binary = false;
    for raw in input.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        if binary {
            if line.starts_with("diff ") {
                binary = false;
            } else {
                continue;
            }
        }
        if line.starts_with("GIT binary patch") {
            binary = true;
            continue;
        }
        if is_meta(line) {
            continue;
        }
        if let Some(path) = line
            .strip_prefix("rename from ")
            .or_else(|| line.strip_prefix("rename to "))
        {
            push_line(&mut out, path.trim());
            continue;
        }
        if let Some(path) = line
            .strip_prefix("--- ")
            .or_else(|| line.strip_prefix("+++ "))
        {
            let path = clean_path(path);
            if path != "/dev/null" {
                push_line(&mut out, path);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@") {
            if let Some((_, context)) = rest.split_once("@@") {
                push_line(&mut out, context.trim());
            }
            continue;
        }
        if let Some(rest) = line
            .strip_prefix('+')
            .or_else(|| line.strip_prefix('-'))
            .or_else(|| line.strip_prefix(' '))
        {
            push_line(&mut out, rest);
        }
    }
    out.trim().to_string()
}

fn is_meta(line: &str) -> bool {
    line.starts_with("diff ")
        || line.starts_with("index ")
        || line.starts_with("new file ")
        || line.starts_with("deleted file ")
        || line.starts_with("old mode ")
        || line.starts_with("new mode ")
        || line.starts_with("similarity ")
        || line.starts_with("\\ No newline")
}

fn clean_path(header: &str) -> &str {
    let path = header.split_whitespace().next().unwrap_or(header);
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
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
    fn indexes_paths_and_lines_and_skips_hashes() {
        let text = diff_text(
            "diff --git a/src/main.rs b/src/main.rs\n\
             index deadbeef..cafebabe 100644\n\
             --- a/src/main.rs\n\
             +++ b/src/main.rs\n\
             @@ -10,6 +10,7 @@ fn open_file() {\n\
             \u{20}context\n\
             -old line\n\
             +new orchid line\n\
             GIT binary patch\n\
             SECRETBIN\n\
             diff --git a/other.txt b/other.txt\n\
             --- a/other.txt\n\
             +++ b/other.txt\n\
             +visible\n",
        );
        assert!(text.contains("src/main.rs"), "{text}");
        assert!(text.contains("fn open_file()"), "{text}");
        assert!(text.contains("context"), "{text}");
        assert!(text.contains("old line"), "{text}");
        assert!(text.contains("new orchid line"), "{text}");
        assert!(text.contains("other.txt"), "{text}");
        assert!(text.contains("visible"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("cafebabe"), "{text}");
        assert!(!text.contains("-10"), "{text}");
        assert!(!text.contains("SECRETBIN"), "{text}");
    }
}
