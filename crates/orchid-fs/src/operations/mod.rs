//! File operations (copy, move, delete, recycle, compare, hash, encode, attributes).

pub mod attrs;
pub mod checksum;
pub mod compare;
pub mod copy;
pub mod delete;
pub mod encode;
pub mod inspect;
pub mod link;
#[path = "move_.rs"]
pub mod move_;
pub mod progress;
pub mod recycle;
pub mod signature;
pub mod split;

pub use attrs::{
    acl_grant, acl_reset, acl_text, apply_attr_patch, apply_name_case, chmod, chown, parse_mode,
    parse_timestamp, set_mtime, AttrPatch, NameCase,
};
pub use checksum::{
    format_hash_report, format_verify_report, hash_path, hash_paths, is_hash_sidecar,
    verify_hash_file, write_hash_file, HashAlgo, HashRecord, VerifyRow,
};
pub use compare::{
    compare_dirs, compare_files, sync_dirs, DiffKind, DirDiff, DirDiffEntry, FileCompare, SyncMode,
    SyncStats,
};
pub use copy::{copy, copy_with_control, CopyOptions};
pub use delete::{delete, DeleteOptions};
pub use encode::{
    decode_base64, decode_uue, decoded_path, encode_base64, encode_uue, sidecar_path,
};
pub use inspect::{inspect_path, FileProperties};
pub use link::{create_hard_link, create_junction, create_symlink};
pub use move_::move_;
pub use progress::{OperationProgress, ProgressSink, TransferControl};
pub use recycle::{
    empty_recycle, is_recycle_item, is_recycle_listing, is_recycle_virtual, list_recycle,
    purge_recycle, recycle_entries, recycle_item_id, recycle_listing_supported,
    recycle_original_path, restore_recycle, RecycleItem, RECYCLE_PATH,
};
pub use signature::{inspect_signature, SignatureReport};
pub use split::{discover_parts, join_files, join_output_name, split_file};
