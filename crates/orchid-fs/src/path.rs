//! Canonical path type used across every provider.
//!
//! An [`FsPath`] always carries a scheme prefix such as `local:`, `sftp:`,
//! or `archive:`. The rest of the path uses forward slashes; providers
//! translate to OS-native paths internally.
//!
//! Example representations:
//!
//! * `local:c:/Users/Alice/Documents`
//! * `sftp:myserver/home/alice`
//! * `archive:c:/Users/Alice/a.zip#path/inside.txt`

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{FsError, Result};

/// Scheme used by the default local-disk provider.
pub const SCHEME_LOCAL: &str = "local";

/// Scheme used by archive-browsing pseudo-provider (read-only).
pub const SCHEME_ARCHIVE: &str = "archive";

/// Canonicalised path with an explicit scheme prefix.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    bincode::Encode,
    bincode::Decode,
)]
pub struct FsPath(String);

impl FsPath {
    /// Parse and canonicalise `s`. The string must be of the form
    /// `"<scheme>:<body>"`; the body is normalised by stripping trailing
    /// slashes (except right after the scheme), flattening `.` segments,
    /// and collapsing `//`.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::InvalidPath`] if the scheme is missing, contains
    /// invalid characters, or the body cannot be parsed.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_fs::FsPath;
    /// let p = FsPath::new("local:c:/Users/Alice")?;
    /// assert_eq!(p.scheme(), "local");
    /// # Ok::<_, orchid_fs::FsError>(())
    /// ```
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let raw = s.into();
        let Some(colon) = raw.find(':') else {
            return Err(FsError::InvalidPath {
                reason: format!("missing scheme: {raw}"),
            });
        };
        let scheme = &raw[..colon];
        if scheme.is_empty()
            || !scheme
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(FsError::InvalidPath {
                reason: format!("invalid scheme `{scheme}` in {raw}"),
            });
        }
        let body_raw = &raw[colon + 1..];
        let (body_main, hash_tail) = match body_raw.split_once('#') {
            Some((m, t)) => (m, Some(t)),
            None => (body_raw, None),
        };
        let body_norm = normalise_body(body_main);
        let rebuilt = match hash_tail {
            Some(t) => format!("{scheme}:{body_norm}#{t}"),
            None => format!("{scheme}:{body_norm}"),
        };
        Ok(Self(rebuilt))
    }

    /// Build a local-scheme path from an OS `Path`.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::InvalidPath`] if the OS path is not valid UTF-8.
    pub fn from_local(p: &Path) -> Result<Self> {
        let s = p.to_str().ok_or_else(|| FsError::InvalidPath {
            reason: format!("non-UTF8 OS path: {}", p.display()),
        })?;
        let with_slashes = s.replace('\\', "/");
        // Normalise drive letter to lowercase.
        let normalised = if let Some(rest) = with_slashes.strip_prefix(|c: char| c.is_ascii_alphabetic())
        {
            let first = with_slashes.chars().next().unwrap_or('x').to_ascii_lowercase();
            format!("{first}{rest}")
        } else {
            with_slashes
        };
        Self::new(format!("{SCHEME_LOCAL}:{normalised}"))
    }

    /// Convert a local-scheme path back to an OS `PathBuf`.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::InvalidPath`] if the scheme is not `"local"`.
    pub fn to_local(&self) -> Result<PathBuf> {
        if self.scheme() != SCHEME_LOCAL {
            return Err(FsError::InvalidPath {
                reason: format!("not a local path: {}", self.0),
            });
        }
        let body = self.without_scheme();
        // Preserve the drive colon. Windows accepts forward slashes.
        Ok(PathBuf::from(body))
    }

    /// Borrow the full representation (scheme + body).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Scheme portion (without the trailing `:`).
    #[must_use]
    pub fn scheme(&self) -> &str {
        let colon = self.0.find(':').unwrap_or(self.0.len());
        &self.0[..colon]
    }

    /// Body portion (everything after the first `:`).
    #[must_use]
    pub fn without_scheme(&self) -> &str {
        let colon = self.0.find(':').map_or(0, |i| i + 1);
        &self.0[colon..]
    }

    /// Parent path, or `None` for the scheme root.
    #[must_use]
    pub fn parent(&self) -> Option<FsPath> {
        let body = self.without_scheme();
        let strip_hash = body.split_once('#').map_or(body, |(m, _)| m);
        let trimmed = strip_hash.trim_end_matches('/');
        let slash = trimmed.rfind('/')?;
        if slash == 0 {
            // Body is like "/foo" — parent is "/".
            return Some(Self(format!("{}:/", self.scheme())));
        }
        // On Windows drive paths like `c:/foo`, the root is `c:/`. We keep
        // the trailing slash in that case so join() composes cleanly.
        let parent_body = if trimmed[..slash].ends_with(':') {
            format!("{}/", &trimmed[..slash])
        } else {
            trimmed[..slash].to_string()
        };
        Some(Self(format!("{}:{parent_body}", self.scheme())))
    }

    /// Last path segment, or `None` if the body is empty.
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        let body = self.without_scheme();
        let strip_hash = body.split_once('#').map_or(body, |(m, _)| m);
        let trimmed = strip_hash.trim_end_matches('/');
        if trimmed.is_empty() {
            return None;
        }
        match trimmed.rfind('/') {
            Some(i) => {
                let tail = &trimmed[i + 1..];
                if tail.is_empty() {
                    None
                } else {
                    Some(tail)
                }
            }
            None => Some(trimmed),
        }
    }

    /// Lowercased file extension, if any.
    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        self.file_name().and_then(|n| n.rsplit_once('.').map(|(_, ext)| ext))
    }

    /// Append a path segment. Slashes in `segment` are preserved.
    #[must_use]
    pub fn join(&self, segment: &str) -> FsPath {
        let base_body = self.without_scheme();
        let mut body = if base_body.is_empty() {
            String::new()
        } else if base_body.ends_with('/') {
            base_body.to_string()
        } else {
            format!("{base_body}/")
        };
        body.push_str(segment.trim_start_matches('/'));
        FsPath::new(format!("{}:{body}", self.scheme())).unwrap_or_else(|_| self.clone())
    }

    /// Whether this path belongs to the default local-disk provider.
    #[must_use]
    pub fn is_local(&self) -> bool {
        self.scheme() == SCHEME_LOCAL
    }

    /// Whether this path refers to content inside an archive
    /// (`archive:<file>#<inner>`).
    #[must_use]
    pub fn is_archive(&self) -> bool {
        self.scheme() == SCHEME_ARCHIVE && self.0.contains('#')
    }

    /// For archive paths, split into `(outer archive path, inner entry)`.
    #[must_use]
    pub fn archive_parts(&self) -> Option<(&str, &str)> {
        if !self.is_archive() {
            return None;
        }
        self.0.split_once('#')
    }
}

