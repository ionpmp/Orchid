//! One shared outbound HTTP client for the whole process.
//!
//! `reqwest::Client` owns a connection pool and a TLS root store; building
//! one per request throws both away and pays a fresh TLS handshake. Widgets
//! (weather, RSS, clock, jyotish) and one-off UI actions all resolve the
//! same instance through [`shared_client`].

use std::sync::OnceLock;
use std::time::Duration;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Shared, pooled HTTP client.
///
/// Falls back to a default client if the configured builder fails, so
/// callers never have to handle construction errors.
#[must_use]
pub fn shared_client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(format!("Orchid/{}", env!("CARGO_PKG_VERSION")))
            // Bounded waits keep tests and the UI thread from depending on
            // hung outbound connections.
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_client_is_one_instance() {
        assert!(std::ptr::eq(shared_client(), shared_client()));
    }
}
