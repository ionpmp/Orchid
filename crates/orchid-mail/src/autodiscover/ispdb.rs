//! Mozilla ISPDB client (`autoconfig.thunderbird.net`).

use crate::account::{AuthKind, ServerEndpoint, ServerSuggestion, TlsMode};
use crate::error::{MailError, Result};

/// Fetch Thunderbird ISPDB XML for `domain`.
pub async fn fetch_ispdb(domain: &str, http: &reqwest::Client) -> Result<Option<ServerSuggestion>> {
    let url = format!("https://autoconfig.thunderbird.net/v1.1/{domain}");
    let response = http.get(&url).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(MailError::Autodiscover(format!(
            "ISPDB HTTP {}",
            response.status()
        )));
    }
    let xml = response.text().await?;
    Ok(parse_ispdb_xml(&xml))
}

/// Parse ISPDB XML into a [`ServerSuggestion`].
#[must_use]
pub fn parse_ispdb_xml(xml: &str) -> Option<ServerSuggestion> {
    // Lightweight tag scrape — ISPDB XML is small and regular.
    let imap_host = tag_text(xml, "incomingServer", "hostname")?;
    let imap_port: u16 = tag_text(xml, "incomingServer", "port")?.parse().ok()?;
    let imap_sock = tag_text(xml, "incomingServer", "socketType").unwrap_or_else(|| "SSL".into());
    let smtp_host = tag_text(xml, "outgoingServer", "hostname")?;
    let smtp_port: u16 = tag_text(xml, "outgoingServer", "port")?.parse().ok()?;
    let smtp_sock =
        tag_text(xml, "outgoingServer", "socketType").unwrap_or_else(|| "STARTTLS".into());

    Some(ServerSuggestion {
        imap: ServerEndpoint {
            host: imap_host,
            port: imap_port,
            tls: socket_type(&imap_sock),
            username: String::new(),
        },
        smtp: ServerEndpoint {
            host: smtp_host,
            port: smtp_port,
            tls: socket_type(&smtp_sock),
            username: String::new(),
        },
        auth: AuthKind::Password,
        oauth_provider: None,
        source: String::new(),
    })
}

fn socket_type(raw: &str) -> TlsMode {
    match raw.to_ascii_uppercase().as_str() {
        "SSL" => TlsMode::Implicit,
        "STARTTLS" => TlsMode::StartTls,
        _ => TlsMode::None,
    }
}

fn tag_text(xml: &str, section: &str, tag: &str) -> Option<String> {
    let sec_start = xml.find(&format!("<{section}"))?;
    let rest = &xml[sec_start..];
    let sec_end = rest.find(&format!("</{section}>"))?;
    let section_xml = &rest[..sec_end];
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let a = section_xml.find(&open)? + open.len();
    let b = section_xml[a..].find(&close)? + a;
    let value = section_xml[a..b].trim();
    if value.contains("%EMAIL") {
        Some(String::new())
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sample_ispdb() {
        let xml = r#"
        <clientConfig version="1.1">
          <emailProvider id="example.com">
            <incomingServer type="imap">
              <hostname>imap.example.com</hostname>
              <port>993</port>
              <socketType>SSL</socketType>
              <username>%EMAILADDRESS%</username>
              <authentication>password-cleartext</authentication>
            </incomingServer>
            <outgoingServer type="smtp">
              <hostname>smtp.example.com</hostname>
              <port>587</port>
              <socketType>STARTTLS</socketType>
              <username>%EMAILADDRESS%</username>
              <authentication>password-cleartext</authentication>
            </outgoingServer>
          </emailProvider>
        </clientConfig>
        "#;
        let s = parse_ispdb_xml(xml).expect("parse");
        assert_eq!(s.imap.host, "imap.example.com");
        assert_eq!(s.imap.port, 993);
        assert_eq!(s.imap.tls, TlsMode::Implicit);
        assert_eq!(s.smtp.port, 587);
        assert_eq!(s.smtp.tls, TlsMode::StartTls);
    }
}
