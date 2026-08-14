//! Batch-rename pattern expansion (`{name}`, `{ext}`, `{n}`).

/// Split `file.tar.gz` → (`file.tar`, `.gz`); `Makefile` → (`Makefile`, ``).
#[must_use]
pub fn split_stem_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(0) | None => (name, ""),
        Some(i) => (&name[..i], &name[i..]),
    }
}

/// Apply find/replace then `{name}` `{ext}` `{n}` `{nn}` `{nnn}` placeholders.
///
/// `index` is 0-based; `{n}` is 1-based.
#[must_use]
pub fn apply_rename_pattern(
    original: &str,
    index: usize,
    pattern: &str,
    find: &str,
    replace: &str,
) -> String {
    let replaced = if find.is_empty() {
        original.to_string()
    } else {
        original.replace(find, replace)
    };
    let (stem, ext) = split_stem_ext(&replaced);
    let n = index + 1;
    let pat = if pattern.trim().is_empty() {
        "{name}{ext}"
    } else {
        pattern
    };
    pat.replace("{name}", stem)
        .replace("{ext}", ext)
        .replace("{nnn}", &format!("{n:03}"))
        .replace("{nn}", &format!("{n:02}"))
        .replace("{n}", &n.to_string())
}

/// Next unused sibling name: `file.txt` → `file (2).txt`.
#[must_use]
pub fn unique_numbered_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = split_stem_ext(name);
    for i in 2..10_000 {
        let candidate = format!("{stem} ({i}){ext}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{stem} ({}){ext}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_substitutes_placeholders() {
        assert_eq!(
            apply_rename_pattern("photo.jpg", 0, "{name}_{n}{ext}", "", ""),
            "photo_1.jpg"
        );
        assert_eq!(
            apply_rename_pattern("photo.jpg", 8, "{nnn}{ext}", "", ""),
            "009.jpg"
        );
        assert_eq!(
            apply_rename_pattern("a_b.txt", 0, "{name}{ext}", "_", "-"),
            "a-b.txt"
        );
    }

    #[test]
    fn unique_name_skips_taken() {
        let taken = |s: &str| s == "a.txt" || s == "a (2).txt";
        assert_eq!(unique_numbered_name("a.txt", taken), "a (3).txt");
    }
}
