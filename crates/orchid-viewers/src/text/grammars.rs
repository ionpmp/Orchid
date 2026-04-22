//! Language detection for text viewer.
//!
//! The MVP ships with **no** tree-sitter grammar bundled — adding them
//! is a follow-up task because every grammar is a C build that requires
//! the MSVC / clang toolchain. The detection here still produces the
//! canonical language tag (`"rust"`, `"python"`, …) so the future
//! grammar pack wires in without callers changing.

/// Stable language tag for `plaintext` (no highlighting).
pub const PLAINTEXT: &str = "plaintext";

/// Detect a language from the path + a small byte sample.
///
/// Strategy:
/// 1. Extension lookup table.
/// 2. Shebang on the first line (for extension-less scripts).
/// 3. Fallback to [`PLAINTEXT`].
#[must_use]
pub fn detect_language(path: &orchid_fs::FsPath, first_bytes: &[u8]) -> &'static str {
    if let Some(ext) = extension_of(path) {
        if let Some(lang) = by_extension(&ext) {
            return lang;
        }
    }
    if let Some(lang) = by_shebang(first_bytes) {
        return lang;
    }
    PLAINTEXT
}

fn extension_of(path: &orchid_fs::FsPath) -> Option<String> {
    let name = path.file_name()?;
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

fn by_extension(ext: &str) -> Option<&'static str> {
    // Keep the table in alphabetical order for easier review.
    Some(match ext {
        "bash" | "sh" | "zsh" => "bash",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => "cpp",
        "cs" => "csharp",
        "css" | "scss" | "sass" => "css",
        "go" => "go",
        "html" | "htm" => "html",
        "java" => "java",
        "js" | "mjs" | "cjs" => "javascript",
        "json" | "json5" | "jsonc" => "json",
        "kt" | "kts" => "kotlin",
        "lua" => "lua",
        "md" | "markdown" => "markdown",
        "php" => "php",
        "py" | "pyi" | "pyw" => "python",
        "rb" | "rake" => "ruby",
        "rs" => "rust",
        "sql" => "sql",
        "swift" => "swift",
        "toml" => "toml",
        "ts" | "tsx" => "typescript",
        "xml" | "xhtml" | "plist" => "xml",
        "yaml" | "yml" => "yaml",
        "ini" | "cfg" | "conf" => "ini",
        "dockerfile" => "dockerfile",
        "log" | "txt" | "text" => "plaintext",
        _ => return None,
    })
}

fn by_shebang(bytes: &[u8]) -> Option<&'static str> {
    if !bytes.starts_with(b"#!") {
        return None;
    }
    let end = bytes.iter().take(256).position(|b| *b == b'\n').unwrap_or(bytes.len().min(256));
    let line = std::str::from_utf8(&bytes[..end]).ok()?.to_ascii_lowercase();
    for (needle, lang) in [
        ("python", "python"),
        ("node", "javascript"),
        ("bash", "bash"),
        ("/sh", "bash"),
        ("zsh", "bash"),
        ("ruby", "ruby"),
        ("perl", "perl"),
        ("php", "php"),
    ] {
        if line.contains(needle) {
            return Some(lang);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(s: &str) -> orchid_fs::FsPath {
        orchid_fs::FsPath::new(s).unwrap()
    }

    #[test]
    fn detects_by_extension() {
        assert_eq!(detect_language(&path("local:/a/b.rs"), b""), "rust");
        assert_eq!(detect_language(&path("local:/a/b.py"), b""), "python");
        assert_eq!(detect_language(&path("local:/a/b.json"), b""), "json");
        assert_eq!(detect_language(&path("local:/a/unknown.xyz"), b""), PLAINTEXT);
    }

    #[test]
    fn detects_by_shebang_when_no_extension() {
        assert_eq!(
            detect_language(&path("local:/a/script"), b"#!/usr/bin/env python3\n"),
            "python"
        );
        assert_eq!(
            detect_language(&path("local:/a/script"), b"#!/bin/bash -e\n"),
            "bash"
        );
    }

    #[test]
    fn falls_back_to_plaintext() {
        assert_eq!(detect_language(&path("local:/a/b"), b"hello"), PLAINTEXT);
    }
}
