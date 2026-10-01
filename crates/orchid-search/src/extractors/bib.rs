//! BibTeX (`.bib`) and RIS (`.ris`) bibliography extractors.
//!
//! Titles, authors, abstracts, and related fields are kept. Citation keys
//! and `@string` / `@preamble` blocks are not indexed.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable fields from BibTeX databases.
#[derive(Debug, Default, Clone, Copy)]
pub struct BibExtractor;

/// Extract searchable fields from RIS reference files.
#[derive(Debug, Default, Clone, Copy)]
pub struct RisExtractor;

const BIB_FIELDS: &[&str] = &[
    "title",
    "booktitle",
    "author",
    "editor",
    "abstract",
    "annote",
    "keywords",
    "journal",
    "publisher",
    "note",
    "school",
    "institution",
    "howpublished",
    "series",
    "organization",
];

const RIS_TAGS: &[&str] = &[
    "TI", "T1", "T2", "T3", "AU", "A1", "A2", "A3", "AB", "N2", "KW", "JO", "JF", "JA", "PB", "CY",
    "N1", "DO", "UR",
];

#[async_trait]
impl ContentExtractor for BibExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/x-bibtex")
                || base.eq_ignore_ascii_case("text/x-bibtex")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("bib"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(bib_text(&decode_best_effort(&raw)))
    }
}

#[async_trait]
impl ContentExtractor for RisExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/x-research-info-systems")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("ris"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(ris_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn bib_text(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = String::new();
    while i < chars.len() {
        if chars[i] != '@' {
            i += 1;
            continue;
        }
        i += 1;
        let kind_at = i;
        while i < chars.len() && chars[i].is_ascii_alphabetic() {
            i += 1;
        }
        let kind: String = chars[kind_at..i]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        if kind.is_empty() {
            continue;
        }
        if matches!(kind.as_str(), "comment" | "string" | "preamble") {
            i = skip_group(&chars, i);
            continue;
        }
        while i < chars.len() && chars[i] != '{' && chars[i] != '(' {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let close = if chars[i] == '{' { '}' } else { ')' };
        i += 1;
        while i < chars.len() && chars[i] != ',' && chars[i] != close {
            i += 1;
        }
        if i < chars.len() && chars[i] == ',' {
            i += 1;
        }
        while i < chars.len() && chars[i] != close {
            while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
                i += 1;
            }
            if i >= chars.len() || chars[i] == close {
                break;
            }
            let name_at = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
            {
                i += 1;
            }
            let name: String = chars[name_at..i]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase();
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && chars[i] == '=' {
                i += 1;
            }
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            let (value, next) = read_value(&chars, i, close);
            i = next;
            if BIB_FIELDS.contains(&name.as_str()) {
                push_line(&mut out, &value);
            }
        }
        if i < chars.len() && chars[i] == close {
            i += 1;
        }
    }
    out
}

pub(crate) fn ris_text(input: &str) -> String {
    let mut out = String::new();
    for line in input.lines() {
        let line = line.trim();
        if line.len() < 4 {
            continue;
        }
        let tag = line[..2].to_ascii_uppercase();
        if !tag.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let Some(value) = line[2..].trim_start().strip_prefix('-') else {
            continue;
        };
        if !RIS_TAGS.iter().any(|t| *t == tag) {
            continue;
        }
        push_line(&mut out, value.trim());
    }
    out
}

fn skip_group(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i] != '{' && chars[i] != '(' {
        if chars[i] == '@' {
            return i;
        }
        i += 1;
    }
    if i >= chars.len() {
        return i;
    }
    let close = if chars[i] == '{' { '}' } else { ')' };
    let mut depth = 0i32;
    while i < chars.len() {
        if chars[i] == '{' || chars[i] == '(' {
            depth += 1;
        } else if chars[i] == close || chars[i] == '}' || chars[i] == ')' {
            depth -= 1;
            if depth <= 0 {
                return i + 1;
            }
        }
        i += 1;
    }
    i
}

fn read_value(chars: &[char], mut i: usize, entry_close: char) -> (String, usize) {
    if i >= chars.len() {
        return (String::new(), i);
    }
    if chars[i] == '{' || chars[i] == '"' {
        let close = if chars[i] == '{' { '}' } else { '"' };
        i += 1;
        let mut depth = 1i32;
        let mut raw = String::new();
        while i < chars.len() && depth > 0 {
            let ch = chars[i];
            if ch == '\\' && i + 1 < chars.len() {
                raw.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if close == '}' && ch == '{' {
                depth += 1;
                i += 1;
                continue;
            }
            if ch == close {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    break;
                }
                continue;
            }
            raw.push(ch);
            i += 1;
        }
        return (collapse_ws(&raw), i);
    }
    let start = i;
    while i < chars.len() && chars[i] != ',' && chars[i] != entry_close {
        i += 1;
    }
    let raw: String = chars[start..i].iter().collect();
    (collapse_ws(raw.trim()), i)
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
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
    fn bib_keeps_fields_and_skips_string_macro() {
        let text = bib_text(
            r#"
            @string{me = {Ada}}
            @article{key,
              title = {Nested {Braces} Title},
              author = {Ada Lovelace and Grace Hopper},
              abstract = {A short abstract},
              year = 1952
            }
            "#,
        );
        assert!(text.contains("Nested Braces Title"));
        assert!(text.contains("Ada Lovelace and Grace Hopper"));
        assert!(text.contains("A short abstract"));
        assert!(!text.contains("1952"));
        assert!(!text.contains("@string"));
    }

    #[test]
    fn ris_keeps_title_author_and_abstract() {
        let text = ris_text(
            "TY  - JOUR\nTI  - Northern Road\nAU  - Lovelace, Ada\nAB  - Field notes\nER  - \n",
        );
        assert!(text.contains("Northern Road"));
        assert!(text.contains("Lovelace, Ada"));
        assert!(text.contains("Field notes"));
        assert!(!text.contains("JOUR"));
    }
}
