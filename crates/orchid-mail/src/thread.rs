//! Group cached messages into conversation threads.
//!
//! A thread is messages in one folder whose subjects match after `Re:`,
//! `Fwd:`, and the same prefixes in a few other languages are removed.
//! The server THREAD command is not used. The newest message is the root.

use crate::account::MessageHeader;

/// Subject key used to group a thread.
#[must_use]
pub fn thread_key(subject: &str) -> String {
    let mut text = subject.trim().to_lowercase();
    loop {
        let mut next = None;
        for prefix in ["re:", "fwd:", "fw:", "aw:", "sv:", "vs:", "ref:"] {
            if let Some(rest) = text.strip_prefix(prefix) {
                let rest = rest.trim();
                if !rest.is_empty() && rest.len() < text.len() {
                    next = Some(rest.to_string());
                    break;
                }
            }
        }
        let Some(rest) = next else {
            break;
        };
        text = rest;
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One list row after threading. `indent` is 0 for the newest message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadRow {
    /// Index into the input slice.
    pub index: usize,
    /// 0 for the newest message in the thread, 1 for an older one.
    pub indent: u8,
}

/// Order messages so each thread stays together, newest thread first.
#[must_use]
pub fn arrange(headers: &[MessageHeader]) -> Vec<ThreadRow> {
    let mut order: Vec<usize> = (0..headers.len()).collect();
    order.sort_by(|&a, &b| {
        headers[b]
            .date_unix
            .cmp(&headers[a].date_unix)
            .then(headers[b].uid.cmp(&headers[a].uid))
    });
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for index in order {
        let key = thread_key(&headers[index].subject);
        if let Some(group) = groups
            .iter_mut()
            .find(|group| thread_key(&headers[group[0]].subject) == key)
        {
            group.push(index);
        } else {
            groups.push(vec![index]);
        }
    }
    groups.sort_by(|a, b| {
        headers[b[0]]
            .date_unix
            .cmp(&headers[a[0]].date_unix)
            .then(headers[b[0]].uid.cmp(&headers[a[0]].uid))
    });
    let mut rows = Vec::with_capacity(headers.len());
    for group in groups {
        for (place, index) in group.into_iter().enumerate() {
            rows.push(ThreadRow {
                index,
                indent: if place == 0 { 0 } else { 1 },
            });
        }
    }
    rows
}

/// Quote `text` for an IMAP search atom. Controls are rejected.
pub fn quote_imap(text: &str) -> Option<String> {
    if text.chars().any(|ch| ch.is_control()) {
        return None;
    }
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut out = String::from("\"");
    for ch in text.chars() {
        if ch == '\\' || ch == '"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn header(uid: u32, subject: &str, date_unix: i64) -> MessageHeader {
        MessageHeader {
            account_id: Uuid::nil(),
            folder: "INBOX".into(),
            uid,
            message_id: None,
            from: "Ada".into(),
            to: "me".into(),
            subject: subject.into(),
            date: String::new(),
            date_unix,
            seen: true,
            flagged: false,
            has_attachment: false,
            snippet: String::new(),
        }
    }

    #[test]
    fn strips_reply_prefixes_and_groups_newest_first() {
        assert_eq!(thread_key("  Re: Fwd:  Lunch "), "lunch");
        assert_eq!(thread_key("Aw: Re: Lunch"), "lunch");
        let headers = vec![
            header(1, "Lunch", 10),
            header(2, "Other", 30),
            header(3, "Re: Lunch", 20),
        ];
        let rows = arrange(&headers);
        assert_eq!(rows[0].index, 1);
        assert_eq!(rows[0].indent, 0);
        assert_eq!(rows[1].index, 2);
        assert_eq!(rows[1].indent, 0);
        assert_eq!(rows[2].index, 0);
        assert_eq!(rows[2].indent, 1);
    }

    #[test]
    fn quotes_a_search_string() {
        assert_eq!(quote_imap("lunch").as_deref(), Some("\"lunch\""));
        assert_eq!(quote_imap("a\"b").as_deref(), Some("\"a\\\"b\""));
        assert_eq!(quote_imap("a\\b").as_deref(), Some("\"a\\\\b\""));
        assert!(quote_imap("line\n").is_none());
        assert!(quote_imap("   ").is_none());
    }
}
