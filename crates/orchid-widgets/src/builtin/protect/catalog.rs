//! Stable cleaner ids. The UI and the persisted config speak these strings.

/// Which list a cleaner appears on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtectTab {
    /// Temporary files, caches, recycle bin, clipboard, DNS.
    Traces,
    /// Shell, browser, and activity histories.
    Histories,
}

/// One cleanup target. The set is fixed; paths come from [`super::HostLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CleanerId {
    /// `%TEMP%` for this user.
    UserTemp,
    /// `Windows\Temp`. Often needs an administrator.
    WindowsTemp,
    /// Recycle bin on every drive.
    Recycle,
    /// Explorer `thumbcache_*.db`.
    Thumbnails,
    /// Windows Error Reporting for this user and the machine cache.
    ErrorReports,
    /// Delivery Optimization download cache.
    Delivery,
    /// Resolved-name cache (`ipconfig /flushdns`).
    Dns,
    /// Current clipboard and clipboard history files.
    Clipboard,
    /// Chromium and Firefox cache directories.
    BrowserCache,
    /// Turns off the per-user advertising identifier.
    Advertising,
    /// `.lnk` files in the Recent folder.
    Recent,
    /// Run dialog MRU.
    RunMru,
    /// Explorer search terms.
    SearchMru,
    /// Paths typed in the Explorer address bar.
    TypedPaths,
    /// Taskbar and Start jump lists.
    JumpLists,
    /// RecentDocs and common-dialog open/save MRU.
    OpenSave,
    /// UserAssist program-launch counts.
    UserAssist,
    /// Office Recent shortcuts.
    OfficeRecent,
    /// Chromium history files and Firefox history rows (bookmarks stay).
    BrowserHistory,
    /// Browser cookies. Off unless the user checks it.
    BrowserCookies,
    /// Timeline files under ConnectedDevicesPlatform.
    Activity,
}

impl CleanerId {
    /// Every cleaner, traces first, then histories.
    pub const ALL: [CleanerId; 21] = [
        Self::UserTemp,
        Self::WindowsTemp,
        Self::Recycle,
        Self::Thumbnails,
        Self::ErrorReports,
        Self::Delivery,
        Self::Dns,
        Self::Clipboard,
        Self::BrowserCache,
        Self::Advertising,
        Self::Recent,
        Self::RunMru,
        Self::SearchMru,
        Self::TypedPaths,
        Self::JumpLists,
        Self::OpenSave,
        Self::UserAssist,
        Self::OfficeRecent,
        Self::BrowserHistory,
        Self::BrowserCookies,
        Self::Activity,
    ];

