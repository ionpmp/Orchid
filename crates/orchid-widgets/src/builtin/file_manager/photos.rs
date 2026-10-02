//! Photo library over hierarchical file tags.
//!
//! Tags stay free-form strings. A slash is a folder boundary:
//! `people/ada`, `event/2026-10-02/picnic`, `album/vacation`.
//! Files → Photos browses that tree. It does not detect faces.

use std::collections::BTreeSet;

/// One stored tag on one file path.
#[derive(Debug, Clone)]
pub(crate) struct TaggedFile {
    /// Normalised tag (`people/ada`).
    pub tag: String,
    /// Orchid path of the file (`local:c:/Photos/a.jpg`).
    pub path: String,
}

/// Translated names for the fixed library folders.
#[derive(Debug, Clone)]
pub(crate) struct PhotoLabels {
    /// `virtual:photos/people`.
    pub people: String,
    /// `virtual:photos/events`.
    pub events: String,
    /// `virtual:photos/albums`.
    pub albums: String,
    /// `virtual:photos/tags`.
    pub tags: String,
    /// Smart album of every `people/` tag.
    pub album_people: String,
    /// Smart album of every `event/` tag.
    pub album_events: String,
    /// Smart album of tags that are not people, events, or albums.
    pub album_other: String,
}

/// One row in a photo-library listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhotoRow {
    /// Display name.
    pub name: String,
    /// Virtual folder path, or the real file path.
    pub path: String,
    /// `true` when the row opens another library folder.
    pub directory: bool,
}

const FILE_CAP: usize = 2000;

/// Rows for `virtual:photos` and everything under it.
#[must_use]
pub(crate) fn photo_listing(
    tags: &[TaggedFile],
    virtual_path: &str,
    labels: &PhotoLabels,
) -> Vec<PhotoRow> {
    let Some(rest) = strip_root(virtual_path) else {
        return Vec::new();
    };
    if rest.is_empty() {
        return vec![
            dir(&labels.people, "virtual:photos/people"),
            dir(&labels.events, "virtual:photos/events"),
            dir(&labels.albums, "virtual:photos/albums"),
            dir(&labels.tags, "virtual:photos/tags"),
        ];
    }
    let (head, tail) = split_head(rest);
    match head {
        "people" => tree(
            tags,
            &join_prefix("people", tail),
            "virtual:photos/people",
            tail,
        ),
        "events" => tree(
            tags,
            &join_prefix("event", tail),
            "virtual:photos/events",
            tail,
        ),
        "tags" => tree(tags, tail, "virtual:photos/tags", tail),
        "albums" => albums(tags, tail, labels),
        _ => Vec::new(),
    }
}

/// Tags implied by a `People` or `Events` directory above an image file.
///
/// Only image extensions are considered. Names are lowercased to match
/// stored tags. A picture sitting directly in `People` (with no person
/// folder) is left untagged.
#[must_use]
pub(crate) fn auto_tags_for_path(path: &str) -> Vec<String> {
    let slashed = path.replace('\\', "/");
    let ext = slashed.rsplit('.').next().unwrap_or("");
    if ext.contains('/') || !orchid_viewers::is_image_file_extension(ext) {
        return Vec::new();
    }
    let body = path_body(&slashed);
    let parts: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        return Vec::new();
    }
    let dirs = &parts[..parts.len() - 1];
    let mut tags = Vec::new();
    for (i, seg) in dirs.iter().enumerate() {
        let key = seg.to_lowercase();
        if key == "people" || key == "person" {
            if let Some(name) = dirs.get(i + 1) {
                let name = name.trim().to_lowercase();
                if valid_segment(&name) {
                    push_unique(&mut tags, format!("people/{name}"));
                }
            }
        } else if key == "events" || key == "event" {
            let rest: Vec<String> = dirs[i + 1..]
                .iter()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| valid_segment(s))
                .collect();
            if !rest.is_empty() {
                push_unique(&mut tags, format!("event/{}", rest.join("/")));
            }
        }
    }
    tags
}

