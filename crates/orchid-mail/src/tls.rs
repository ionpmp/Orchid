//! Shared rustls connector helpers.

use std::sync::Arc;

use rustls::ClientConfig;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use crate::error::{MailError, Result};

/// Build a rustls client config with webpki roots.
#[must_use]
pub fn client_config() -> Arc<ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

/// Connect TCP + implicit TLS to `host:port`.
pub async fn connect_tls(host: &str, port: u16) -> Result<TlsStream<TcpStream>> {
    let addr = format!("{host}:{port}");
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| MailError::Imap(format!("connect {addr}: {e}")))?;
    let connector = TlsConnector::from(client_config());
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| MailError::Imap(format!("invalid host {host}: {e}")))?;
    connector
        .connect(name, stream)
        .await
        .map_err(|e| MailError::Imap(format!("tls {host}: {e}")))
}
