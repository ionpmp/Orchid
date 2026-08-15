//! Resolve the device's current geographic position.
//!
//! Primary path: Windows Location API (`Geolocator`). On denial, timeout, or
//! non-Windows platforms, falls back to a key-free HTTPS IP lookup.

use std::future::Future;
use std::time::Duration;

use tracing::debug;

/// Where a [`ResolvedLocation`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationSource {
    /// OS / GNSS location services.
    Gps,
    /// Approximate location from the public IP.
    Ip,
}

/// Successfully resolved observation coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLocation {
    /// Human label (city name when known; may be empty for bare GPS).
    pub label: String,
    /// WGS84 latitude.
    pub latitude: f64,
    /// WGS84 longitude.
    pub longitude: f64,
    /// Resolution path used.
    pub source: LocationSource,
    /// IANA timezone id when the source provides one (e.g. IP lookup).
    pub timezone: Option<String>,
}

/// Errors from GPS or IP geolocation.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum GeolocateError {
    #[error("location access denied")]
    Denied,
    #[error("location unavailable: {0}")]
    Unavailable(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("location response parse error: {0}")]
    Parse(String),
}

/// Resolve current location: try `try_gps`, then IP fallback on any failure.
pub async fn resolve_with_gps_fallback<F, Fut>(
    try_gps: F,
    client: &reqwest::Client,
) -> Result<ResolvedLocation, GeolocateError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<ResolvedLocation, GeolocateError>>,
{
    match try_gps().await {
        Ok(loc) => Ok(loc),
        Err(e) => {
            debug!(error = %e, "primary geolocation failed; trying IP fallback");
            resolve_ip_location(client).await
        }
    }
}

/// Resolve via Windows GPS (when available) then IP.
pub async fn resolve_current_location(
    client: &reqwest::Client,
) -> Result<ResolvedLocation, GeolocateError> {
    resolve_with_gps_fallback(resolve_windows_gps, client).await
}

