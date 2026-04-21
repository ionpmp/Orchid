//! Convention-based detection of encrypted paths.

use crate::path::FsPath;

/// File extension used for age-encrypted payloads.
pub const AGE_EXT: &str = "age";

/// Returns true when `path` looks like an age-encrypted file (has `.age`
/// extension and a matching sidecar is expected nearby).
#[must_use]
pub fn looks_encrypted(path: &FsPath) -> bool {
    path.extension().map(|e| e.eq_ignore_ascii_case(AGE_EXT)).unwrap_or(false)
}
