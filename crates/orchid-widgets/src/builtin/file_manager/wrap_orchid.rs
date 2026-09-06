//! FM “Wrap as .orchid” — pack selection into sealed or linked containers.

use std::sync::Arc;

use orchid_format::{
    default_wrap_output, wrap_as_linked, wrap_as_sealed, WrapAsOrchidRequest, EXTENSION,
};
use orchid_fs::FsPath;
use orchid_search::Extractor;

use super::map_fs_error;
use super::ActionOutcome;
use super::FileManagerInner;
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

/// Dispatch wrap action.
pub(super) async fn run(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    if paths.is_empty() {
        return Ok(ActionOutcome::Done);
    }

    let extractor = Extractor::new();
    let mut names: Vec<String> = Vec::new();

    for path_str in paths {
        let src = FsPath::new(path_str).map_err(map_fs_error)?;
        if !src.is_local() {
            continue;
        }
        let local = src.to_local().map_err(map_fs_error)?;
        if local.is_dir() {
            continue;
        }
        let Some(file_name) = local.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if file_name.ends_with(EXTENSION) || file_name.ends_with(".orchid") {
            continue;
        }

        let meta = tokio::fs::metadata(&local)
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("wrap-orchid-stat: {e}")))?;
        // Cap Raw at 256 MiB for an interactive wrap (larger → use linked later).
        if meta.len() > 256 * 1024 * 1024 {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-wrap-orchid-too-large".into(),
            ));
        }

        let raw = tokio::fs::read(&local)
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("wrap-orchid-read: {e}")))?;

        let provider = inner
            .deps
            .registry
            .for_path(&src)
            .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-wrap-orchid".into()))?;
        let clean_text = match extractor
            .extract(provider.as_ref(), &src, None)
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        {
            Some(s) => s.into_bytes(),
            None => file_name.as_bytes().to_vec(),
        };

        let output = default_wrap_output(&local);
        if output.exists() {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-wrap-orchid-exists".into(),
            ));
        }

        let ctype = mime_guess_from_name(file_name);
        let embeddings = orchid_embed::stub_embedding_payload(
            &String::from_utf8_lossy(&clean_text),
        )
        .ok()
        .flatten();
        let req = WrapAsOrchidRequest {
            output: output.clone(),
            raw,
            raw_name: Some(file_name.to_string()),
            raw_content_type: ctype,
            clean_text,
            embeddings,
        };

        let out_key = FsPath::from_local(&output)
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|_| output.to_string_lossy().into_owned());
        if let (Some(_), Some(store)) = (
            inner.managed_root_for_path(&out_key),
            inner.deps.chunk_store.as_ref(),
        ) {
            wrap_as_linked(&req, store.as_ref())
                .await
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        } else {
            wrap_as_sealed(&req)
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        }

        names.push(
            output
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("wrapped.orchid")
                .to_string(),
        );
    }

    if names.is_empty() {
        return Ok(ActionOutcome::Done);
    }

    inner.refresh_all_tabs().await;
    let body = if names.len() == 1 {
        inner.deps.locale.tr_args(
            "fm-wrap-orchid-done",
            &orchid_i18n::FluentArgs::new().with("name", names[0].clone()),
        )
    } else {
        inner.deps.locale.tr_args(
            "fm-wrap-orchid-done-many",
            &orchid_i18n::FluentArgs::new().with("count", names.len().to_string()),
        )
    };
    Ok(ActionOutcome::NeedsReport {
        title: inner.deps.locale.tr("fm-wrap-orchid-title"),
        body,
    })
}

fn mime_guess_from_name(name: &str) -> Option<String> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "txt" | "log" | "md" | "markdown" | "csv" | "tsv" => "text/plain",
            "json" => "application/json",
            "xml" | "html" | "htm" => "text/html",
            "pdf" => "application/pdf",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            _ => "application/octet-stream",
        }
        .to_string(),
    )
}
