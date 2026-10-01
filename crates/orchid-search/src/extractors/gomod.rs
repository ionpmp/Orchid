//! Go module file extractor.
//!
//! The module path and required module paths are indexed. Toolchain and
//! dependency versions are not. Dispatch lives in
//! [`super::Extractor::extract`] because `text/plain` would keep the versions.

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_gomod_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("go.mod")
}

pub(crate) fn gomod_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() || line == "(" || line == ")" {
            continue;
        }
        if line.starts_with("go ") || line.starts_with("toolchain ") || line.starts_with("retract")
        {
            continue;
        }
        let spec = line
            .strip_prefix("module ")
            .or_else(|| line.strip_prefix("require "))
            .or_else(|| line.strip_prefix("exclude "))
            .or_else(|| line.strip_prefix("replace "))
            .unwrap_or(line);
        if let Some((left, right)) = spec.split_once("=>") {
            push_module(&mut out, left);
            push_module(&mut out, right);
        } else {
            push_module(&mut out, spec);
        }
    }
    out.trim().to_string()
}

fn strip_comment(line: &str) -> &str {
    match line.find("//") {
        Some(index) => line[..index].trim_end(),
        None => line,
    }
}

fn push_module(out: &mut String, spec: &str) {
    let Some(token) = spec.split_whitespace().next() else {
        return;
    };
    if token == "(" || token == ")" || is_version(token) {
        return;
    }
    if token.contains('/') || token.contains('.') || token.starts_with('.') {
        push_line(out, token);
    }
}

fn is_version(token: &str) -> bool {
    let rest = token.strip_prefix('v').unwrap_or(token);
    rest.chars().next().is_some_and(|ch| ch.is_ascii_digit())
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
    fn indexes_module_paths_and_skips_versions() {
        let text = gomod_text(
            "module github.com/example/orchid\n\
             \n\
             go 1.22.0\n\
             \n\
             require (\n\
             \tgithub.com/foo/bar v1.2.3 // SECRET\n\
             )\n\
             \n\
             replace github.com/foo/bar => ../local/bar\n",
        );
        assert!(text.contains("github.com/example/orchid"), "{text}");
        assert!(text.contains("github.com/foo/bar"), "{text}");
        assert!(text.contains("../local/bar"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("1.22.0"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(is_gomod_name("go.mod"));
        assert!(!is_gomod_name("go.sum"));
    }
}