fn albums(tags: &[TaggedFile], tail: &str, labels: &PhotoLabels) -> Vec<PhotoRow> {
    if tail.is_empty() {
        let mut rows = vec![
            dir(&labels.album_people, "virtual:photos/albums/people"),
            dir(&labels.album_events, "virtual:photos/albums/events"),
            dir(&labels.album_other, "virtual:photos/albums/other"),
        ];
        let (folders, _) = split(tags, "album");
        for name in folders {
            if name == "people" || name == "events" || name == "other" {
                continue;
            }
            rows.push(dir(&name, &format!("virtual:photos/albums/{name}")));
        }
        return rows;
    }
    if tail == "people" {
        return files_where(tags, |tag| tag == "people" || tag.starts_with("people/"));
    }
    if tail == "events" {
        return files_where(tags, |tag| tag == "event" || tag.starts_with("event/"));
    }
    if tail == "other" {
        return files_where(tags, |tag| {
            tag != "people"
                && !tag.starts_with("people/")
                && tag != "event"
                && !tag.starts_with("event/")
                && tag != "album"
                && !tag.starts_with("album/")
        });
    }
    tree(
        tags,
        &format!("album/{tail}"),
        "virtual:photos/albums",
        tail,
    )
}

fn tree(tags: &[TaggedFile], prefix: &str, base_virtual: &str, tail: &str) -> Vec<PhotoRow> {
    let base = if tail.is_empty() {
        base_virtual.to_string()
    } else {
        format!("{base_virtual}/{tail}")
    };
    let (folders, files) = split(tags, prefix);
    let mut rows = Vec::new();
    for name in folders {
        rows.push(dir(&name, &format!("{base}/{name}")));
    }
    let mut n = 0usize;
    for path in files {
        if n >= FILE_CAP {
            break;
        }
        n += 1;
        rows.push(file_row(&path));
    }
    rows
}

fn split(tags: &[TaggedFile], prefix: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut folders = BTreeSet::new();
    let mut files = BTreeSet::new();
    for t in tags {
        let Some(rest) = remainder(&t.tag, prefix) else {
            continue;
        };
        if rest.is_empty() {
            files.insert(t.path.clone());
            continue;
        }
        if let Some(child) = rest.split('/').next() {
            if !child.is_empty() {
                folders.insert(child.to_string());
            }
        }
    }
    (folders, files)
}

fn files_where(tags: &[TaggedFile], pred: impl Fn(&str) -> bool) -> Vec<PhotoRow> {
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for t in tags {
        if !pred(&t.tag) {
            continue;
        }
        if !seen.insert(t.path.clone()) {
            continue;
        }
        if rows.len() >= FILE_CAP {
            break;
        }
        rows.push(file_row(&t.path));
    }
    rows
}

fn remainder<'a>(tag: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        return Some(tag);
    }
    if tag == prefix {
        return Some("");
    }
    let stripped = tag.strip_prefix(prefix)?;
    stripped.strip_prefix('/')
}

fn strip_root(path: &str) -> Option<&str> {
    if path == "virtual:photos" {
        Some("")
    } else {
        path.strip_prefix("virtual:photos/")
    }
}

fn split_head(rest: &str) -> (&str, &str) {
    match rest.split_once('/') {
        Some((head, tail)) => (head, tail),
        None => (rest, ""),
    }
}

fn join_prefix(root: &str, tail: &str) -> String {
    if tail.is_empty() {
        root.to_string()
    } else {
        format!("{root}/{tail}")
    }
}

fn dir(name: &str, path: &str) -> PhotoRow {
    PhotoRow {
        name: name.to_string(),
        path: path.to_string(),
        directory: true,
    }
}

fn file_row(path: &str) -> PhotoRow {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    PhotoRow {
        name: name.to_string(),
        path: path.to_string(),
        directory: false,
    }
}

fn path_body(slashed: &str) -> &str {
    match slashed.split_once(':') {
        Some((scheme, rest))
            if !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') =>
        {
            rest
        }
        _ => slashed,
    }
}

fn valid_segment(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".."
}

