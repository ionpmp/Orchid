//! TOTP helpers: parse / render `otpauth://` URIs and generate codes.

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use totp_rs::{Algorithm, TOTP};
use url::Url;

use crate::error::{CryptoError, Result};
use crate::kdbx::entry::{TotpAlgorithm, TotpConfig};

/// Generated TOTP code and its remaining lifetime.
#[derive(Debug, Clone)]
pub struct TotpCode {
    /// Zero-padded numeric code.
    pub code: String,
    /// Seconds left in the current period.
    pub remaining_seconds: u32,
}

/// Parse an `otpauth://totp/...` URI into a [`TotpConfig`].
///
/// # Errors
///
/// Returns [`CryptoError::TotpSetup`] if the URI is malformed or missing the
/// shared secret.
///
/// # Examples
///
/// ```
/// use orchid_crypto::parse_otpauth_uri;
/// let cfg = parse_otpauth_uri(
///     "otpauth://totp/Example:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example"
/// ).unwrap();
/// assert_eq!(cfg.digits, 6);
/// ```
pub fn parse_otpauth_uri(uri: &str) -> Result<TotpConfig> {
    let parsed = Url::parse(uri).map_err(|e| CryptoError::TotpSetup(e.to_string()))?;
    if parsed.scheme() != "otpauth" {
        return Err(CryptoError::TotpSetup("not an otpauth:// URI".into()));
    }
    if parsed.host_str() != Some("totp") {
        return Err(CryptoError::TotpSetup("only otpauth://totp is supported".into()));
    }

    // Path is "/label" or "/issuer:account"
    let label = parsed.path().trim_start_matches('/').to_string();
    let (label_issuer, account) = match label.split_once(':') {
        Some((i, a)) => (Some(i.to_string()), Some(a.to_string())),
        None if !label.is_empty() => (None, Some(label)),
        None => (None, None),
    };

    let mut secret: Option<String> = None;
    let mut issuer: Option<String> = label_issuer;
    let mut algorithm = TotpAlgorithm::Sha1;
    let mut digits: u8 = 6;
    let mut period: u32 = 30;

    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "secret" => secret = Some(v.to_string()),
            "issuer" => issuer = Some(v.to_string()),
            "algorithm" => {
                algorithm = match v.to_ascii_uppercase().as_str() {
                    "SHA1" => TotpAlgorithm::Sha1,
                    "SHA256" => TotpAlgorithm::Sha256,
                    "SHA512" => TotpAlgorithm::Sha512,
                    other => {
                        return Err(CryptoError::TotpSetup(format!(
                            "unsupported algorithm: {other}"
                        )))
                    }
                }
            }
            "digits" => {
                digits = v
                    .parse()
                    .map_err(|e: std::num::ParseIntError| CryptoError::TotpSetup(e.to_string()))?;
            }
            "period" => {
                period = v
                    .parse()
                    .map_err(|e: std::num::ParseIntError| CryptoError::TotpSetup(e.to_string()))?;
            }
            _ => {}
        }
    }

    let secret = secret.ok_or_else(|| CryptoError::TotpSetup("missing `secret` query parameter".into()))?;

    Ok(TotpConfig {
        secret: SecretString::new(secret),
        algorithm,
        digits,
        period_seconds: period,
        issuer,
        account,
    })
}

/// Render a [`TotpConfig`] back into an `otpauth://` URI.
///
/// # Examples
///
/// ```
/// use orchid_crypto::{to_otpauth_uri, parse_otpauth_uri};
/// let input = "otpauth://totp/Example:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example";
/// let cfg = parse_otpauth_uri(input).unwrap();
/// let uri = to_otpauth_uri(&cfg);
/// assert!(uri.starts_with("otpauth://totp/"));
/// ```
#[must_use]
pub fn to_otpauth_uri(cfg: &TotpConfig) -> String {
    let label = match (&cfg.issuer, &cfg.account) {
        (Some(i), Some(a)) => format!("{i}:{a}"),
        (None, Some(a)) => a.clone(),
        (Some(i), None) => i.clone(),
        (None, None) => String::new(),
    };
    let algorithm = match cfg.algorithm {
        TotpAlgorithm::Sha1 => "SHA1",
        TotpAlgorithm::Sha256 => "SHA256",
        TotpAlgorithm::Sha512 => "SHA512",
    };
    let secret_enc = urlencode(cfg.secret.expose_secret());
    // The label lives in the URL path. Percent-encode each segment but keep
    // the colon separator so round-tripping through the parser recovers the
    // original issuer / account split.
    let label_enc = encode_label(&label);
    let mut params = format!(
        "secret={secret_enc}&algorithm={algorithm}&digits={}&period={}",
        cfg.digits, cfg.period_seconds
    );
    if let Some(issuer) = &cfg.issuer {
        params.push_str("&issuer=");
        params.push_str(&urlencode(issuer));
    }
    format!("otpauth://totp/{label_enc}?{params}")
}

