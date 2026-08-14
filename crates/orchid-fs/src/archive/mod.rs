//! Archive navigation, extraction, and creation.
//!
//! Native read/write: ZIP, TAR, TAR.GZ, TAR.XZ, TAR.BZ2. 7z is read natively.
//! RAR / CAB / ISO / ACE / ARJ / LZH, passwords, volumes, and SFX use 7-Zip
//! when `7z` is on PATH (or `ORCHID_7Z`).

pub mod cli;
pub mod ops;
pub mod reader;
pub mod sevenz;
pub mod tar;
pub mod types;
pub mod zip;

pub use ops::{
    add_to_archive, create_archive, default_extract_dir, delete_from_archive, extract_archive,
    is_archive_file, test_archive,
};
pub use reader::{detect_format, open_archive, ArchiveReader};
pub use types::{
    looks_like_archive_name, looks_like_archive_path, ArchiveEntry, ArchiveFormat,
    ArchiveTestReport, CreateArchiveOptions,
};
