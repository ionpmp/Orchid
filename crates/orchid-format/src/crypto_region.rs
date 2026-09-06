//! Per-region age encrypt / decrypt helpers (Phase 2).

use orchid_crypto::content::hash_bytes;
use orchid_crypto::{Decryptor, Encryptor, Identity, IdentityKind};

use crate::compress::{compress_zstd, decompress_zstd};
use crate::toc::CompressionCodec;
use crate::{FormatError, Result};

/// TOC encryption row for a private region.
#[derive(Debug, Clone)]
pub struct RegionEncryptionSpec {
    /// Optional age header prefix bytes (empty when the full age stream is the payload).
    pub age_header: Vec<u8>,
    /// BLAKE3 of the plaintext after decompress.
    pub plaintext_blake3: [u8; 32],
    /// `0` = passphrase, `1` = X25519.
    pub identity_kind: u8,
    /// Public regions skip age; private regions set `false`.
    pub is_public: bool,
}

/// Result of compressing (and optionally encrypting) a region body.
#[derive(Debug, Clone)]
pub struct PreparedRegionBody {
    /// Bytes written inline or chunked into the store.
    pub stored: Vec<u8>,
    /// Compression applied before encryption.
    pub compression: CompressionCodec,
    /// BLAKE3 of compressed plaintext before encryption.
    pub payload_blake3: [u8; 32],
    /// Present when the region is private/age-encrypted.
    pub encryption: Option<RegionEncryptionSpec>,
}

/// Prepare a region body: optional zstd, then optional age.
pub fn prepare_region_body(
    plaintext: &[u8],
    compression: CompressionCodec,
    identity: Option<&Identity>,
) -> Result<PreparedRegionBody> {
    let compressed = match compression {
        CompressionCodec::None => plaintext.to_vec(),
        CompressionCodec::Zstd => compress_zstd(plaintext)?,
        other => return Err(FormatError::UnsupportedCompression(other.0)),
    };
    let payload_blake3 = hash_bytes(&compressed);
    let Some(identity) = identity else {
        return Ok(PreparedRegionBody {
            stored: compressed,
            compression,
            payload_blake3,
            encryption: None,
        });
    };
    let encryptor = Encryptor::new(identity.clone());
    let ciphertext = encryptor
        .encrypt_bytes(&compressed)
        .map_err(FormatError::from)?;
    let encryption = Some(RegionEncryptionSpec {
        age_header: Vec::new(),
        plaintext_blake3: hash_bytes(plaintext),
        identity_kind: identity_kind_byte(identity.kind()),
        is_public: false,
    });
    Ok(PreparedRegionBody {
        stored: ciphertext,
        compression,
        payload_blake3,
        encryption,
    })
}

/// Reverse [`prepare_region_body`]: decrypt if needed, verify hashes, decompress.
pub fn decode_region_body(
    stored: &[u8],
    compression: CompressionCodec,
    payload_blake3: Option<[u8; 32]>,
    encryption: Option<&RegionEncryptionSpec>,
    identity: Option<&Identity>,
) -> Result<Vec<u8>> {
    let compressed = if let Some(enc) = encryption {
        if enc.is_public {
            stored.to_vec()
        } else {
            let identity = identity.ok_or(FormatError::IdentityRequired)?;
            let decryptor = Decryptor::new(identity.clone());
            let plain_compressed = decryptor.decrypt_bytes(stored).map_err(FormatError::from)?;
            plain_compressed.as_slice().to_vec()
        }
    } else {
        stored.to_vec()
    };

    if let Some(expected) = payload_blake3 {
        if hash_bytes(&compressed) != expected {
            return Err(FormatError::RegionDecode(
                "payload BLAKE3 mismatch (compressed)".into(),
            ));
        }
    }

    let plaintext = match compression {
        CompressionCodec::None => compressed,
        CompressionCodec::Zstd => decompress_zstd(&compressed)?,
        other => return Err(FormatError::UnsupportedCompression(other.0)),
    };

    if let Some(enc) = encryption {
        if !enc.is_public && hash_bytes(&plaintext) != enc.plaintext_blake3 {
            return Err(FormatError::RegionDecode(
                "plaintext BLAKE3 mismatch after decrypt".into(),
            ));
        }
    }
    Ok(plaintext)
}

fn identity_kind_byte(kind: IdentityKind) -> u8 {
    match kind {
        IdentityKind::Passphrase => 0,
        IdentityKind::X25519 => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_crypto::Identity;

    #[test]
    fn encrypt_roundtrip_zstd() {
        let id = Identity::passphrase("phase2-test");
        let plain = b"secret clean text\n".repeat(20);
        let (stored, codec, payload_hash, enc) = {
            let p = prepare_region_body(&plain, CompressionCodec::Zstd, Some(&id)).unwrap();
            (p.stored, p.compression, p.payload_blake3, p.encryption)
        };
        assert_eq!(codec, CompressionCodec::Zstd);
        assert!(enc.is_some());
        assert_ne!(stored, plain);
        let back = decode_region_body(&stored, codec, Some(payload_hash), enc.as_ref(), Some(&id))
            .unwrap();
        assert_eq!(back, plain);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let id = Identity::passphrase("right");
        let (stored, codec, payload_hash, enc) = {
            let p = prepare_region_body(b"hello", CompressionCodec::None, Some(&id)).unwrap();
            (p.stored, p.compression, p.payload_blake3, p.encryption)
        };
        let wrong = Identity::passphrase("wrong");
        let err = decode_region_body(
            &stored,
            codec,
            Some(payload_hash),
            enc.as_ref(),
            Some(&wrong),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            FormatError::Crypto(_) | FormatError::RegionDecode(_)
        ));
    }
}
