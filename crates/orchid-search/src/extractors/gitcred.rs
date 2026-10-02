//! Git credential-store extractor.
//!
//! Host names are indexed. Usernames and passwords are not. Dispatch lives
//! in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_git_credentials_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(".git-credentials")
}

pub(crate) fn git_credentials_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(host) = host_of(line) {
            push_line(&mut out, host);
        }
    }
    out.trim().to_string()
}

fn host_of(line: &str) -> Option<&str> {
    let rest = line.split_once("://").map_or(line, |(_, rest)| rest);
    let hostport = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
    let host = hostport.split(['/', '?', '#']).next().unwrap_or(hostport);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() || host.contains(' ') {
        None
    } else {
        Some(host)
    }
}

fn push_line(out: &mut String, value: &str) {
    let value = value.trim();
    if value.is_empty() || out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    out.push_str(&value.chars().take(room).collect::<String>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_hosts_and_skips_passwords() {
        let text = git_credentials_text(
            "https://alice:SECRET@github.com\n\
             https://TOKEN@gitlab.example.com:8443/group/repo.git\n\
             # https://hidden:PASSWORD@bitbucket.org\n",
        );
        assert!(text.contains("github.com"), "{text}");
        assert!(text.contains("gitlab.example.com"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("TOKEN"), "{text}");
        assert!(!text.contains("alice"), "{text}");
        assert!(!text.contains("PASSWORD"), "{text}");
        assert!(!text.contains("8443"), "{text}");
        assert!(is_git_credentials_name(".git-credentials"));
        assert!(!is_git_credentials_name(".gitconfig"));
    }
}