fn push_unique(tags: &mut Vec<String>, tag: String) {
    if !tags.iter().any(|t| t == &tag) {
        tags.push(tag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> PhotoLabels {
        PhotoLabels {
            people: "People".into(),
            events: "Events".into(),
            albums: "Albums".into(),
            tags: "Tags".into(),
            album_people: "All people".into(),
            album_events: "All events".into(),
            album_other: "Other".into(),
        }
    }

    fn tagged(tag: &str, path: &str) -> TaggedFile {
        TaggedFile {
            tag: tag.into(),
            path: path.into(),
        }
    }

    #[test]
    fn root_lists_four_folders() {
        let rows = photo_listing(&[], "virtual:photos", &labels());
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|r| r.directory));
        assert_eq!(rows[0].path, "virtual:photos/people");
    }

    #[test]
    fn person_folder_holds_the_file() {
        let tags = [tagged("people/ada", "local:c:/Photos/a.jpg")];
        let people = photo_listing(&tags, "virtual:photos/people", &labels());
        assert_eq!(people.len(), 1);
        assert!(people[0].directory);
        assert_eq!(people[0].name, "ada");
        let ada = photo_listing(&tags, "virtual:photos/people/ada", &labels());
        assert_eq!(ada.len(), 1);
        assert!(!ada[0].directory);
        assert_eq!(ada[0].name, "a.jpg");
    }

    #[test]
    fn people2_is_not_inside_people() {
        let tags = [tagged("people2", "local:c:/x.jpg")];
        let people = photo_listing(&tags, "virtual:photos/people", &labels());
        assert!(people.is_empty());
        let root_tags = photo_listing(&tags, "virtual:photos/tags", &labels());
        assert_eq!(root_tags[0].name, "people2");
    }

    #[test]
    fn event_date_is_a_folder() {
        let tags = [tagged("event/2026-10-02/picnic", "local:c:/Photos/p.jpg")];
        let events = photo_listing(&tags, "virtual:photos/events", &labels());
        assert_eq!(events[0].name, "2026-10-02");
        let day = photo_listing(&tags, "virtual:photos/events/2026-10-02", &labels());
        assert_eq!(day[0].name, "picnic");
        let picnic = photo_listing(&tags, "virtual:photos/events/2026-10-02/picnic", &labels());
        assert_eq!(picnic[0].path, "local:c:/Photos/p.jpg");
    }

    #[test]
    fn smart_albums_split_people_events_and_other() {
        let tags = [
            tagged("people/ada", "local:c:/a.jpg"),
            tagged("event/picnic", "local:c:/p.jpg"),
            tagged("album/vacation", "local:c:/v.jpg"),
            tagged("work", "local:c:/w.jpg"),
            tagged("album/people", "local:c:/hidden.jpg"),
        ];
        let albums = photo_listing(&tags, "virtual:photos/albums", &labels());
        let paths: Vec<&str> = albums.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"virtual:photos/albums/people"));
        assert!(paths.contains(&"virtual:photos/albums/vacation"));
        assert_eq!(
            paths
                .iter()
                .filter(|p| **p == "virtual:photos/albums/people")
                .count(),
            1
        );
        let people = photo_listing(&tags, "virtual:photos/albums/people", &labels());
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].path, "local:c:/a.jpg");
        let other = photo_listing(&tags, "virtual:photos/albums/other", &labels());
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].path, "local:c:/w.jpg");
        let vacation = photo_listing(&tags, "virtual:photos/albums/vacation", &labels());
        assert_eq!(vacation[0].path, "local:c:/v.jpg");
    }

    #[test]
    fn auto_tag_reads_people_and_events_folders() {
        assert_eq!(
            auto_tags_for_path("local:c:/Photos/People/Ada/img.jpg"),
            vec!["people/ada".to_string()]
        );
        assert_eq!(
            auto_tags_for_path(r"D:\Events\2026-10-02\Picnic\a.PNG"),
            vec!["event/2026-10-02/picnic".to_string()]
        );
        assert!(auto_tags_for_path("local:c:/People/Ada/readme.txt").is_empty());
        assert!(auto_tags_for_path("local:c:/Photos/People/img.jpg").is_empty());
        let both = auto_tags_for_path("local:c:/People/Ada/Events/Picnic/a.jpg");
        assert_eq!(
            both,
            vec!["people/ada".to_string(), "event/picnic".to_string()]
        );
    }
}
