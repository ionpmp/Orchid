//! Secret-handling primitives.
//!
//! * [`zeroizing::ZeroizingBytes`] — byte buffer wiped on drop.
//! * [`zeroizing::SecretBytes`] — opaque wrapper using [`secrecy`] for
//!   debug / access discipline.
//! * [`dpapi`] — Windows DPAPI wrapper (no-op on non-Windows).
//! * [`stored`] — DPAPI-encoded strings for config / disk (`dpapi:<hex>`).

pub mod dpapi;
pub mod stored;
pub mod zeroizing;

pub use stored::{
    is_protected, protect_for_storage, protect_network_mount_passwords, resolve_stored_secret,
};
pub use zeroizing::{SecretBytes, ZeroizingBytes};
