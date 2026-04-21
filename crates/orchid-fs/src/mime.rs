//! MIME detection for [`FsPath`]s.

use crate::path::FsPath;

/// Best-effort MIME lookup: extension first, fall back to content sniffing
/// if the caller can supply a sample prefix.
///
/// Returns `None` when neither source is conclusive.
///
/// # Examples
///
/// ```no_run
/// # async fn demo() {
/// use orchid_fs::{guess_mime, FsPath};
/// let path = FsPath::new("local:/tmp/file.png").unwrap();
/// let mime = guess_mime(&path, None).await;
/// assert_eq!(mime.as_deref(), Some("image/png"));
/// # }
/// ```
pub async fn guess_mime(path: &FsPath, sample_bytes: Option<&[u8]>) -> Option<String> {
    // Extension-based guess has the lowest cost; try it first.
    let ext_guess = path
        .extension()
        .map(|e| mime_guess::from_ext(e).first_or_octet_stream().essence_str().to_string());

    // If we have a sample, verify against magic bytes and promote a
    // content-based answer when it differs from the extension guess.
    if let Some(bytes) = sample_bytes {
        if let Some(kind) = infer::get(bytes) {
            let mime_from_bytes = kind.mime_type().to_string();
            match &ext_guess {
                Some(ext) if ext == &mime_from_bytes => return Some(ext.clone()),
                Some(ext) if ext == "application/octet-stream" => return Some(mime_from_bytes),
                Some(_) | None => return Some(mime_from_bytes),
            }
        }
    }
    ext_guess.filter(|m| m != "application/octet-stream")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn guess_by_extension() {
        let p = FsPath::new("local:/tmp/a.png").unwrap();
        assert_eq!(guess_mime(&p, None).await.as_deref(), Some("image/png"));

        let p = FsPath::new("local:/tmp/a.txt").unwrap();
        assert_eq!(guess_mime(&p, None).await.as_deref(), Some("text/plain"));
    }

    #[tokio::test]
    async fn content_overrides_mislabeled_extension() {
        // A zip file incorrectly named `.txt`.
        let p = FsPath::new("local:/tmp/wrong.txt").unwrap();
        let magic = b"PK\x03\x04fake-zip-header-content";
        let got = guess_mime(&p, Some(magic)).await;
        assert_eq!(got.as_deref(), Some("application/zip"));
    }

    #[tokio::test]
    async fn unknown_returns_none() {
        let p = FsPath::new("local:/tmp/no-extension").unwrap();
        assert!(guess_mime(&p, None).await.is_none());
    }
}