/// Approximate location from the client's public IP (`ipwho.is`, no API key).
pub async fn resolve_ip_location(
    client: &reqwest::Client,
) -> Result<ResolvedLocation, GeolocateError> {
    let resp = client
        .get("https://ipwho.is/")
        .timeout(Duration::from_secs(8))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(GeolocateError::Unavailable(format!(
            "IP lookup HTTP {}",
            resp.status()
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| GeolocateError::Parse(e.to_string()))?;
    parse_ipwho_response(&body)
}

/// Parse an `ipwho.is` JSON body into a [`ResolvedLocation`].
pub fn parse_ipwho_response(body: &str) -> Result<ResolvedLocation, GeolocateError> {
    #[derive(serde::Deserialize)]
    struct IpWhoTz {
        id: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct IpWho {
        success: Option<bool>,
        message: Option<String>,
        city: Option<String>,
        country: Option<String>,
        latitude: Option<f64>,
        longitude: Option<f64>,
        timezone: Option<IpWhoTz>,
    }
    let parsed: IpWho =
        serde_json::from_str(body).map_err(|e| GeolocateError::Parse(e.to_string()))?;
    if parsed.success == Some(false) {
        return Err(GeolocateError::Unavailable(
            parsed.message.unwrap_or_else(|| "IP lookup failed".into()),
        ));
    }
    let latitude = parsed
        .latitude
        .ok_or_else(|| GeolocateError::Parse("missing latitude".into()))?;
    let longitude = parsed
        .longitude
        .ok_or_else(|| GeolocateError::Parse("missing longitude".into()))?;
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return Err(GeolocateError::Parse("coordinates out of range".into()));
    }
    let label = match (parsed.city, parsed.country) {
        (Some(city), Some(country)) if !city.is_empty() && !country.is_empty() => {
            format!("{city}, {country}")
        }
        (Some(city), _) if !city.is_empty() => city,
        (_, Some(country)) if !country.is_empty() => country,
        _ => String::new(),
    };
    let timezone = parsed
        .timezone
        .and_then(|tz| tz.id)
        .filter(|id| !id.is_empty());
    Ok(ResolvedLocation {
        label,
        latitude,
        longitude,
        source: LocationSource::Ip,
        timezone,
    })
}

#[cfg(windows)]
async fn resolve_windows_gps() -> Result<ResolvedLocation, GeolocateError> {
    tokio::task::spawn_blocking(windows_gps_blocking)
        .await
        .map_err(|e| GeolocateError::Unavailable(e.to_string()))?
}

#[cfg(windows)]
fn windows_gps_blocking() -> Result<ResolvedLocation, GeolocateError> {
    use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator};
    use windows::Foundation::TimeSpan;
    let access = Geolocator::RequestAccessAsync()
        .map_err(|e| GeolocateError::Unavailable(e.to_string()))?
        .join()
        .map_err(|e| GeolocateError::Unavailable(e.to_string()))?;
    if access != GeolocationAccessStatus::Allowed {
        return Err(GeolocateError::Denied);
    }

    let locator = Geolocator::new().map_err(|e| GeolocateError::Unavailable(e.to_string()))?;
    // 60s max age, 8s timeout (100 ns ticks).
    let max_age = TimeSpan {
        Duration: 60 * 10_000_000,
    };
    let timeout = TimeSpan {
        Duration: 8 * 10_000_000,
    };
    let position = locator
        .GetGeopositionAsyncWithAgeAndTimeout(max_age, timeout)
        .map_err(|e| GeolocateError::Unavailable(e.to_string()))?
        .join()
        .map_err(|e| GeolocateError::Unavailable(e.to_string()))?;
    let coord = position
        .Coordinate()
        .map_err(|e| GeolocateError::Unavailable(e.to_string()))?;
    let point = coord
        .Point()
        .map_err(|e| GeolocateError::Unavailable(e.to_string()))?;
    let pos = point
        .Position()
        .map_err(|e| GeolocateError::Unavailable(e.to_string()))?;
    let latitude = pos.Latitude;
    let longitude = pos.Longitude;
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return Err(GeolocateError::Unavailable(
            "GPS coordinates out of range".into(),
        ));
    }
    Ok(ResolvedLocation {
        label: String::new(),
        latitude,
        longitude,
        source: LocationSource::Gps,
        timezone: None,
    })
}

#[cfg(not(windows))]
async fn resolve_windows_gps() -> Result<ResolvedLocation, GeolocateError> {
    Err(GeolocateError::Unavailable(
        "Windows Location API unavailable on this platform".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ipwho_success() {
        let body = r#"{
            "success": true,
            "city": "Varanasi",
            "country": "India",
            "latitude": 25.3176,
            "longitude": 82.9739,
            "timezone": { "id": "Asia/Kolkata" }
        }"#;
        let loc = parse_ipwho_response(body).expect("parse");
        assert_eq!(loc.label, "Varanasi, India");
        assert!((loc.latitude - 25.3176).abs() < 1e-6);
        assert!((loc.longitude - 82.9739).abs() < 1e-6);
        assert_eq!(loc.source, LocationSource::Ip);
        assert_eq!(loc.timezone.as_deref(), Some("Asia/Kolkata"));
    }

    #[test]
    fn parse_ipwho_failure_message() {
        let body = r#"{"success": false, "message": "reserved range"}"#;
        let err = parse_ipwho_response(body).expect_err("should fail");
        assert!(matches!(err, GeolocateError::Unavailable(_)));
    }

    #[tokio::test]
    async fn gps_fallback_uses_ip_when_primary_fails() {
        let client = reqwest::Client::new();
        // Force GPS failure; IP path needs network — skip if offline.
        let result =
            resolve_with_gps_fallback(|| async { Err(GeolocateError::Denied) }, &client).await;
        match result {
            Ok(loc) => {
                assert_eq!(loc.source, LocationSource::Ip);
                assert!((-90.0..=90.0).contains(&loc.latitude));
            }
            Err(e) => {
                // Offline CI / blocked egress — still exercised the fallback call.
                assert!(matches!(
                    e,
                    GeolocateError::Http(_)
                        | GeolocateError::Unavailable(_)
                        | GeolocateError::Parse(_)
                ));
            }
        }
    }
}
