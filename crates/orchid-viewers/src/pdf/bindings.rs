//! Runtime binding to the Pdfium shared library.

use std::cell::RefCell;
use std::path::PathBuf;

use pdfium_render::prelude::*;

use crate::error::{Result, ViewerError};

thread_local! {
    static PDFIUM: RefCell<Option<Pdfium>> = const { RefCell::new(None) };
}

/// Run `f` with a thread-local Pdfium instance, binding on first use.
///
/// # Errors
///
/// Returns [`ViewerError::PdfUnavailable`] when no Pdfium library can be loaded.
pub fn with_pdfium<R>(f: impl FnOnce(&Pdfium) -> Result<R>) -> Result<R> {
    PDFIUM.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(bind_pdfium()?);
        }
        f(slot.as_ref().expect("Pdfium initialized above"))
    })
}

fn bind_pdfium() -> Result<Pdfium> {
    for dir in candidate_library_dirs() {
        let lib_path = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if lib_path.is_file() {
            if let Ok(bindings) = Pdfium::bind_to_library(&lib_path) {
                tracing::info!(path = %lib_path.display(), "loaded Pdfium");
                return Ok(Pdfium::new(bindings));
            }
        }
    }

    if let Ok(bindings) = Pdfium::bind_to_system_library() {
        tracing::info!("loaded Pdfium from system library search path");
        return Ok(Pdfium::new(bindings));
    }

    Err(ViewerError::PdfUnavailable)
}

fn candidate_library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let bundled = PathBuf::from(manifest).join("../../third-party/pdfium/win-x64");
        dirs.push(bundled);
    }
    dirs
}
