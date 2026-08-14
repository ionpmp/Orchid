//! Find-files criteria, matching, and result sessions.

use std::collections::HashMap;
use std::sync::Arc;

use regex::RegexBuilder;
use uuid::Uuid;

use super::selection::{name_glob_match, parse_byte_size};
use super::virtual_folders::is_virtual;
use super::ActionOutcome;
use super::FileManagerInner;
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

const WALK_CAP: usize = 20_000;
const WALK_DEPTH: u32 = 16;
const CONTENT_BYTE_LIMIT: usize = 2 * 1024 * 1024;
const INDEXED_LIMIT: usize = 500;

/// Criteria for a Find Files run.
#[derive(Debug, Clone, Default)]
pub struct FindSpec {
    /// Folder to search from.
    pub root: String,
    /// Name substring, glob (`*.txt`), or regex.
    pub name: String,
    /// Treat [`Self::name`] as a regular expression.
    pub name_regex: bool,
    /// Case-sensitive name / content match.
    pub case_sensitive: bool,
    /// Recurse into subfolders.
    pub recursive: bool,
    /// Inclusive lower size bound for files.
    pub min_size: Option<u64>,
    /// Inclusive upper size bound for files.
    pub max_size: Option<u64>,
    /// Modified within this many days.
    pub newer_than_days: Option<u32>,
    /// Include files.
    pub files: bool,
    /// Include folders.
    pub folders: bool,
    /// Require the hidden attribute.
    pub hidden: bool,
    /// Require the read-only attribute.
    pub readonly: bool,
    /// Require the system attribute.
    pub system: bool,
    /// Grep needle (literal or regex).
    pub content: String,
    /// Treat [`Self::content`] as a regular expression.
    pub content_regex: bool,
    /// Also scan archive members.
    pub in_archives: bool,
    /// Prefer Windows Search / Tantivy.
    pub indexed: bool,
    /// Keep the result folder in `virtual:search`.
    pub save_as_virtual: bool,
}

