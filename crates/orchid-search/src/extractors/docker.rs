//! Dockerfile extractor.
//!
//! Image names, labels, copy paths, and commands are indexed. `ENV` and
//! `ARG` values are not. Extension-less names such as `Dockerfile` are
//! dispatched from [`super::Extractor::extract`].

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from Dockerfiles.
#[derive(Debug, Default, Clone, Copy)]
pub struct DockerExtractor;

#[async_trait]
impl ContentExtractor for DockerExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "dockerfile" | "containerfile"
            )
        })
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(docker_text(&decode_best_effort(&raw)))
    }
}

/// `Dockerfile`, `Containerfile`, or a suffix such as `Dockerfile.dev`.
///
/// `app.dockerfile` is left to [`DockerExtractor`] via its extension.
pub(crate) fn needs_name_dispatch(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if name.ends_with(".dockerfile") || name.ends_with(".containerfile") {
        return false;
    }
    name == "dockerfile"
        || name == "containerfile"
        || name.starts_with("dockerfile.")
        || name.starts_with("containerfile.")
}

pub(crate) fn docker_text(input: &str) -> String {
    let mut out = String::new();
    let mut carry = Carry::None;
    let mut buf = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        match carry {
            Carry::Skip => {
                if !continues(line) {
                    carry = Carry::None;
                }
                continue;
            }
            Carry::Keep => {
                buf.push(' ');
                buf.push_str(without_cont(line));
                if continues(line) {
                    continue;
                }
                push_line(&mut out, buf.trim());
                buf.clear();
                carry = Carry::None;
                continue;
            }
            Carry::None => {}
        }
        if let Some(comment) = line.strip_prefix('#') {
            push_line(&mut out, comment.trim());
            continue;
        }
        let Some((instr, rest)) = split_instr(line) else {
            continue;
        };
        if is_secret(instr) {
            if continues(line) {
                carry = Carry::Skip;
            }
            continue;
        }
        if !is_kept(instr) {
            continue;
        }
        let rest = without_cont(rest);
        if continues(line) {
            buf = rest.to_string();
            carry = Carry::Keep;
        } else {
            push_line(&mut out, rest);
        }
    }
    if carry == Carry::Keep {
        push_line(&mut out, buf.trim());
    }
    out.trim().to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Carry {
    None,
    Keep,
    Skip,
}

fn split_instr(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let instr = parts.next()?;
    if instr.is_empty() {
        return None;
    }
    Some((instr, parts.next().unwrap_or("").trim()))
}

fn is_kept(instr: &str) -> bool {
    matches!(
        instr.to_ascii_lowercase().as_str(),
        "from"
            | "label"
            | "copy"
            | "add"
            | "cmd"
            | "entrypoint"
            | "user"
            | "workdir"
            | "expose"
            | "volume"
            | "healthcheck"
            | "shell"
            | "stopsignal"
            | "run"
            | "maintainer"
    )
}

fn is_secret(instr: &str) -> bool {
    matches!(instr.to_ascii_lowercase().as_str(), "env" | "arg")
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
    fn indexes_images_and_skips_env() {
        let text = docker_text(
            "# Builds the app\n\
             FROM rust:1.98\n\
             LABEL description=\"File manager\"\n\
             ENV API_TOKEN=SECRET\n\
             ENV OTHER=\\\n\
             \u{20} SECRET2\n\
             COPY src /app\n",
        );
        assert!(text.contains("Builds the app"), "{text}");
        assert!(text.contains("rust:1.98"), "{text}");
        assert!(text.contains("File manager"), "{text}");
        assert!(text.contains("src /app"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("API_TOKEN"), "{text}");
        assert!(needs_name_dispatch("Dockerfile"));
        assert!(needs_name_dispatch("Dockerfile.dev"));
        assert!(!needs_name_dispatch("app.dockerfile"));
    }
}
