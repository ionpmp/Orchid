//! netrc extractor.
//!
//! Machine names are indexed. Logins, passwords, and accounts are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_netrc_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".netrc" || lower == "_netrc"
}

pub(crate) fn netrc_text(input: &str) -> String {
    let mut out = String::new();
    let mut in_macro = false;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if in_macro {
            let top_level = !raw.starts_with(|ch: char| ch.is_whitespace());
            if !(top_level && (line.starts_with("machine") || line.starts_with("default"))) {
                continue;
            }
            in_macro = false;
        }
        let mut tokens = line.split_whitespace();
        while let Some(token) = tokens.next() {
            match token {
                "machine" => {
                    if let Some(host) = tokens.next() {
                        if !is_keyword(host) {
                            push_line(&mut out, host);
                        }
                    }
                }
                "login" | "password" | "account" => {
                    let _ = tokens.next();
                }
                "macdef" => {
                    in_macro = true;
                    break;
                }
                _ => {}
            }
        }
    }
    out.trim().to_string()
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token,
        "login" | "password" | "account" | "macdef" | "default" | "machine"
    )
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
    fn indexes_machines_and_skips_passwords() {
        let text = netrc_text(
            "machine example.com\n\
             login alice\n\
             password SECRET\n\
             \n\
             machine ftp.example.org login bob password OTHER\n\
             \n\
             macdef init\n\
             \u{20}password LEAK\n\
             \n\
             default\n\
             login anonymous\n\
             password guest\n",
        );
        assert!(text.contains("example.com"), "{text}");
        assert!(text.contains("ftp.example.org"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("alice"), "{text}");
        assert!(!text.contains("bob"), "{text}");
        assert!(!text.contains("LEAK"), "{text}");
        assert!(!text.contains("guest"), "{text}");
        assert!(is_netrc_name(".netrc"));
        assert!(is_netrc_name("_netrc"));
        assert!(!is_netrc_name("netrc.txt"));
    }
}