impl FindSpec {
    /// Default find in `root` (current folder, files + folders).
    #[must_use]
    pub fn in_root(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            files: true,
            folders: true,
            recursive: false,
            ..Self::default()
        }
    }

    /// Pack into the newline format used by the find dialog.
    #[must_use]
    pub fn pack(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.name,
            self.content,
            flag(self.name_regex),
            flag(self.content_regex),
            flag(self.case_sensitive),
            flag(self.recursive),
            flag(self.in_archives),
            flag(self.indexed),
            flag(self.save_as_virtual),
            fmt_opt_size(self.min_size),
            fmt_opt_size(self.max_size),
            self.newer_than_days.unwrap_or(0),
            flag(self.files),
            flag(self.folders),
            flag(self.hidden),
            flag(self.readonly),
            flag(self.system),
            self.root,
        )
    }

    /// Parse the dialog payload. `fallback_root` is used when the pack omits a root.
    #[must_use]
    pub fn unpack(raw: &str, fallback_root: &str) -> Self {
        let mut lines = raw.split('\n');
        let next = |it: &mut std::str::Split<'_, char>| it.next().unwrap_or("").to_string();
        let name = next(&mut lines);
        let content = next(&mut lines);
        let name_regex = next(&mut lines) == "1";
        let content_regex = next(&mut lines) == "1";
        let case_sensitive = next(&mut lines) == "1";
        let recursive = next(&mut lines) == "1";
        let in_archives = next(&mut lines) == "1";
        let indexed = next(&mut lines) == "1";
        let save_as_virtual = next(&mut lines) == "1";
        let min_size = parse_byte_size(&next(&mut lines));
        let max_size = parse_byte_size(&next(&mut lines));
        let newer_than_days = next(&mut lines).parse::<u32>().ok().filter(|d| *d > 0);
        let files = next(&mut lines) != "0";
        let folders = next(&mut lines) != "0";
        let hidden = next(&mut lines) == "1";
        let readonly = next(&mut lines) == "1";
        let system = next(&mut lines) == "1";
        let root = {
            let r = next(&mut lines);
            if r.is_empty() {
                fallback_root.to_string()
            } else {
                r
            }
        };
        Self {
            root,
            name,
            name_regex,
            case_sensitive,
            recursive,
            min_size,
            max_size,
            newer_than_days,
            files,
            folders,
            hidden,
            readonly,
            system,
            content,
            content_regex,
            in_archives,
            indexed,
            save_as_virtual,
        }
    }

    /// Whether `entry` matches name / size / date / attribute predicates.
    #[must_use]
    pub fn matches_meta(&self, entry: &orchid_fs::FsEntry) -> bool {
        let is_dir = matches!(entry.metadata.kind, orchid_fs::FsEntryKind::Directory);
        if is_dir {
            if !self.folders {
                return false;
            }
        } else if !self.files {
            return false;
        }
        if !self.matches_name(&entry.name) {
            return false;
        }
        if !is_dir {
            if let Some(min) = self.min_size {
                if entry.metadata.size < min {
                    return false;
                }
            }
            if let Some(max) = self.max_size {
                if entry.metadata.size > max {
                    return false;
                }
            }
        } else if self.min_size.is_some() || self.max_size.is_some() {
            return false;
        }
        if self.hidden && !entry.metadata.hidden {
            return false;
        }
        if self.readonly && !entry.metadata.readonly {
            return false;
        }
        if self.system && !entry.metadata.system {
            return false;
        }
        if let Some(days) = self.newer_than_days {
            let Some(modified) = entry.metadata.modified else {
                return false;
            };
            let age = chrono::Utc::now().signed_duration_since(modified);
            if age.num_days() > i64::from(days) {
                return false;
            }
        }
        true
    }

    /// Name predicate (empty name matches everything).
    #[must_use]
    pub fn matches_name(&self, name: &str) -> bool {
        let pat = self.name.trim();
        if pat.is_empty() {
            return true;
        }
        if self.name_regex {
            return compile_re(pat, self.case_sensitive)
                .ok()
                .is_some_and(|re| re.is_match(name));
        }
        if pat.contains('*') || pat.contains('?') {
            if self.case_sensitive {
                return glob_cs(pat.as_bytes(), name.as_bytes());
            }
            return name_glob_match(pat, name);
        }
        if self.case_sensitive {
            name.contains(pat)
        } else {
            name.to_lowercase().contains(&pat.to_lowercase())
        }
    }

    /// Content predicate. Empty content matches everything.
    #[must_use]
    pub fn matches_content(&self, text: &str) -> bool {
        let q = self.content.trim();
        if q.is_empty() {
            return true;
        }
        if self.content_regex {
            return compile_re(q, self.case_sensitive)
                .ok()
                .is_some_and(|re| re.is_match(text));
        }
        if self.case_sensitive {
            text.contains(q)
        } else {
            text.to_lowercase().contains(&q.to_lowercase())
        }
    }
}

fn flag(v: bool) -> &'static str {
    if v {
        "1"
    } else {
        "0"
    }
}

