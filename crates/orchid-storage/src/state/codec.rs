//! redb `Value` adapter that encodes Rust values through `bincode`.
//!
//! [`Value<T>`] is a zero-sized type that implements [`redb::Value`] for any
//! `T: bincode::Encode + bincode::Decode<()>`, letting us declare tables like:
//!
//! ```ignore
//! use redb::TableDefinition;
//! use orchid_storage::state::{codec::Value, types::Workspace};
//!
//! const WORKSPACES: TableDefinition<&[u8; 16], Value<Workspace>> =
//!     TableDefinition::new("workspaces");
//! ```
//!
//! The adapter is intentionally unsuitable as a `redb::Key`: ordering over
//! bincode-encoded blobs is not meaningful. Use the raw key types that redb
//! supports directly (`&str`, `&[u8]`, fixed-size arrays, integers) instead.

use std::fmt::Debug;
use std::marker::PhantomData;

use bincode::config;

use crate::error::Result;

/// Zero-sized redb `Value` adapter for types that implement bincode's
/// `Encode` + `Decode<()>`.
#[derive(Debug)]
pub struct Value<T>(PhantomData<T>);

impl<T> redb::Value for Value<T>
where
    T: bincode::Encode + bincode::Decode<()> + Debug + 'static,
{
    type SelfType<'a>
        = T
    where
        Self: 'a;
    type AsBytes<'a>
        = Vec<u8>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        // redb's `Value` trait is infallible by design. A decode failure here
        // means the on-disk database is corrupted -- there is no recovery
        // path short of restoring from a backup, so panicking with a clear
        // message is the accepted pattern.
        #[allow(clippy::expect_used)]
        {
            let (value, _) = bincode::decode_from_slice(data, config::standard())
                .expect("orchid-storage: bincode decode failed (database corrupted?)");
            value
        }
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        // Same reasoning: encoding a well-typed value into a `Vec<u8>` cannot
        // fail unless the encoder runs out of memory, which is unrecoverable.
        #[allow(clippy::expect_used)]
        {
            bincode::encode_to_vec(value, config::standard())
                .expect("orchid-storage: bincode encode should not fail")
        }
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new(&format!(
            "orchid_storage::Value<{}>",
            std::any::type_name::<T>()
        ))
    }
}

/// Encode a value to a `Vec<u8>` using the workspace-standard bincode config.
///
/// Exposed for tests and for callers that need to hand-craft a payload (for
/// example to seed a database for migration tests).
///
/// # Errors
///
/// Propagates [`crate::StorageError::Bincode`] if encoding fails.
pub fn bincode_encode<T>(value: &T) -> Result<Vec<u8>>
where
    T: bincode::Encode,
{
    Ok(bincode::encode_to_vec(value, config::standard())?)
}

/// Decode a value from a byte slice using the workspace-standard bincode
/// config.
///
/// # Errors
///
/// Propagates [`crate::StorageError::BincodeDecode`] if decoding fails.
pub fn bincode_decode<T>(bytes: &[u8]) -> Result<T>
where
    T: bincode::Decode<()>,
{
    let (value, _) = bincode::decode_from_slice(bytes, config::standard())?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::types::{GridPosition, LifecycleState, WidgetInstance, WidgetSize};
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn bincode_roundtrip_for_representative_type() {
        let w = WidgetInstance {
            id: Uuid::new_v4(),
            widget_type: "rss".into(),
            workspace_id: Uuid::new_v4(),
            position: GridPosition { col: 0, row: 0 },
            size: WidgetSize::Medium,
            lifecycle: LifecycleState::Active,
            config: vec![42],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let bytes = bincode_encode(&w).unwrap();
        let back: WidgetInstance = bincode_decode(&bytes).unwrap();
        assert_eq!(w.widget_type, back.widget_type);
        assert_eq!(w.lifecycle, back.lifecycle);
        assert_eq!(w.size, back.size);
    }
}
