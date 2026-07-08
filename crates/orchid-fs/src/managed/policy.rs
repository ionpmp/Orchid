//! Retention, quota, and exclude rules for managed folders.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::error::{FsError, Result};
use crate::managed::config::ManagedFolderStats;

/// Optional policy constraints applied to a managed folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode, PartialEq, Eq)]
pub struct ManagedFolderPolicy {
    /// Maximum chunk-store bytes allowed for this folder (`None` = unlimited).
    pub max_size_bytes: Option<u64>,
    /// Drop ingested manifests older than this many days (`None` = keep forever).
    pub retention_days: Option<u32>,
    /// Glob patterns; matching paths are never ingested.
    pub exclude_patterns: Vec<String>,
}

impl ManagedFolderPolicy {
    /// Returns `false` when `path` matches any [`Self::exclude_patterns`].
    #[must_use]
    pub fn should_ingest(&self, path: &str) -> bool {
        if self.exclude_patterns.is_empty() {
            return true;
        }
        let norm = normalize_path_for_glob(path);
        let file_name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        for pattern in &self.exclude_patterns {
            if glob_matches(pattern, &norm) || glob_matches(pattern, &file_name) {
                return false;
            }
        }
        true
    }

    /// Returns [`FsError::ManagedQuotaExceeded`] when [`ManagedFolderStats::physical_bytes`]
    /// is above [`Self::max_size_bytes`].
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ManagedQuotaExceeded`] when the quota is exceeded.
    pub fn check_quota(&self, stats: &ManagedFolderStats) -> Result<()> {
        if let Some(limit) = self.max_size_bytes {
            if stats.physical_bytes > limit {
                return Err(FsError::ManagedQuotaExceeded {
                    current: stats.physical_bytes,
                    limit,
                });
            }
        }
        Ok(())
    }
}

fn normalize_path_for_glob(path: &str) -> String {
    path.replace('\\', "/")
}

/// Simple glob matcher supporting `*` (any run) and `?` (one character).
fn glob_matches(pattern: &str, text: &str) -> bool {
    glob_match_recursive(pattern.as_bytes(), text.as_bytes())
}

fn glob_match_recursive(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            glob_match_recursive(&pattern[1..], text)
                || (!text.is_empty() && glob_match_recursive(pattern, &text[1..]))
        }
        (Some(b'?'), Some(_)) => glob_match_recursive(&pattern[1..], &text[1..]),
        (Some(p), Some(t)) if p == t => glob_match_recursive(&pattern[1..], &text[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclude_patterns_skip_matching_paths() {
        let policy = ManagedFolderPolicy {
            exclude_patterns: vec!["*.tmp".into(), "*.log".into()],
            ..Default::default()
        };
        assert!(!policy.should_ingest("local:/data/cache/file.tmp"));
        assert!(!policy.should_ingest(r"C:\Projects\app.log"));
        assert!(policy.should_ingest("local:/data/readme.md"));
    }

    #[test]
    fn exclude_question_mark_pattern() {
        let policy = ManagedFolderPolicy {
            exclude_patterns: vec!["temp?".into()],
            ..Default::default()
        };
        assert!(!policy.should_ingest("local:/data/temp1"));
        assert!(policy.should_ingest("local:/data/temp12"));
    }

    #[test]
    fn quota_allows_under_limit() {
        let policy = ManagedFolderPolicy {
            max_size_bytes: Some(1024),
            ..Default::default()
        };
        let stats = ManagedFolderStats {
            physical_bytes: 512,
            ..Default::default()
        };
        assert!(policy.check_quota(&stats).is_ok());
    }

    #[test]
    fn quota_rejects_over_limit() {
        let policy = ManagedFolderPolicy {
            max_size_bytes: Some(1024),
            ..Default::default()
        };
        let stats = ManagedFolderStats {
            physical_bytes: 2048,
            ..Default::default()
        };
        let err = policy.check_quota(&stats).unwrap_err();
        assert!(matches!(
            err,
            FsError::ManagedQuotaExceeded {
                current: 2048,
                limit: 1024
            }
        ));
    }

    #[test]
    fn unlimited_quota_always_ok() {
        let policy = ManagedFolderPolicy::default();
        let stats = ManagedFolderStats {
            physical_bytes: u64::MAX,
            ..Default::default()
        };
        assert!(policy.check_quota(&stats).is_ok());
    }
}
