//! LaTeX source extractor.
//!
//! Comments (`%…`) are dropped so notes do not drown the document. A `%`
//! escaped with a backslash is kept. `verbatim`, `lstlisting`, and `minted`
//! blocks are kept whole, including percent signs.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from LaTeX sources.
#[derive(Debug, Default, Clone, Copy)]
pub struct LatexExtractor;

#[async_trait]
impl ContentExtractor for LatexExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("text/x-tex")
                || base.eq_ignore_ascii_case("application/x-tex")
                || base.eq_ignore_ascii_case("application/x-latex")
        }) || extension
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "tex" | "ltx" | "latex"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(latex_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn latex_text(input: &str) -> String {
    let mut out = String::new();
    let mut verbatim: Option<String> = None;
    for raw in input.lines() {
        if let Some(env) = verbatim.as_deref() {
            push_line(&mut out, raw.trim_end());
            if ends_env(raw, env) {
                verbatim = None;
            }
            continue;
        }
        if let Some(env) = begins_verbatim(raw) {
            verbatim = Some(env);
            push_line(&mut out, strip_comment(raw));
            continue;
        }
        push_line(&mut out, strip_comment(raw));
    }
    out.trim().to_string()
}

fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut slashes = 0usize;
    for (i, byte) in bytes.iter().copied().enumerate() {
        if byte == b'\\' {
            slashes += 1;
            continue;
        }
        if byte == b'%' && slashes.is_multiple_of(2) {
            return line[..i].trim_end();
        }
        slashes = 0;
    }
    line.trim_end()
}

fn begins_verbatim(line: &str) -> Option<String> {
    for env in ["verbatim", "lstlisting", "minted"] {
        if contains_env(line, "begin", env) {
            return Some(env.to_string());
        }
    }
    None
}

fn ends_env(line: &str, env: &str) -> bool {
    contains_env(line, "end", env)
}

fn contains_env(line: &str, kind: &str, env: &str) -> bool {
    let needle = format!("\\{kind}{{{env}}}");
    line.contains(&needle)
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
    fn drops_comments_and_keeps_escaped_percent_and_verbatim() {
        let text = latex_text(
            "\\section{Hello World}\n\
             % SECRET comment\n\
             100\\% done\n\
             \\\\ % HIDDEN after a linebreak\n\
             \\begin{verbatim}\n\
             code % KEEP\n\
             \\end{verbatim}\n",
        );
        assert!(text.contains("Hello World"), "{text}");
        assert!(text.contains("100\\% done"), "{text}");
        assert!(text.contains("code % KEEP"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("HIDDEN"), "{text}");
    }
}