    /// Stable id stored in the widget config.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserTemp => "user-temp",
            Self::WindowsTemp => "windows-temp",
            Self::Recycle => "recycle",
            Self::Thumbnails => "thumbnails",
            Self::ErrorReports => "error-reports",
            Self::Delivery => "delivery",
            Self::Dns => "dns",
            Self::Clipboard => "clipboard",
            Self::BrowserCache => "browser-cache",
            Self::Advertising => "advertising",
            Self::Recent => "recent",
            Self::RunMru => "run-mru",
            Self::SearchMru => "search-mru",
            Self::TypedPaths => "typed-paths",
            Self::JumpLists => "jump-lists",
            Self::OpenSave => "open-save",
            Self::UserAssist => "user-assist",
            Self::OfficeRecent => "office-recent",
            Self::BrowserHistory => "browser-history",
            Self::BrowserCookies => "browser-cookies",
            Self::Activity => "activity",
        }
    }

    /// Parse a config id.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == raw)
    }

    /// Tab that shows this cleaner.
    #[must_use]
    pub fn tab(self) -> ProtectTab {
        match self {
            Self::UserTemp
            | Self::WindowsTemp
            | Self::Recycle
            | Self::Thumbnails
            | Self::ErrorReports
            | Self::Delivery
            | Self::Dns
            | Self::Clipboard
            | Self::BrowserCache
            | Self::Advertising => ProtectTab::Traces,
            Self::Recent
            | Self::RunMru
            | Self::SearchMru
            | Self::TypedPaths
            | Self::JumpLists
            | Self::OpenSave
            | Self::UserAssist
            | Self::OfficeRecent
            | Self::BrowserHistory
            | Self::BrowserCookies
            | Self::Activity => ProtectTab::Histories,
        }
    }

    /// Fluent key for the row title.
    #[must_use]
    pub fn title_key(self) -> &'static str {
        match self {
            Self::UserTemp => "protect-item-user-temp",
            Self::WindowsTemp => "protect-item-windows-temp",
            Self::Recycle => "protect-item-recycle",
            Self::Thumbnails => "protect-item-thumbnails",
            Self::ErrorReports => "protect-item-error-reports",
            Self::Delivery => "protect-item-delivery",
            Self::Dns => "protect-item-dns",
            Self::Clipboard => "protect-item-clipboard",
            Self::BrowserCache => "protect-item-browser-cache",
            Self::Advertising => "protect-item-advertising",
            Self::Recent => "protect-item-recent",
            Self::RunMru => "protect-item-run-mru",
            Self::SearchMru => "protect-item-search-mru",
            Self::TypedPaths => "protect-item-typed-paths",
            Self::JumpLists => "protect-item-jump-lists",
            Self::OpenSave => "protect-item-open-save",
            Self::UserAssist => "protect-item-user-assist",
            Self::OfficeRecent => "protect-item-office-recent",
            Self::BrowserHistory => "protect-item-browser-history",
            Self::BrowserCookies => "protect-item-browser-cookies",
            Self::Activity => "protect-item-activity",
        }
    }

    /// Fluent key for the row explanation.
    #[must_use]
    pub fn detail_key(self) -> &'static str {
        match self {
            Self::UserTemp => "protect-detail-user-temp",
            Self::WindowsTemp => "protect-detail-windows-temp",
            Self::Recycle => "protect-detail-recycle",
            Self::Thumbnails => "protect-detail-thumbnails",
            Self::ErrorReports => "protect-detail-error-reports",
            Self::Delivery => "protect-detail-delivery",
            Self::Dns => "protect-detail-dns",
            Self::Clipboard => "protect-detail-clipboard",
            Self::BrowserCache => "protect-detail-browser-cache",
            Self::Advertising => "protect-detail-advertising",
            Self::Recent => "protect-detail-recent",
            Self::RunMru => "protect-detail-run-mru",
            Self::SearchMru => "protect-detail-search-mru",
            Self::TypedPaths => "protect-detail-typed-paths",
            Self::JumpLists => "protect-detail-jump-lists",
            Self::OpenSave => "protect-detail-open-save",
            Self::UserAssist => "protect-detail-user-assist",
            Self::OfficeRecent => "protect-detail-office-recent",
            Self::BrowserHistory => "protect-detail-browser-history",
            Self::BrowserCookies => "protect-detail-browser-cookies",
            Self::Activity => "protect-detail-activity",
        }
    }

    /// Checked the first time the widget is created.
    ///
    /// Windows Temp needs an administrator. Cookies sign the user out.
    /// The advertising id is a setting change, so it stays off until chosen.
    #[must_use]
    pub fn default_on(self) -> bool {
        !matches!(
            self,
            Self::WindowsTemp | Self::BrowserCookies | Self::Advertising
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_cover_both_tabs() {
        let mut traces = 0;
        let mut histories = 0;
        for id in CleanerId::ALL {
            assert_eq!(CleanerId::parse(id.as_str()), Some(id));
            match id.tab() {
                ProtectTab::Traces => traces += 1,
                ProtectTab::Histories => histories += 1,
            }
            assert!(id.title_key().starts_with("protect-item-"));
            assert!(id.detail_key().starts_with("protect-detail-"));
        }
        assert!(traces > 0 && histories > 0);
        assert_eq!(CleanerId::parse("registry-cleaner"), None);
        assert!(!CleanerId::BrowserCookies.default_on());
        assert!(CleanerId::UserTemp.default_on());
    }
}
