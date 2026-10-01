//! Bundler lockfile extractor.
//!
//! Gem names are indexed. Versions, revisions, and checksums are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_gemfile_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("Gemfile.lock")
}

pub(crate) fn gemfile_lock_text(input: &str) -> String {
    let mut out = String::new();
    let mut mode = Mode::Skip;
    for raw in input.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.chars().take_while(|ch| ch.is_whitespace()).count();
        let line = raw.trim();
        if indent == 0 {
            mode = match line {
                "GEM" | "PATH" | "GIT" => Mode::WaitSpecs,
                "DEPENDENCIES" => Mode::Deps,
                _ => Mode::Skip,
            };
            continue;
        }
        match mode {
            Mode::WaitSpecs if line == "specs:" => mode = Mode::Specs,
            Mode::Specs | Mode::Deps => {
                if let Some(name) = gem_name(line) {
                    push_line(&mut out, name);
                }
            }
            Mode::WaitSpecs | Mode::Skip => {}
        }
    }
    out.trim().to_string()
}

enum Mode {
    Skip,
    WaitSpecs,
    Specs,
    Deps,
}

fn gem_name(line: &str) -> Option<&str> {
    let name = line.split_whitespace().next()?;
    if name.ends_with(':') {
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
    fn indexes_gem_names_and_skips_checksums() {
        let text = gemfile_lock_text(
            "GEM\n\
             \u{20}\u{20}remote: https://rubygems.org/\n\
             \u{20}\u{20}specs:\n\
             \u{20}\u{20}\u{20}\u{20}orchid (1.2.3)\n\
             \u{20}\u{20}\u{20}\u{20}\u{20}\u{20}rack (~> 2.0)\n\
             \n\
             GIT\n\
             \u{20}\u{20}revision: deadbeefcafebabe\n\
             \u{20}\u{20}specs:\n\
             \u{20}\u{20}\u{20}\u{20}foo (1.0.0)\n\
             \n\
             CHECKSUMS\n\
             \u{20}\u{20}orchid (1.2.3) sha256=SECRET\n\
             \n\
             DEPENDENCIES\n\
             \u{20}\u{20}rails (~> 7.0)\n\
             \n\
             BUNDLED WITH\n\
             \u{20}\u{20}2.4.10\n",
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("rack"), "{text}");
        assert!(text.contains("foo"), "{text}");
        assert!(text.contains("rails"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("2.4.10"), "{text}");
        assert!(is_gemfile_lock_name("Gemfile.lock"));
        assert!(!is_gemfile_lock_name("Gemfile"));
    }
}
