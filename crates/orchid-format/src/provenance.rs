//! C2PA Provenance region helpers (Phase 3).
//!
//! `.orchid` is not a native C2PA carrier format, so we sign a tiny PNG
//! carrier that embeds a C2PA manifest and store that signed PNG as the
//! public Provenance region. Third-party validators (`c2pa` Reader /
//! `c2patool`) accept the extracted payload as a normal PNG asset.

use std::io::Cursor;

use orchid_crypto::content::hash_bytes;
use serde_json::json;

use crate::{FormatError, Result};

/// MIME type for the Provenance carrier payload (C2PA-signed PNG).
pub const PROVENANCE_CONTENT_TYPE: &str = "image/png";

/// Assertion label binding Clean-Text integrity into the C2PA manifest.
pub const CLEAN_TEXT_ASSERTION: &str = "com.orchid.clean_text";

/// Minimal 1×1 PNG used as the C2PA carrier asset.
const CARRIER_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC, 0xCF, 0xC0, 0x50,
    0x0F, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xA9, 0x8C, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Result of signing a Provenance carrier.
#[derive(Debug, Clone)]
pub struct SignedProvenance {
    /// Signed PNG bytes (store as Provenance region payload).
    pub carrier_png: Vec<u8>,
    /// Isolated C2PA JUMBF store returned by the signer (also valid sidecar).
    pub c2pa_manifest: Vec<u8>,
    /// BLAKE3 of the Clean-Text that was asserted.
    pub clean_text_blake3: [u8; 32],
}

/// Sign a C2PA manifest over a carrier PNG, asserting Clean-Text integrity.
pub fn sign_clean_text_provenance(clean_text: &[u8], title: &str) -> Result<SignedProvenance> {
    use c2pa::{Builder, BuilderIntent, Context, DigitalSourceType, EphemeralSigner};

    let clean_text_blake3 = hash_bytes(clean_text);
    let blake3_hex = orchid_crypto::content::hex(&clean_text_blake3);

    let signer = EphemeralSigner::new("orchid.format")
        .map_err(|e| FormatError::C2pa(e.to_string()))?;
    let context = Context::new().with_signer(signer);
    let mut builder = Builder::from_context(context)
        .with_definition(json!({ "title": title }))
        .map_err(|e| FormatError::C2pa(e.to_string()))?;
    builder.set_intent(BuilderIntent::Create(DigitalSourceType::Empty));
    builder
        .add_assertion(
            CLEAN_TEXT_ASSERTION,
            &json!({
                "blake3_hex": blake3_hex,
                "bytes": clean_text.len(),
                "encoding": "utf-8",
            }),
        )
        .map_err(|e| FormatError::C2pa(e.to_string()))?;

    let mut source = Cursor::new(CARRIER_PNG);
    let mut dest = Cursor::new(Vec::<u8>::new());
    let c2pa_manifest = builder
        .save_to_stream("image/png", &mut source, &mut dest)
        .map_err(|e| FormatError::C2pa(e.to_string()))?;

    Ok(SignedProvenance {
        carrier_png: dest.into_inner(),
        c2pa_manifest,
        clean_text_blake3,
    })
}

/// Validate a Provenance carrier PNG with the `c2pa` Reader (third-party SDK).
pub fn verify_provenance_carrier(carrier_png: &[u8]) -> Result<c2pa::ValidationState> {
    use c2pa::{Context, Reader};

    let reader = Reader::from_context(Context::new())
        .with_stream("image/png", Cursor::new(carrier_png.to_vec()))
        .map_err(|e| FormatError::C2pa(e.to_string()))?;
    if reader.active_manifest().is_none() {
        return Err(FormatError::C2pa("no active C2PA manifest".into()));
    }
    Ok(reader.validation_state())
}

/// True when the carrier's validation state is cryptographically acceptable.
#[must_use]
pub fn is_c2pa_accepted(state: c2pa::ValidationState) -> bool {
    matches!(
        state,
        c2pa::ValidationState::Valid | c2pa::ValidationState::Trusted
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_carrier() {
        let signed = sign_clean_text_provenance(b"hello provenance\n", "Unit").unwrap();
        assert!(signed.carrier_png.len() > CARRIER_PNG.len());
        let state = verify_provenance_carrier(&signed.carrier_png).unwrap();
        assert!(is_c2pa_accepted(state), "state={state:?}");
    }
}
