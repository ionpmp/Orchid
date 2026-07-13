//! Encrypted paths — files and folders stored on disk as `.age` ciphertexts.
//!
//! # Security
//!
//! The engine persists only [`orchid_crypto::IdentityKind`] so that the
//! `fs_encrypted_paths` table knows which paths are encrypted and which
//! auth flow to use. The actual passphrase / X25519 key is NEVER written
//! to disk by this crate — it is supplied fresh at every reveal and lives
//! only in memory for the duration of the operation.
//!
//! In-place encrypt may overwrite the plaintext with zeros before unlink.
//! That is best-effort only — see `docs/SECURITY.md` ("Disk wipe after
//! encryption / reveal") for SSD / NTFS limits.

pub mod engine;
pub(crate) mod index;
pub mod marker;

pub use engine::{EncryptedFolderConfig, EncryptedFolderEngine, EncryptedPathRegistered};
pub use index::EncryptedFolderRecord;
pub use marker::{looks_encrypted, looks_encrypted_directory, AGE_EXT, DIR_ARCHIVE_NAME, DIR_META_NAME};
