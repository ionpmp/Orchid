//! AWS shared-credentials extractor.
//!
//! Profile names and regions are indexed. Access keys and secrets are not.
//! Only `.aws/credentials` is claimed. Dispatch lives in
//! [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_aws_credentials_name(name: &str, parent: Option<&str>) -> bool {
    name.eq_ignore_ascii_case("credentials")
        && parent.is_some_and(|dir| dir.eq_ignore_ascii_case(".aws"))
}

pub(crate) fn aws_credentials_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = section_name(line) {
            push_line(&mut out, name);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if is_secret_key(key) {
            continue;
        }
        if key.trim().eq_ignore_ascii_case("region") {
            push_line(&mut out, value.trim().trim_matches('"'));
        }
    }
    out.trim().to_string()
}

fn section_name(line: &str) -> Option<&str> {
    let name = line.strip_prefix('[')?.strip_suffix(']')?.trim();
    let name = name.strip_prefix("profile ").unwrap_or(name).trim();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("secret")
        || key.contains("token")
        || key.contains("password")
        || key.contains("access_key")
        || key.contains("key_id")
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
    fn indexes_profiles_and_skips_keys() {
        let text = aws_credentials_text(
            "[default]\n\
             aws_access_key_id = AKIAIOSFODNN7EXAMPLE\n\
             aws_secret_access_key = SECRET\n\
             region = us-east-1\n\
             \n\
             [orchid]\n\
             aws_secret_access_key = OTHER\n\
             aws_session_token = TOKEN\n\
             region = eu-west-1\n",
        );
        assert!(text.contains("default"), "{text}");
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("us-east-1"), "{text}");
        assert!(text.contains("eu-west-1"), "{text}");
        assert!(!text.contains("AKIA"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("TOKEN"), "{text}");
        assert!(is_aws_credentials_name("credentials", Some(".aws")));
        assert!(!is_aws_credentials_name("credentials", Some("secrets")));
        assert!(!is_aws_credentials_name("config", Some(".aws")));
    }
}
