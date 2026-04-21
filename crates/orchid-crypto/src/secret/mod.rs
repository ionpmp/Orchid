//! Secret-handling primitives.
//!
//! * [`zeroizing::ZeroizingBytes`] — byte buffer wiped on drop.
//! * [`zeroizing::SecretBytes`] — opaque wrapper using [`secrecy`] for
//!   debug / access discipline.
//! * [`dpapi`] — Windows DPAPI wrapper (no-op on non-Windows).

pub mod dpapi;
pub mod zeroizing;

pub use zeroizing::{SecretBytes, ZeroizingBytes};
