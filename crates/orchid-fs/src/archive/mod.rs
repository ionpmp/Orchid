//! Read-only archive navigation and extraction.
//!
//! Supported formats: ZIP, 7z, TAR (plain), TAR+GZ, TAR+XZ.

pub mod reader;
pub mod sevenz;
pub mod tar;
pub mod types;
pub mod zip;

pub use reader::{detect_format, open_archive, ArchiveReader};
pub use types::{ArchiveEntry, ArchiveFormat};
