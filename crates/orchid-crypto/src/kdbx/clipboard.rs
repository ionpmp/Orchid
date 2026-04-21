//! Abstract trait for platform clipboard integration.
//!
//! The password widget copies a password to the clipboard and schedules an
//! automatic clear after a configured delay (see
//! `privacy.clear_clipboard_seconds` in the Orchid config). The concrete
//! integration with the Windows clipboard lives in `orchid-ui`; this crate
//! only provides the contract.

use async_trait::async_trait;
use secrecy::SecretString;

use crate::error::Result;

/// Abstraction over the system clipboard with auto-clear support.
///
/// Implementations must:
///
/// * Not overwrite a clipboard payload the user has since replaced. This
///   typically means recording a fingerprint (e.g. BLAKE3 hash) of the
///   secret at copy time and comparing it at clear time.
/// * Treat the clipboard as best-effort: `copy_with_auto_clear` returns
///   `Ok(())` once the copy has been scheduled; actual clipboard
///   availability is platform-dependent.
#[async_trait]
pub trait SecureClipboard: Send + Sync {
    /// Copy `secret` to the clipboard and schedule its removal after
    /// `clear_after`. If the user copies something else before the timer
    /// fires, the implementation must leave that value alone.
    ///
    /// # Errors
    ///
    /// Implementations should surface [`crate::CryptoError::Encoding`] or
    /// their own platform-specific variant on failure.
    async fn copy_with_auto_clear(
        &self,
        secret: SecretString,
        clear_after: std::time::Duration,
    ) -> Result<()>;

    /// Immediately clear the clipboard if it still contains one of our
    /// secrets. Returns `true` if a clear was performed.
    ///
    /// # Errors
    ///
    /// Implementations should surface platform errors.
    async fn clear_if_ours(&self) -> Result<bool>;
}
