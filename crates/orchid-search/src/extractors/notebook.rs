//! Jupyter notebook (`.ipynb`) text extractor.
//!
//! Markdown and code cell sources are indexed. Cell outputs, including
//! embedded images, are skipped.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract cell sources from Jupyter notebooks.
#[derive(Debug, Default, Clone, Copy)]
pub struct NotebookExtractor;

#[async_trait]
impl ContentExtractor for NotebookExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/x-ipynb+json")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("ipynb"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(notebook_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn notebook_text(json: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return String::new();
    };
    let Some(cells) = value.get("cells").and_then(|c| c.as_array()) else {
        return String::new();
    };
    let mut out = String::new();
    for cell in cells {
        let kind = cell.get("cell_type").and_then(|c| c.as_str()).unwrap_or("");
        if !matches!(kind, "markdown" | "code" | "raw") {
            continue;
        }
        let Some(source) = cell.get("source") else {
            continue;
        };
        push_line(&mut out, &source_text(source));
    }
    out.trim().to_string()
}

fn source_text(source: &serde_json::Value) -> String {
    match source {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => {
            parts.iter().filter_map(|p| p.as_str()).collect::<String>()
        }
        _ => String::new(),
    }
}

fn push_line(out: &mut String, value: &str) {
    let value = value.trim();
    if value.is_empty() || out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    out.push_str(&value.chars().take(room).collect::<String>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_cell_sources_and_skips_outputs() {
        let text = notebook_text(
            r##"{
              "cells": [
                {"cell_type": "markdown", "source": ["# Title\n", "Intro"]},
                {"cell_type": "code", "source": "print('hello')\n", "outputs": [{"text": "NOTINDEXED"}]},
                {"cell_type": "code", "source": ["x = 1\n"], "outputs": [{"data": {"image/png": "AAAA"}}]}
              ]
            }"##,
        );
        assert!(text.contains("# Title"));
        assert!(text.contains("Intro"));
        assert!(text.contains("print('hello')"));
        assert!(text.contains("x = 1"));
        assert!(!text.contains("NOTINDEXED"));
        assert!(!text.contains("AAAA"));
    }
}
