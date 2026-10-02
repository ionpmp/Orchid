//! Fetch `policy.toml` at startup and leave the previous file when that fails.

use std::path::Path;
use std::time::Duration;

use orchid_storage::{
    append_audit, audit_path, install_policy_body, load_policy, policy_path, policy_url_allowed,
    InstallPolicy, PolicyLoad, POLICY_MAX_BYTES,
};
use tracing::warn;

/// Read the configured address into `policy.toml`, then record what is on disk.
///
/// An empty address skips the network. A network error, a redirect, or a body
/// that is not a policy document leaves the existing file in place.
pub async fn refresh_policy(config_file: &Path, url: &str) {
    let policy_file = policy_path(config_file);
    let audit_file = audit_path(config_file);
    let trimmed = url.trim();
    if !trimmed.is_empty() {
        match fetch_policy(trimmed).await {
            Fetched::Body(body) => match install_policy_body(&policy_file, &body) {
                Ok(InstallPolicy::Saved { locks }) => {
                    let _ =
                        append_audit(&audit_file, "policy-fetch", &format!("saved locks={locks}"));
                }
                Ok(InstallPolicy::Rejected) => {
                    let _ = append_audit(&audit_file, "policy-fetch", "kept reason=invalid");
                }
                Err(err) => {
                    warn!(error = %err, "policy file");
                    let _ = append_audit(&audit_file, "policy-fetch", "kept reason=write");
                }
            },
            Fetched::Kept(reason) => {
                let _ = append_audit(
                    &audit_file,
                    "policy-fetch",
                    &format!("kept reason={reason}"),
                );
            }
        }
    }
    match load_policy(&policy_file) {
        PolicyLoad::Absent => {}
        PolicyLoad::Document(doc) => {
            let _ = append_audit(
                &audit_file,
                "policy-apply",
                &format!("locks={}", doc.lock.count()),
            );
        }
        PolicyLoad::Invalid => {
            let _ = append_audit(&audit_file, "policy-apply", "kept reason=invalid");
        }
    }
}

enum Fetched {
    Body(String),
    Kept(&'static str),
}

async fn fetch_policy(url: &str) -> Fetched {
    if !policy_url_allowed(url) {
        return Fetched::Kept("not-https");
    }
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent(format!("Orchid/{}", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            warn!(error = %err, "policy client");
            return Fetched::Kept("client");
        }
    };
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(err) => {
            warn!(error = %err, "policy fetch");
            return Fetched::Kept("network");
        }
    };
    if response.status().is_redirection() {
        return Fetched::Kept("redirect");
    }
    if !response.status().is_success() {
        return Fetched::Kept("status");
    }
    if response
        .content_length()
        .is_some_and(|len| len > POLICY_MAX_BYTES as u64)
    {
        return Fetched::Kept("too-large");
    }
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(err) => {
            warn!(error = %err, "policy body");
            return Fetched::Kept("network");
        }
    };
    if bytes.len() > POLICY_MAX_BYTES {
        return Fetched::Kept("too-large");
    }
    match std::str::from_utf8(&bytes) {
        Ok(text) => Fetched::Body(text.to_string()),
        Err(_) => Fetched::Kept("invalid"),
    }
}
