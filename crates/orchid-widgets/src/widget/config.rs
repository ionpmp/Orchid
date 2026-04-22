//! Per-widget configuration helpers — serde bincode helpers used by
//! [`crate::Widget::save_state`] / [`crate::Widget::restore_state`].

use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Result, WidgetError};

/// Bincode-serialise `value` to a `Vec<u8>` suitable for returning from
/// [`crate::Widget::save_state`].
///
/// # Errors
///
/// Returns [`WidgetError::CreationFailed`] on encode failure.
pub fn save_state<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|e| WidgetError::CreationFailed(format!("bincode encode: {e}")))
}

/// Inverse of [`save_state`].
///
/// # Errors
///
/// Returns [`WidgetError::CreationFailed`] on decode failure.
pub fn restore_state<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    let (value, _) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
        .map_err(|e| WidgetError::CreationFailed(format!("bincode decode: {e}")))?;
    Ok(value)
}