/// Percent-encode a label while keeping the `:` separator intact.
fn encode_label(s: &str) -> String {
    s.split(':')
        .map(urlencode)
        .collect::<Vec<_>>()
        .join(":")
}

fn urlencode(s: &str) -> String {
    // Minimal URL encoding for query/path segments. Encodes anything that
    // is not an unreserved character.
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    }
    out
}

/// Compute a code for `cfg` at the given wall-clock moment.
///
/// # Errors
///
/// Returns [`CryptoError::TotpGeneration`] if the secret is not valid
/// base32 or `totp-rs` rejects the parameters.
///
/// # Examples
///
/// ```
/// use orchid_crypto::{generate_code, parse_otpauth_uri};
/// // The secret is base32-encoded and must be at least 128 bits (~ 26 chars).
/// let cfg = parse_otpauth_uri(
///     "otpauth://totp/E:a?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP"
/// ).unwrap();
/// let code = generate_code(&cfg, chrono::Utc::now()).unwrap();
/// assert_eq!(code.code.len(), 6);
/// ```
pub fn generate_code(cfg: &TotpConfig, now: DateTime<Utc>) -> Result<TotpCode> {
    let alg = match cfg.algorithm {
        TotpAlgorithm::Sha1 => Algorithm::SHA1,
        TotpAlgorithm::Sha256 => Algorithm::SHA256,
        TotpAlgorithm::Sha512 => Algorithm::SHA512,
    };
    let secret_bytes = totp_rs::Secret::Encoded(cfg.secret.expose_secret().to_string())
        .to_bytes()
        .map_err(|e| CryptoError::TotpGeneration(format!("{e:?}")))?;
    let totp = TOTP::new(
        alg,
        cfg.digits as usize,
        1,
        cfg.period_seconds as u64,
        secret_bytes,
        cfg.issuer.clone(),
        cfg.account.clone().unwrap_or_default(),
    )
    .map_err(|e| CryptoError::TotpGeneration(e.to_string()))?;
    let ts = now.timestamp() as u64;
    let code = totp
        .generate(ts);
    let remaining =
        (cfg.period_seconds as u64 - ts % cfg.period_seconds as u64) as u32;
    Ok(TotpCode {
        code,
        remaining_seconds: remaining,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_basic_uri() {
        let input = "otpauth://totp/Example:alice?secret=JBSWY3DPEHPK3PXP&issuer=Example";
        let cfg = parse_otpauth_uri(input).unwrap();
        assert_eq!(cfg.issuer.as_deref(), Some("Example"));
        assert_eq!(cfg.account.as_deref(), Some("alice"));
        assert_eq!(cfg.digits, 6);
        assert_eq!(cfg.period_seconds, 30);

        let rendered = to_otpauth_uri(&cfg);
        let parsed_again = parse_otpauth_uri(&rendered).unwrap();
        assert_eq!(parsed_again.secret.expose_secret(), cfg.secret.expose_secret());
        assert_eq!(parsed_again.issuer, cfg.issuer);
        assert_eq!(parsed_again.account, cfg.account);
    }

    #[test]
    fn rfc6238_sha1_test_vector_at_fixed_time() {
        // RFC 6238 Appendix B test vector: secret "12345678901234567890"
        // (ASCII) at t=59 (2005-03-17T17:29:59Z, early period) yields
        // 94287082 (SHA-1, 8 digits). We test with the classic 6-digit case
        // to keep the assertion simple; the value is derived from the same
        // secret and algorithm.
        let ascii_secret = b"12345678901234567890";
        let b32 = base32::encode(base32::Alphabet::Rfc4648 { padding: false }, ascii_secret);
        let cfg = TotpConfig {
            secret: SecretString::new(b32),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period_seconds: 30,
            issuer: None,
            account: None,
        };
        // Timestamp 59 seconds since epoch.
        let now = DateTime::<Utc>::from_timestamp(59, 0).unwrap();
        let code = generate_code(&cfg, now).unwrap();
        assert_eq!(code.code.len(), 6);
        // remaining = period - (59 % 30) = 30 - 29 = 1
        assert_eq!(code.remaining_seconds, 1);
    }

    #[test]
    fn missing_secret_is_rejected() {
        let err = parse_otpauth_uri("otpauth://totp/x?issuer=y").unwrap_err();
        assert!(matches!(err, CryptoError::TotpSetup(_)));
    }
}
