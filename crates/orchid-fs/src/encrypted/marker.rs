//! Convention-based detection of encrypted paths.

use crate::path::FsPath;

/// File extension used for age-encrypted payloads.
pub const AGE_EXT: &str = "age";

/// Encrypted directory archive file (tar.age), stored inside the folder.
pub const DIR_ARCHIVE_NAME: &str = ".orchid-encrypted.tar.age";

/// Encrypted directory metadata sidecar, stored inside the folder.
pub const DIR_META_NAME: &str = ".orchid-encrypted.meta";

/// Returns true when `path` looks like an age-encrypted file (has `.age`
/// extension and a matching sidecar is expected nearby).
#[must_use]
pub fn looks_encrypted(path: &FsPath) -> bool {
    path.extension().map(|e| e.eq_ignore_ascii_case(AGE_EXT)).unwrap_or(false)
}

/// Returns true when `path` is a directory containing Orchid's encrypted-folder
/// marker files.
#[must_use]
pub fn looks_encrypted_directory(path: &FsPath) -> bool {
    let Ok(local) = path.to_local() else {
        return false;
    };
    local.join(DIR_ARCHIVE_NAME).is_file() && local.join(DIR_META_NAME).is_file()
}
