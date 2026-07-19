//! Biometric user verification (Windows Hello on Windows).

use crate::error::Result;

/// Whether the platform can prompt for biometric verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiometricAvailability {
    /// Fingerprint / face / PIN verification can be requested.
    Available,
    /// Hello is not configured on this device.
    NotConfigured,
    /// Policy or hardware disables verification.
    DisabledByPolicy,
    /// Not supported on this OS build.
    Unavailable,
}

/// Outcome of a verification prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiometricVerification {
    /// User verified successfully.
    Verified,
    /// User dismissed the prompt.
    Cancelled,
    /// Sensor busy or temporarily unavailable.
    DeviceBusy,
    /// Other failure.
    Failed,
}

/// Query whether Windows Hello (or equivalent) can be used.
pub fn check_availability() -> BiometricAvailability {
    imp::check_availability()
}

/// Show a system biometric consent prompt.
///
/// # Errors
///
/// Returns [`CryptoError::BiometricUnavailable`] when the platform cannot
/// show a prompt.
pub fn verify_user(message: &str) -> Result<BiometricVerification> {
    imp::verify_user(message)
}

#[cfg(windows)]
mod imp {
    use windows::core::HSTRING;
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier,
        UserConsentVerifierAvailability,
    };
    use windows_future::IAsyncOperation;

    use super::{BiometricAvailability, BiometricVerification};
    use crate::error::{CryptoError, Result};

    pub fn check_availability() -> BiometricAvailability {
        match UserConsentVerifier::CheckAvailabilityAsync() {
            Ok(op) => map_availability(op),
            Err(_) => BiometricAvailability::Unavailable,
        }
    }

    pub fn verify_user(message: &str) -> Result<BiometricVerification> {
        if check_availability() != BiometricAvailability::Available {
            return Err(CryptoError::BiometricUnavailable);
        }
        let op = UserConsentVerifier::RequestVerificationAsync(&HSTRING::from(message))
            .map_err(|e| CryptoError::BiometricFailed(e.to_string()))?;
        map_verification(op)
    }

    fn map_availability(
        op: IAsyncOperation<UserConsentVerifierAvailability>,
    ) -> BiometricAvailability {
        match op.join() {
            Ok(UserConsentVerifierAvailability::Available) => BiometricAvailability::Available,
            Ok(UserConsentVerifierAvailability::DeviceNotPresent) => {
                BiometricAvailability::NotConfigured
            }
            Ok(UserConsentVerifierAvailability::DisabledByPolicy) => {
                BiometricAvailability::DisabledByPolicy
            }
            Ok(UserConsentVerifierAvailability::DeviceBusy) => BiometricAvailability::Unavailable,
            Ok(_) => BiometricAvailability::Unavailable,
            Err(_) => BiometricAvailability::Unavailable,
        }
    }

    fn map_verification(
        op: IAsyncOperation<UserConsentVerificationResult>,
    ) -> Result<BiometricVerification> {
        match op.join() {
            Ok(UserConsentVerificationResult::Verified) => Ok(BiometricVerification::Verified),
            Ok(UserConsentVerificationResult::Canceled) => Ok(BiometricVerification::Cancelled),
            Ok(UserConsentVerificationResult::DeviceBusy) => Ok(BiometricVerification::DeviceBusy),
            Ok(UserConsentVerificationResult::DeviceNotPresent) => {
                Err(CryptoError::BiometricUnavailable)
            }
            Ok(UserConsentVerificationResult::DisabledByPolicy) => {
                Err(CryptoError::BiometricUnavailable)
            }
            Ok(_) => Ok(BiometricVerification::Failed),
            Err(e) => Err(CryptoError::BiometricFailed(e.to_string())),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{BiometricAvailability, BiometricVerification};
    use crate::error::{CryptoError, Result};

    pub fn check_availability() -> BiometricAvailability {
        BiometricAvailability::Unavailable
    }

    pub fn verify_user(_message: &str) -> Result<BiometricVerification> {
        Err(CryptoError::BiometricUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_is_defined_on_all_platforms() {
        let _ = check_availability();
    }
}
