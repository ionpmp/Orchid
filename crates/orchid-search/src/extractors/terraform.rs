//! Terraform lockfile extractor.
//!
//! Provider addresses are indexed. Versions and hashes are not. Dispatch
//! lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_terraform_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(".terraform.lock.hcl")
}

pub(crate) fn terraform_lock_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("provider ") else {
            continue;
        };
        if let Some(name) = first_quoted(rest) {
            push_line(&mut out, name);
        }
    }
    out.trim().to_string()
}

fn first_quoted(line: &str) -> Option<&str> {
    let rest = line.split_once('"')?.1;
    let name = rest.split_once('"')?.0;
    if name.is_empty() {
        None
    } else {
        Some(name)
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
    fn indexes_providers_and_skips_hashes() {
        let text = terraform_lock_text(
            "provider \"registry.terraform.io/hashicorp/aws\" {\n\
             \u{20}\u{20}version = \"5.31.0\"\n\
             \u{20}\u{20}hashes = [\n\
             \u{20}\u{20}\u{20}\u{20}\"zh:SECRET\",\n\
             \u{20}\u{20}\u{20}\u{20}\"h1:OTHER\",\n\
             \u{20}\u{20}]\n\
             }\n",
        );
        assert!(
            text.contains("registry.terraform.io/hashicorp/aws"),
            "{text}"
        );
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("5.31.0"), "{text}");
        assert!(is_terraform_lock_name(".terraform.lock.hcl"));
        assert!(!is_terraform_lock_name("main.tf"));
    }
}
