//! DNS SRV lookup for IMAP / submission (RFC 6186).

use crate::account::{AuthKind, ServerEndpoint, ServerSuggestion, TlsMode};
use crate::error::{MailError, Result};

/// Resolve `_imaps._tcp` / `_submission._tcp` (with cleartext fallbacks).
pub async fn lookup_srv(domain: &str) -> Result<Option<ServerSuggestion>> {
    let resolver = hickory_resolver::TokioResolver::builder_tokio()
        .map_err(|e| MailError::Autodiscover(e.to_string()))?
        .build();

    let imap = first_srv(&resolver, &format!("_imaps._tcp.{domain}"))
        .await
        .map(|(host, port)| ServerEndpoint {
            host,
            port,
            tls: TlsMode::Implicit,
            username: String::new(),
        });
    let imap = match imap {
        Some(v) => v,
        None => {
            let clear = first_srv(&resolver, &format!("_imap._tcp.{domain}")).await;
            match clear {
                Some((host, port)) => ServerEndpoint {
                    host,
                    port,
                    tls: TlsMode::StartTls,
                    username: String::new(),
                },
                None => return Ok(None),
            }
        }
    };

    let smtp = first_srv(&resolver, &format!("_submission._tcp.{domain}"))
        .await
        .or(first_srv(&resolver, &format!("_submissions._tcp.{domain}")).await)
        .map(|(host, port)| {
            let tls = if port == 465 {
                TlsMode::Implicit
            } else {
                TlsMode::StartTls
            };
            ServerEndpoint {
                host,
                port,
                tls,
                username: String::new(),
            }
        })
        .unwrap_or(ServerEndpoint {
            host: format!("smtp.{domain}"),
            port: 587,
            tls: TlsMode::StartTls,
            username: String::new(),
        });

    Ok(Some(ServerSuggestion {
        imap,
        smtp,
        auth: AuthKind::Password,
        oauth_provider: None,
        source: String::new(),
    }))
}

async fn first_srv(
    resolver: &hickory_resolver::TokioResolver,
    name: &str,
) -> Option<(String, u16)> {
    let lookup = resolver.srv_lookup(name).await.ok()?;
    let mut records: Vec<_> = lookup.iter().collect();
    records.sort_by_key(|r| (r.priority(), r.weight()));
    let rec = records.first()?;
    let mut host = rec.target().to_ascii();
    if host.ends_with('.') {
        host.pop();
    }
    if host.is_empty() {
        return None;
    }
    Some((host, rec.port()))
}
