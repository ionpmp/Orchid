//! Visual Studio solution extractor.
//!
//! Project names, project paths, and solution items are indexed. Project
//! type GUIDs and configuration tables are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable names from `.sln` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlnExtractor;

#[async_trait]
impl ContentExtractor for SlnExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| ext.eq_ignore_ascii_case("sln"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(sln_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn sln_text(input: &str) -> String {
    let mut out = String::new();
    let mut items = false;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("EndGlobalSection") || line.starts_with("EndProjectSection") {
            items = false;
            continue;
        }
        if let Some(name) = section_name(line) {
            items = name.eq_ignore_ascii_case("SolutionItems");
            continue;
        }
        if line.starts_with("Project(") {
            push_project(&mut out, line);
            continue;
        }
        if items {
            if let Some((left, _)) = line.split_once('=') {
                push_line(&mut out, left.trim());
            }
        }
    }
    out.trim().to_string()
}

fn section_name(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("GlobalSection(")
        .or_else(|| line.strip_prefix("ProjectSection("))?;
    rest.split(')').next()
}

fn push_project(out: &mut String, line: &str) {
    let Some((_, rest)) = line.split_once('=') else {
        return;
    };
    let mut cursor = rest;
    while let Some(start) = cursor.find('"') {
        cursor = &cursor[start + 1..];
        let Some(end) = cursor.find('"') else {
            break;
        };
        let value = &cursor[..end];
        cursor = &cursor[end + 1..];
        if !value.is_empty() && !is_guid(value) {
            push_line(out, value);
        }
    }
}

fn is_guid(value: &str) -> bool {
    let value = value.trim_matches(|c| c == '{' || c == '}');
    let mut parts = value.split('-');
    let Some(a) = parts.next() else {
        return false;
    };
    let Some(b) = parts.next() else {
        return false;
    };
    let Some(c) = parts.next() else {
        return false;
    };
    let Some(d) = parts.next() else {
        return false;
    };
    let Some(e) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && a.len() == 8
        && b.len() == 4
        && c.len() == 4
        && d.len() == 4
        && e.len() == 12
        && value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
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
    fn indexes_projects_and_skips_guids() {
        let text = sln_text(
            "Microsoft Visual Studio Solution File, Format Version 12.00\n\
             # Visual Studio Version 17\n\
             VisualStudioVersion = 17.0.31903.59\n\
             Project(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"Orchid\", \"src\\\\Orchid.csproj\", \"{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}\"\n\
             \tProjectSection(SolutionItems) = preProject\n\
             \t\tREADME.md = README.md\n\
             \tEndProjectSection\n\
             EndProject\n\
             Global\n\
             \tGlobalSection(SolutionConfigurationPlatforms) = preSolution\n\
             \t\tDebug|Any CPU = Debug|Any CPU\n\
             \tEndGlobalSection\n\
             \tGlobalSection(NestedProjects) = preSolution\n\
             \t\t{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE} = {BBBBBBBB-BBBB-CCCC-DDDD-EEEEEEEEEEEE}\n\
             \tEndGlobalSection\n\
             EndGlobal\n",
        );
        assert!(text.contains("Orchid"), "{text}");
        assert!(text.contains("Orchid.csproj"), "{text}");
        assert!(text.contains("README.md"), "{text}");
        assert!(!text.contains("FAE04EC0"), "{text}");
        assert!(!text.contains("AAAAAAAA"), "{text}");
        assert!(!text.contains("Debug"), "{text}");
        assert!(!text.contains("17.0"), "{text}");
        assert!(!text.contains("12.00"), "{text}");
    }
}
