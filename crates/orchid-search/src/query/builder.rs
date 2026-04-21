//! Query types with a fluent builder.

/// Query parameters accepted by [`crate::SearchEngine::search`].
#[derive(Debug, Clone, Default)]
pub struct Query {
    /// Free-text needle searched against `name` + `content`.
    pub text: Option<String>,
    /// Require the entry's extension to be one of these.
    pub extensions: Vec<String>,
    /// Require the MIME to match.
    pub mimes: Vec<String>,
    /// Require every listed tag (lowercased exact).
    pub tags: Vec<String>,
    /// Require this colour label.
    pub color_label: Option<String>,
    /// Restrict results to paths starting with this prefix.
    pub path_prefix: Option<String>,
    /// Lower size bound, inclusive.
    pub min_size: Option<u64>,
    /// Upper size bound, inclusive.
    pub max_size: Option<u64>,
    /// Lower bound on `modified` (Unix seconds, inclusive).
    pub modified_after: Option<i64>,
    /// Upper bound on `modified` (Unix seconds, inclusive).
    pub modified_before: Option<i64>,
    /// Restrict to files only.
    pub only_files: bool,
    /// Restrict to directories only.
    pub only_directories: bool,
    /// Maximum number of results. Zero becomes 50.
    pub limit: usize,
    /// Results to skip before the first hit (pagination).
    pub offset: usize,
}

impl Query {
    /// Start a new empty query.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            limit: 50,
            ..Default::default()
        }
    }
}

/// Fluent builder over [`Query`].
#[derive(Debug, Clone, Default)]
pub struct QueryBuilder {
    inner: Query,
}

impl QueryBuilder {
    /// Begin a new builder pre-populated with `limit = 50`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Query::empty(),
        }
    }

    /// Set the free-text needle.
    #[must_use]
    pub fn text(mut self, s: impl Into<String>) -> Self {
        self.inner.text = Some(s.into());
        self
    }

    /// Require a specific extension.
    #[must_use]
    pub fn extension(mut self, ext: impl Into<String>) -> Self {
        self.inner.extensions.push(ext.into().to_lowercase());
        self
    }

    /// Require any of several extensions.
    #[must_use]
    pub fn extensions(mut self, exts: impl IntoIterator<Item = String>) -> Self {
        for e in exts {
            self.inner.extensions.push(e.to_lowercase());
        }
        self
    }

    /// Require a tag.
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.inner.tags.push(tag.into().to_lowercase());
        self
    }

    /// Require a path prefix.
    #[must_use]
    pub fn path_prefix(mut self, p: impl Into<String>) -> Self {
        self.inner.path_prefix = Some(p.into());
        self
    }

    /// Require a file-size range.
    #[must_use]
    pub fn size_range(mut self, min: u64, max: u64) -> Self {
        self.inner.min_size = Some(min);
        self.inner.max_size = Some(max);
        self
    }

    /// Lower bound on the modified timestamp.
    #[must_use]
    pub fn modified_after(mut self, ts: i64) -> Self {
        self.inner.modified_after = Some(ts);
        self
    }

    /// Upper bound on the modified timestamp.
    #[must_use]
    pub fn modified_before(mut self, ts: i64) -> Self {
        self.inner.modified_before = Some(ts);
        self
    }

    /// Cap the number of hits.
    #[must_use]
    pub fn limit(mut self, n: usize) -> Self {
        self.inner.limit = n.max(1);
        self
    }

    /// Offset for pagination.
    #[must_use]
    pub fn offset(mut self, n: usize) -> Self {
        self.inner.offset = n;
        self
    }

    /// Finalise.
    #[must_use]
    pub fn build(self) -> Query {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_populates_every_field() {
        let q = QueryBuilder::new()
            .text("invoice")
            .extension("PDF")
            .tag("Work")
            .path_prefix("local:c:/docs")
            .size_range(1024, 10_000_000)
            .modified_after(0)
            .modified_before(1_700_000_000)
            .limit(25)
            .offset(5)
            .build();
        assert_eq!(q.text.as_deref(), Some("invoice"));
        assert_eq!(q.extensions, vec!["pdf".to_string()]);
        assert_eq!(q.tags, vec!["work".to_string()]);
        assert_eq!(q.path_prefix.as_deref(), Some("local:c:/docs"));
        assert_eq!(q.min_size, Some(1024));
        assert_eq!(q.max_size, Some(10_000_000));
        assert_eq!(q.modified_after, Some(0));
        assert_eq!(q.modified_before, Some(1_700_000_000));
        assert_eq!(q.limit, 25);
        assert_eq!(q.offset, 5);
    }
}
