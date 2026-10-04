//! Browsers that keep history, cache, and cookie files open.

/// `true` when this process keeps Chromium or Firefox profile files locked.
#[must_use]
pub fn locks_browser_files(process_name: &str) -> bool {
    let base = process_name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(process_name);
    let stem = base
        .strip_suffix(".exe")
        .or_else(|| base.strip_suffix(".EXE"))
        .unwrap_or(base);
    matches!(
        stem.to_ascii_lowercase().as_str(),
        "chrome"
            | "msedge"
            | "brave"
            | "vivaldi"
            | "opera"
            | "opera_gx"
            | "firefox"
            | "chromium"
            | "waterfox"
            | "librewolf"
    )
}

/// History, cache, and cookies cannot be removed while a browser holds them.
#[must_use]
pub fn cleaner_needs_closed_browser(id: super::catalog::CleanerId) -> bool {
    matches!(
        id,
        super::catalog::CleanerId::BrowserHistory
            | super::catalog::CleanerId::BrowserCache
            | super::catalog::CleanerId::BrowserCookies
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_browser_executables() {
        assert!(locks_browser_files("chrome.exe"));
        assert!(locks_browser_files(
            r"C:\Program Files\Mozilla Firefox\firefox.exe"
        ));
        assert!(locks_browser_files("msedge.EXE"));
        assert!(!locks_browser_files("orchid.exe"));
        assert!(!locks_browser_files("chrome_proxy.exe"));
        assert!(!locks_browser_files("explorer.exe"));
    }
}