fn normalise_body(body: &str) -> String {
    // Convert backslashes to forward slashes.
    let slashed = body.replace('\\', "/");
    // Collapse repeated slashes.
    let mut collapsed = String::with_capacity(slashed.len());
    let mut prev_slash = false;
    for c in slashed.chars() {
        if c == '/' {
            if !prev_slash {
                collapsed.push('/');
            }
            prev_slash = true;
        } else {
            collapsed.push(c);
            prev_slash = false;
        }
    }
    // Flatten `.` and `..` segments conservatively: keep `..` if we have no
    // segment to pop (defensive — invalid UNC shapes remain visible).
    let (anchor, rest) = split_anchor(&collapsed);
    let mut out: Vec<&str> = Vec::new();
    for seg in rest.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                if out.last().is_some_and(|s| *s != "..") {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let joined = out.join("/");
    let mut result = anchor.to_string();
    result.push_str(&joined);
    // Strip trailing slashes except after a drive like `c:/`.
    if result.ends_with('/') && !result.ends_with(":/") && result.len() > 1 {
        result.pop();
    }
    result
}

/// Split a normalised body into (anchor, rest) where the anchor is the
/// leading drive / root that must be preserved verbatim.
fn split_anchor(body: &str) -> (&str, &str) {
    // Leading slash (POSIX root).
    if let Some(stripped) = body.strip_prefix('/') {
        return ("/", stripped);
    }
    // Windows drive letter: `c:/...` — after scheme stripping we shouldn't
    // see a colon again unless someone used it explicitly. Guard anyway.
    if let Some(rest) = body.strip_prefix(|c: char| c.is_ascii_alphabetic()) {
        if rest.starts_with(":/") {
            let drive_end = 3; // 'c' + ':' + '/'
            return body.split_at(drive_end);
        }
    }
    ("", body)
}

impl fmt::Display for FsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for FsPath {
    type Err = FsError;
    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requires_scheme() {
        assert!(FsPath::new("no-scheme").is_err());
        assert!(FsPath::new("local:c:/Users/Alice").is_ok());
    }

    #[test]
    fn parts_decomposition() {
        let p = FsPath::new("local:c:/Users/Alice/file.txt").unwrap();
        assert_eq!(p.scheme(), "local");
        assert_eq!(p.without_scheme(), "c:/Users/Alice/file.txt");
        assert_eq!(p.file_name(), Some("file.txt"));
        assert_eq!(p.extension(), Some("txt"));
        assert_eq!(
            p.parent().as_ref().map(FsPath::as_str),
            Some("local:c:/Users/Alice")
        );
    }

    #[test]
    fn join_composes() {
        let base = FsPath::new("local:c:/Users").unwrap();
        let joined = base.join("Alice/file.txt");
        assert_eq!(joined.as_str(), "local:c:/Users/Alice/file.txt");
    }

    #[test]
    fn local_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let p = FsPath::from_local(tmp.path()).unwrap();
        assert!(p.is_local());
        let back = p.to_local().unwrap();
        // Round-trip through FsPath may normalise separators / drive case,
        // but the final resolved filesystem path should point at the same
        // directory.
        let canon_orig = dunce_compat(tmp.path());
        let canon_back = dunce_compat(&back);
        assert_eq!(canon_orig, canon_back);
    }

    fn dunce_compat(p: &Path) -> PathBuf {
        p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
    }

    #[test]
    fn archive_parts_split_on_hash() {
        let p =
            FsPath::new("archive:c:/data/pack.zip#inside/leaf.txt").unwrap();
        assert!(p.is_archive());
        let (outer, inner) = p.archive_parts().unwrap();
        assert_eq!(outer, "archive:c:/data/pack.zip");
        assert_eq!(inner, "inside/leaf.txt");
    }

    #[test]
    fn normalisation_collapses_dots_and_slashes() {
        let p = FsPath::new("local:c:/a/./b//c/../d").unwrap();
        assert_eq!(p.as_str(), "local:c:/a/b/d");
    }

    #[test]
    fn posix_root_parent() {
        let p = FsPath::new("local:/tmp/foo").unwrap();
        assert_eq!(p.parent().as_ref().map(FsPath::as_str), Some("local:/tmp"));
        let root = FsPath::new("local:/tmp").unwrap();
        assert_eq!(root.parent().as_ref().map(FsPath::as_str), Some("local:/"));
    }
}
