//! Simple, dependency-free search over a [`crate::PasswordDatabase`].

use secrecy::ExposeSecret;
use uuid::Uuid;

use crate::error::Result;
use crate::kdbx::database::PasswordDatabase;
use crate::kdbx::entry::PasswordEntry;

/// Filter + free-text query applied to the database.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// Free-text needle. Matched substring-wise against title, username,
    /// URL, notes, and tags.
    pub text: Option<String>,
    /// Require this tag to be present.
    pub tag: Option<String>,
    /// Restrict to entries directly in this group.
    pub group: Option<Uuid>,
    /// If `true`, text matching is case-sensitive.
    pub case_sensitive: bool,
    /// Optional cap on the number of results returned.
    pub limit: Option<usize>,
}

/// One hit returned by [`PasswordDatabase::search`].
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matched entry.
    pub entry: PasswordEntry,
    /// Relevance score (higher is better).
    pub score: i32,
}

impl PasswordDatabase {
    /// Run `query` against the database.
    ///
    /// # Errors
    ///
    /// Propagates `keepass` errors. In practice this is infallible for
    /// the current keepass crate.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let all = self.list_entries(None)?;
        let mut hits: Vec<SearchResult> = all
            .into_iter()
            .filter_map(|e| score_entry(&e, query).map(|score| SearchResult { entry: e, score }))
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.entry.modified_at.cmp(&a.entry.modified_at))
        });
        if let Some(limit) = query.limit {
            hits.truncate(limit);
        }
        Ok(hits)
    }
}

fn score_entry(entry: &PasswordEntry, query: &SearchQuery) -> Option<i32> {
    if let Some(group) = query.group {
        if entry.group_id != group {
            return None;
        }
    }
    if let Some(tag) = &query.tag {
        let tag_norm = if query.case_sensitive {
            tag.clone()
        } else {
            tag.to_ascii_lowercase()
        };
        let matched = entry.tags.iter().any(|t| {
            if query.case_sensitive {
                t == &tag_norm
            } else {
                t.to_ascii_lowercase() == tag_norm
            }
        });
        if !matched {
            return None;
        }
    }
    let mut score: i32 = 0;
    if let Some(needle_raw) = &query.text {
        let needle = if query.case_sensitive {
            needle_raw.clone()
        } else {
            needle_raw.to_ascii_lowercase()
        };
        let cmp = |hay: &str| -> bool {
            if query.case_sensitive {
                hay.contains(&needle)
            } else {
                hay.to_ascii_lowercase().contains(&needle)
            }
        };
        let mut any = false;
        if cmp(&entry.title) {
            score += 50;
            any = true;
        }
        if cmp(&entry.username) {
            score += 20;
            any = true;
        }
        if let Some(url) = &entry.url {
            if cmp(url) {
                score += 15;
                any = true;
            }
        }
        if let Some(notes) = &entry.notes {
            if cmp(notes) {
                score += 5;
                any = true;
            }
        }
        for t in &entry.tags {
            if cmp(t) {
                score += 25;
                any = true;
                break;
            }
        }
        // Also search custom fields (but not passwords).
        for (k, v) in &entry.custom_fields {
            if cmp(k) || cmp(v.expose_secret()) {
                score += 5;
                any = true;
                break;
            }
        }
        if !any {
            return None;
        }
    } else {
        // No text, but tag / group constraints satisfied -> accept.
        score = 1;
    }
    if query.tag.is_some() {
        score += 10;
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;
    use std::collections::BTreeMap;

    fn sample_db() -> (tempfile::TempDir, PasswordDatabase, Uuid) {
        let td = tempfile::tempdir().unwrap();
        let db =
            PasswordDatabase::create(&td.path().join("db.kdbx"), SecretString::from("pw"))
                .unwrap();
        let root = db.root_group().unwrap().id;
        for (title, tag) in [
            ("GitHub", "work"),
            ("gitlab", "work"),
            ("Personal Email", "personal"),
        ] {
            let e = PasswordEntry {
                id: Uuid::new_v4(),
                title: title.into(),
                username: "alice".into(),
                password: SecretString::from(String::new()),
                url: None,
                notes: None,
                tags: vec![tag.into()],
                custom_fields: BTreeMap::new(),
                totp: None,
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                group_id: root,
            };
            db.add_entry(e).unwrap();
        }
        (td, db, root)
    }

    #[test]
    fn case_insensitive_text_match() {
        let (_td, db, _root) = sample_db();
        let hits = db
            .search(&SearchQuery {
                text: Some("GITHUB".into()),
                ..SearchQuery::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.title, "GitHub");
    }

    #[test]
    fn tag_filter_works() {
        let (_td, db, _root) = sample_db();
        let hits = db
            .search(&SearchQuery {
                tag: Some("work".into()),
                ..SearchQuery::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn case_sensitive_strict() {
        let (_td, db, _root) = sample_db();
        let hits = db
            .search(&SearchQuery {
                text: Some("gitlab".into()),
                case_sensitive: true,
                ..SearchQuery::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.title, "gitlab");

        let no_hits = db
            .search(&SearchQuery {
                text: Some("Gitlab".into()),
                case_sensitive: true,
                ..SearchQuery::default()
            })
            .unwrap();
        assert!(no_hits.is_empty());
    }
}
