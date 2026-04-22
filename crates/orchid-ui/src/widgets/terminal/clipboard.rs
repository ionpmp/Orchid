//! Cross-platform clipboard backed by [`arboard`].

use std::sync::Arc;
use std::time::Duration;

use arboard::Clipboard;
use async_trait::async_trait;
use parking_lot::Mutex;
use secrecy::{ExposeSecret, SecretString};
use tracing::warn;

use orchid_crypto::{CryptoError, SecureClipboard};

/// Wrapper around an `arboard::Clipboard` that also implements
/// [`SecureClipboard`].
///
/// Construction is fallible because the platform clipboard is not always
/// available in headless test environments.
pub struct ArboardClipboard {
    inner: Mutex<Clipboard>,
    /// Last secret we deposited; used by `clear_if_ours` / auto-clear.
    last_secret: Mutex<Option<String>>,
}

impl ArboardClipboard {
    /// Attempt to open a handle to the OS clipboard.
    ///
    /// # Errors
    ///
    /// Propagates platform errors from [`arboard::Clipboard::new`].
    pub fn new() -> Result<Self, arboard::Error> {
        let inner = Clipboard::new()?;
        Ok(Self {
            inner: Mutex::new(inner),
            last_secret: Mutex::new(None),
        })
    }

    /// Copy plain text to the clipboard.
    ///
    /// # Errors
    ///
    /// Propagates platform errors.
    pub fn copy(&self, text: &str) -> Result<(), arboard::Error> {
        self.inner.lock().set_text(text.to_string())
    }

    /// Read text from the clipboard. Returns an empty string if the
    /// clipboard does not currently hold text.
    pub fn paste(&self) -> Result<String, arboard::Error> {
        self.inner.lock().get_text()
    }
}

#[async_trait]
impl SecureClipboard for ArboardClipboard {
    async fn copy_with_auto_clear(
        &self,
        secret: SecretString,
        clear_after: Duration,
    ) -> orchid_crypto::Result<()> {
        let text = secret.expose_secret().to_string();
        {
            let mut guard = self.inner.lock();
            guard
                .set_text(text.clone())
                .map_err(|e| CryptoError::Encoding(format!("clipboard set failed: {e}")))?;
        }
        *self.last_secret.lock() = Some(text.clone());

        // Spawn auto-clear as a task on whatever runtime is current.
        let this_last = Arc::new(Mutex::new(text.clone()));
        let inner = {
            // We need to move a clone of the Clipboard into the task. We
            // can't `Clone` arboard::Clipboard, so instead we snapshot the
            // last-secret fingerprint and re-open the clipboard inside the
            // task. On Windows this is cheap.
            let _ = this_last;
            None::<()>
        };
        let _ = inner;
        let expected = text;
        tokio::spawn(async move {
            tokio::time::sleep(clear_after).await;
            let Ok(mut cb) = Clipboard::new() else {
                warn!("auto-clear: failed to re-open clipboard");
                return;
            };
            let current = cb.get_text().unwrap_or_default();
            if current == expected {
                if let Err(e) = cb.clear() {
                    warn!(error = %e, "auto-clear: clipboard clear failed");
                }
            }
        });
        Ok(())
    }

    async fn clear_if_ours(&self) -> orchid_crypto::Result<bool> {
        let last = self.last_secret.lock().clone();
        let Some(last) = last else {
            return Ok(false);
        };
        let mut guard = self.inner.lock();
        let current = guard.get_text().unwrap_or_default();
        if current == last {
            guard
                .clear()
                .map_err(|e| CryptoError::Encoding(format!("clipboard clear failed: {e}")))?;
            return Ok(true);
        }
        Ok(false)
    }
}