fn fmt_opt_size(v: Option<u64>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

fn compile_re(pat: &str, case_sensitive: bool) -> Result<regex::Regex, regex::Error> {
    RegexBuilder::new(pat)
        .case_insensitive(!case_sensitive)
        .dot_matches_new_line(true)
        .build()
}

fn glob_cs(p: &[u8], t: &[u8]) -> bool {
    match (p.first(), t.first()) {
        (None, None) => true,
        (Some(b'*'), _) => (0..=t.len()).any(|i| glob_cs(&p[1..], &t[i..])),
        (Some(b'?'), Some(_)) => glob_cs(&p[1..], &t[1..]),
        (Some(a), Some(b)) if a == b => glob_cs(&p[1..], &t[1..]),
        _ => false,
    }
}

/// One find / duplicate / large-file result set.
#[derive(Debug, Clone)]
pub struct SearchSession {
    /// Session id (also the `virtual:search/<id>` suffix).
    pub id: Uuid,
    /// Sidebar / tab label.
    pub label: String,
    /// Criteria that produced the hits (empty for tool-built sets).
    pub spec: FindSpec,
    /// Matching entries.
    pub entries: Vec<orchid_fs::FsEntry>,
    /// When true, listed under `virtual:search`.
    pub saved: bool,
}

impl SearchSession {
    /// Virtual folder path for this session.
    #[must_use]
    pub fn virtual_path(&self) -> String {
        format!("virtual:search/{}", self.id)
    }
}

/// `true` when `raw` is a find-results virtual folder.
#[must_use]
pub fn is_search_virtual(raw: &str) -> bool {
    raw == "virtual:search" || raw.starts_with("virtual:search/")
}

/// Parse `virtual:search/<uuid>`.
#[must_use]
pub fn search_session_id(raw: &str) -> Option<Uuid> {
    raw.strip_prefix("virtual:search/")
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// Run find and store a session. Returns the virtual path to navigate to.
pub(super) async fn run_find(
    inner: &Arc<FileManagerInner>,
    spec: FindSpec,
) -> WidgetResult<ActionOutcome> {
    let entries = collect_hits(inner, &spec).await?;
    let label = find_label(&spec, entries.len());
    let id = Uuid::new_v4();
    let path = format!("virtual:search/{id}");
    inner.search_sessions.write().insert(
        id,
        SearchSession {
            id,
            label,
            spec: spec.clone(),
            entries,
            saved: spec.save_as_virtual,
        },
    );
    Ok(ActionOutcome::NavigateSearch { path })
}

/// Group same-size files by BLAKE3 and keep only hash collisions.
pub(super) async fn run_find_duplicates(
    inner: &Arc<FileManagerInner>,
    root: &orchid_fs::FsPath,
) -> WidgetResult<ActionOutcome> {
    let spec = FindSpec {
        root: root.as_str().to_string(),
        recursive: true,
        files: true,
        folders: false,
        ..FindSpec::default()
    };
    let candidates = walk_entries(inner, &spec).await?;
    let mut by_size: HashMap<u64, Vec<orchid_fs::FsEntry>> = HashMap::new();
    for e in candidates {
        if e.metadata.size == 0 {
            continue;
        }
        by_size.entry(e.metadata.size).or_default().push(e);
    }
    let mut to_hash = Vec::new();
    for group in by_size.into_values() {
        if group.len() > 1 {
            to_hash.extend(group);
        }
    }
    let paths: Vec<orchid_fs::FsPath> = to_hash.iter().map(|e| e.path.clone()).collect();
    let records = orchid_fs::hash_paths(&inner.deps.registry, &paths, orchid_fs::HashAlgo::Blake3)
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    let mut by_hash: HashMap<String, Vec<orchid_fs::FsPath>> = HashMap::new();
    for rec in records {
        by_hash.entry(rec.hex).or_default().push(rec.path);
    }
    let dup_paths: std::collections::HashSet<String> = by_hash
        .into_values()
        .filter(|g| g.len() > 1)
        .flatten()
        .map(|p| p.as_str().to_string())
        .collect();
    let entries: Vec<orchid_fs::FsEntry> = to_hash
        .into_iter()
        .filter(|e| dup_paths.contains(e.path.as_str()))
        .collect();
    let id = Uuid::new_v4();
    let n = entries.len();
    inner.search_sessions.write().insert(
        id,
        SearchSession {
            id,
            label: format!("duplicates ({n})"),
            spec,
            entries,
            saved: true,
        },
    );
    Ok(ActionOutcome::NavigateSearch {
        path: format!("virtual:search/{id}"),
    })
}

/// Recursive files at or above `min_size`.
pub(super) async fn run_find_large(
    inner: &Arc<FileManagerInner>,
    root: &orchid_fs::FsPath,
    min_size: u64,
) -> WidgetResult<ActionOutcome> {
    let spec = FindSpec {
        root: root.as_str().to_string(),
        recursive: true,
        files: true,
        folders: false,
        min_size: Some(min_size),
        ..FindSpec::default()
    };
    let mut entries = walk_entries(inner, &spec).await?;
    entries.sort_by_key(|b| std::cmp::Reverse(b.metadata.size));
    let id = Uuid::new_v4();
    let n = entries.len();
    inner.search_sessions.write().insert(
        id,
        SearchSession {
            id,
            label: format!("large files ({n})"),
            spec,
            entries,
            saved: true,
        },
    );
    Ok(ActionOutcome::NavigateSearch {
        path: format!("virtual:search/{id}"),
    })
}

fn find_label(spec: &FindSpec, n: usize) -> String {
    let needle = if !spec.name.trim().is_empty() {
        spec.name.trim()
    } else if !spec.content.trim().is_empty() {
        spec.content.trim()
    } else {
        "*"
    };
    format!("{needle} ({n})")
}

async fn collect_hits(
    inner: &Arc<FileManagerInner>,
    spec: &FindSpec,
) -> WidgetResult<Vec<orchid_fs::FsEntry>> {
    if spec.indexed {
        if let Ok(hits) = indexed_hits(inner, spec).await {
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
    }
    let raw = walk_all(inner, spec).await?;
    let mut entries: Vec<orchid_fs::FsEntry> = raw
        .iter()
        .filter(|e| spec.matches_meta(e))
        .cloned()
        .collect();
    if spec.in_archives {
        entries.extend(scan_archives(&raw, spec).await);
    }
    if spec.content.trim().is_empty() {
        return Ok(entries);
    }
    let mut kept = Vec::new();
    for e in entries {
        if matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory) {
            continue;
        }
        if content_matches(inner, &e, spec).await {
            kept.push(e);
        }
    }
    Ok(kept)
}

async fn walk_entries(
    inner: &Arc<FileManagerInner>,
    spec: &FindSpec,
) -> WidgetResult<Vec<orchid_fs::FsEntry>> {
    let raw = walk_all(inner, spec).await?;
    Ok(raw.into_iter().filter(|e| spec.matches_meta(e)).collect())
}

async fn walk_all(
    inner: &Arc<FileManagerInner>,
    spec: &FindSpec,
) -> WidgetResult<Vec<orchid_fs::FsEntry>> {
    let root = orchid_fs::FsPath::new(&spec.root)
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    if is_virtual(&root) {
        return Ok(inner.list_virtual(&root).await);
    }
    let show_hidden = inner.config.read().show_hidden || spec.hidden;
    let mut out = Vec::new();
    let mut stack = vec![(root, 0u32)];
    while let Some((dir, depth)) = stack.pop() {
        if out.len() >= WALK_CAP {
            break;
        }
        let listed = inner.navigator.navigate(&dir, show_hidden).await.entries;
        for e in listed {
            let is_dir = matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory);
            if spec.recursive && is_dir && depth < WALK_DEPTH {
                stack.push((e.path.clone(), depth + 1));
            }
            out.push(e);
            if out.len() >= WALK_CAP {
                break;
            }
        }
        if !spec.recursive {
            break;
        }
    }
    Ok(out)
}

async fn scan_archives(hosts: &[orchid_fs::FsEntry], spec: &FindSpec) -> Vec<orchid_fs::FsEntry> {
    let mut extra = Vec::new();
    for host in hosts {
        if matches!(host.metadata.kind, orchid_fs::FsEntryKind::Directory) {
            continue;
        }
        if !orchid_fs::looks_like_archive_name(&host.name) {
            continue;
        }
        let Ok(os) = host.path.to_local() else {
            continue;
        };
        let Ok(reader) = orchid_fs::open_archive(&os) else {
            continue;
        };
        let Ok(members) = reader.list().await else {
            continue;
        };
        let outer = os.to_string_lossy().replace('\\', "/");
        for m in members {
            let name = m
                .path
                .rsplit('/')
                .next()
                .unwrap_or(m.path.as_str())
                .to_string();
            let path = orchid_fs::FsPath::new(format!("archive:{outer}#{}", m.path))
                .unwrap_or_else(|_| host.path.clone());
            let fake = orchid_fs::FsEntry {
                path,
                name,
                metadata: orchid_fs::FsMetadata {
                    kind: if m.is_dir {
                        orchid_fs::FsEntryKind::Directory
                    } else {
                        orchid_fs::FsEntryKind::File
                    },
                    size: m.size,
                    created: None,
                    modified: m.modified,
                    accessed: None,
                    readonly: false,
                    hidden: false,
                    system: false,
                    mime: None,
                    extended: orchid_fs::ExtendedAttributes::default(),
                },
            };
            if !spec.matches_meta(&fake) {
                continue;
            }
            if !spec.content.trim().is_empty() && !m.is_dir {
                let Ok(bytes) = reader.read_entry(&m.path).await else {
                    continue;
                };
                let slice = if bytes.len() > CONTENT_BYTE_LIMIT {
                    &bytes[..CONTENT_BYTE_LIMIT]
                } else {
                    &bytes
                };
                if slice.contains(&0) {
                    continue;
                }
                if !spec.matches_content(&String::from_utf8_lossy(slice)) {
                    continue;
                }
            }
            extra.push(fake);
        }
        if extra.len() >= WALK_CAP {
            break;
        }
    }
    extra
}

async fn content_matches(
    inner: &Arc<FileManagerInner>,
    entry: &orchid_fs::FsEntry,
    spec: &FindSpec,
) -> bool {
    if let Some(bytes) = read_content_prefix(inner, entry).await {
        if bytes.contains(&0) {
            return false;
        }
        return spec.matches_content(&String::from_utf8_lossy(&bytes));
    }
    false
}

async fn read_content_prefix(
    inner: &Arc<FileManagerInner>,
    entry: &orchid_fs::FsEntry,
) -> Option<Vec<u8>> {
    if let Some(provider) = inner.deps.registry.for_path(&entry.path) {
        if let Ok(bytes) =
            orchid_fs::read_prefix(provider.as_ref(), &entry.path, CONTENT_BYTE_LIMIT).await
        {
            return Some(bytes);
        }
    }
    let (outer, inner_path) = entry.path.archive_parts()?;
    let outer_body = outer.strip_prefix("archive:").unwrap_or(outer);
    let os = std::path::PathBuf::from(outer_body.replace('/', std::path::MAIN_SEPARATOR_STR));
    let reader = orchid_fs::open_archive(&os).ok()?;
    let bytes = reader.read_entry(inner_path).await.ok()?;
    if bytes.len() > CONTENT_BYTE_LIMIT {
        Some(bytes[..CONTENT_BYTE_LIMIT].to_vec())
    } else {
        Some(bytes)
    }
}

async fn indexed_hits(
    inner: &Arc<FileManagerInner>,
    spec: &FindSpec,
) -> WidgetResult<Vec<orchid_fs::FsEntry>> {
    #[cfg(windows)]
    {
        if let Ok(hits) = windows_search_index(spec) {
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
    }
    let Some(engine) = inner.deps.search.as_ref() else {
        return Ok(Vec::new());
    };
    let mut builder = orchid_search::QueryBuilder::new().limit(INDEXED_LIMIT);
    let needle = if !spec.name.trim().is_empty() {
        spec.name.trim()
    } else {
        spec.content.trim()
    };
    if !needle.is_empty() && !spec.name_regex && !needle.contains('*') && !needle.contains('?') {
        builder = builder.text(needle);
    }
    if let Ok(root) = orchid_fs::FsPath::new(&spec.root) {
        if let Ok(os) = root.to_local() {
            builder = builder.path_prefix(os.to_string_lossy().into_owned());
        } else {
            builder = builder.path_prefix(root.as_str().to_string());
        }
    }
    let mut query = builder.build();
    query.min_size = spec.min_size;
    query.max_size = spec.max_size;
    query.only_files = spec.files && !spec.folders;
    let hits = engine
        .search(query)
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    let mut entries = Vec::new();
    for hit in hits.hits {
        let Ok(path) = orchid_fs::FsPath::new(&hit.path)
            .or_else(|_| orchid_fs::FsPath::from_local(std::path::Path::new(&hit.path)))
        else {
            continue;
        };
        let name = if hit.name.is_empty() {
            path.file_name().unwrap_or_default().to_string()
        } else {
            hit.name
        };
        let entry = orchid_fs::FsEntry {
            name,
            path,
            metadata: orchid_fs::FsMetadata {
                kind: orchid_fs::FsEntryKind::File,
                size: hit.size,
                created: None,
                modified: None,
                accessed: None,
                readonly: false,
                hidden: false,
                system: false,
                mime: None,
                extended: orchid_fs::ExtendedAttributes::default(),
            },
        };
        if spec.matches_meta(&entry) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[cfg(windows)]
fn windows_search_index(spec: &FindSpec) -> Result<Vec<orchid_fs::FsEntry>, ()> {
    let needle = spec.name.trim();
    if needle.is_empty()
        || spec.name_regex
        || needle.contains('*')
        || needle.contains('?')
        || needle.contains('\'')
        || needle.contains('"')
        || needle.contains('\n')
    {
        return Err(());
    }
    let root = orchid_fs::FsPath::new(&spec.root)
        .ok()
        .and_then(|p| p.to_local().ok())
        .ok_or(())?;
    let scope = root.to_string_lossy().replace('\'', "''");
    let sql = format!(
        "SELECT TOP {INDEXED_LIMIT} System.ItemPathDisplay FROM SYSTEMINDEX WHERE SCOPE='file:{scope}' AND CONTAINS(System.FileName, '\"{needle}\"')"
    );
    let mut cmd = std::process::Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &format!(
            "$c=New-Object -ComObject ADODB.Connection; $c.Open(\"Provider=Search.CollatorDSO;Extended Properties='Application=Windows';\"); $r=New-Object -ComObject ADODB.Recordset; $r.Open(@'\n{sql}\n'@, $c); while(-not $r.EOF){{ $r.Fields.Item('System.ItemPathDisplay').Value; $r.MoveNext() }}"
        ),
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd.output().map_err(|_| ())?;
    if !output.status.success() {
        return Err(());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in text.lines() {
        let p = line.trim();
        if p.is_empty() {
            continue;
        }
        let Ok(path) = orchid_fs::FsPath::from_local(std::path::Path::new(p)) else {
            continue;
        };
        let name = path.file_name().unwrap_or_default().to_string();
        let entry = orchid_fs::FsEntry {
            name,
            path,
            metadata: orchid_fs::FsMetadata {
                kind: orchid_fs::FsEntryKind::File,
                size: 0,
                created: None,
                modified: None,
                accessed: None,
                readonly: false,
                hidden: false,
                system: false,
                mime: None,
                extended: orchid_fs::ExtendedAttributes::default(),
            },
        };
        if spec.matches_meta(&entry) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: u64, dir: bool) -> orchid_fs::FsEntry {
        orchid_fs::FsEntry {
            path: orchid_fs::FsPath::new(format!("local:/tmp/{name}")).unwrap(),
            name: name.into(),
            metadata: orchid_fs::FsMetadata {
                kind: if dir {
                    orchid_fs::FsEntryKind::Directory
                } else {
                    orchid_fs::FsEntryKind::File
                },
                size,
                created: None,
                modified: None,
                accessed: None,
                readonly: false,
                hidden: false,
                system: false,
                mime: None,
                extended: orchid_fs::ExtendedAttributes::default(),
            },
        }
    }

    #[test]
    fn pack_roundtrip() {
        let s = FindSpec {
            root: "local:/docs".into(),
            name: "*.txt".into(),
            name_regex: false,
            case_sensitive: true,
            recursive: true,
            min_size: Some(10),
            content: "hello".into(),
            content_regex: true,
            in_archives: true,
            indexed: true,
            save_as_virtual: true,
            files: true,
            folders: false,
            ..FindSpec::default()
        };
        let back = FindSpec::unpack(&s.pack(), "local:/other");
        assert_eq!(back.name, "*.txt");
        assert!(back.case_sensitive);
        assert!(back.recursive);
        assert_eq!(back.min_size, Some(10));
        assert!(back.content_regex);
        assert_eq!(back.root, "local:/docs");
        assert!(!back.folders);
    }

    #[test]
    fn name_glob_and_regex() {
        let mut s = FindSpec::in_root("local:/");
        s.name = "*.txt".into();
        assert!(s.matches_name("a.txt"));
        assert!(!s.matches_name("a.rs"));
        s.name = "foo.*".into();
        s.name_regex = true;
        assert!(s.matches_name("foo.txt"));
        s.case_sensitive = true;
        s.name_regex = false;
        s.name = "Foo".into();
        assert!(!s.matches_name("foo"));
        assert!(s.matches_name("xFooY"));
    }

    #[test]
    fn size_and_content() {
        let mut s = FindSpec::in_root("local:/");
        s.min_size = Some(100);
        assert!(!s.matches_meta(&entry("a", 10, false)));
        assert!(s.matches_meta(&entry("a", 100, false)));
        s.content = "Hello".into();
        assert!(s.matches_content("say Hello"));
        s.case_sensitive = true;
        assert!(!s.matches_content("say hello"));
    }
}
