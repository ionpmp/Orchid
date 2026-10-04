//! Map a [`CleanerId`] onto filesystem jobs and platform actions for one
//! [`HostLayout`]. Tests pass a layout rooted in a temporary directory.

use std::path::{Path, PathBuf};

use super::catalog::CleanerId;
use super::firefox;
use super::fs::{self, CleanStats, FsJob};
use super::platform::{self, PlatformError};

/// Directories the cleaners are allowed to read. Production fills this from
/// the environment. Tests point every field at a scratch tree.
#[derive(Debug, Clone)]
pub struct HostLayout {
    /// User temporary directory.
    pub temp: PathBuf,
    /// `Windows\Temp`.
    pub windows_temp: PathBuf,
    /// `%LOCALAPPDATA%`.
    pub local_app_data: PathBuf,
    /// `%APPDATA%`.
    pub app_data: PathBuf,
    /// User profile directory.
    pub home: PathBuf,
    /// `%ProgramData%`.
    pub program_data: PathBuf,
    /// Windows directory.
    pub windows_dir: PathBuf,
}

impl HostLayout {
    /// Layout for this process.
    #[must_use]
    pub fn from_env() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"));
        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"));
        let windows = std::env::var_os("WINDIR")
            .or_else(|| std::env::var_os("SystemRoot"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let program_data = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        Self {
            temp: std::env::temp_dir(),
            windows_temp: windows.join("Temp"),
            local_app_data: local,
            app_data: roaming,
            home,
            program_data,
            windows_dir: windows,
        }
    }
}

/// Result of cleaning one id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanReport {
    /// Files removed, or files that a scan would have counted.
    pub stats: CleanStats,
    /// First platform or browser error. Filesystem skips stay in `stats`.
    pub error: Option<PlatformError>,
}

/// Count what [`clean_cleaner`] would remove. Platform actions that have no
/// file size (DNS, recycle bin, registry) contribute no bytes.
#[must_use]
pub fn scan_cleaner(id: CleanerId, host: &HostLayout) -> CleanStats {
    let mut stats = fs::scan_jobs(&fs_jobs(id, host));
    if matches!(id, CleanerId::BrowserHistory) {
        for profile in firefox_profiles(host) {
            let places = profile.join("places.sqlite");
            if let Ok(meta) = std::fs::symlink_metadata(&places) {
                if meta.is_file() {
                    stats.files += 1;
                    stats.bytes += meta.len();
                }
            }
        }
    }
    stats
}

/// Run one cleaner against `host`.
#[must_use]
pub fn clean_cleaner(id: CleanerId, host: &HostLayout) -> CleanReport {
    let mut stats = fs::clean_jobs(&fs_jobs(id, host));
    let mut error = None;
    for action in extra_actions(id) {
        let result = match action {
            Extra::RecycleBin => platform::empty_recycle().map(|_| CleanStats::default()),
            Extra::Dns => platform::flush_dns().map(|_| CleanStats::default()),
            Extra::Clipboard => platform::clear_clipboard().map(|_| CleanStats::default()),
            Extra::Registry(keys) => platform::clear_registry_keys(keys).map(|n| CleanStats {
                files: u64::from(n),
                bytes: 0,
                skipped: 0,
            }),
            Extra::Advertising => platform::disable_advertising_id().map(|_| CleanStats::default()),
            Extra::FirefoxHistory => clear_firefox(host),
        };
        match result {
            Ok(extra) => stats.add_from(extra),
            Err(err) if error.is_none() => error = Some(err),
            Err(_) => {}
        }
    }
    CleanReport { stats, error }
}

impl CleanStats {
    fn add_from(&mut self, other: Self) {
        self.files += other.files;
        self.bytes += other.bytes;
        self.skipped += other.skipped;
    }
}

enum Extra {
    RecycleBin,
    Dns,
    Clipboard,
    Registry(&'static [&'static str]),
    Advertising,
    FirefoxHistory,
}

fn extra_actions(id: CleanerId) -> Vec<Extra> {
    match id {
        CleanerId::Recycle => vec![Extra::RecycleBin],
        CleanerId::Dns => vec![Extra::Dns],
        CleanerId::Clipboard => vec![Extra::Clipboard],
        CleanerId::Advertising => vec![Extra::Advertising],
        CleanerId::RunMru => vec![Extra::Registry(&RUN_MRU)],
        CleanerId::SearchMru => vec![Extra::Registry(&SEARCH_MRU)],
        CleanerId::TypedPaths => vec![Extra::Registry(&TYPED_PATHS)],
        CleanerId::OpenSave => vec![Extra::Registry(&OPEN_SAVE)],
        CleanerId::UserAssist => vec![Extra::Registry(&USER_ASSIST)],
        CleanerId::BrowserHistory => vec![Extra::FirefoxHistory],
        _ => Vec::new(),
    }
}

const RUN_MRU: &[&str] = &[r"Software\Microsoft\Windows\CurrentVersion\Explorer\RunMRU"];
const SEARCH_MRU: &[&str] = &[r"Software\Microsoft\Windows\CurrentVersion\Explorer\WordWheelQuery"];
const TYPED_PATHS: &[&str] = &[r"Software\Microsoft\Windows\CurrentVersion\Explorer\TypedPaths"];
const OPEN_SAVE: &[&str] = &[
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\RecentDocs",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\ComDlg32\OpenSavePidlMRU",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\ComDlg32\LastVisitedPidlMRU",
];
const USER_ASSIST: &[&str] = &[r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist"];

fn fs_jobs(id: CleanerId, host: &HostLayout) -> Vec<FsJob> {
    match id {
        CleanerId::UserTemp => vec![FsJob::Descend {
            dir: host.temp.clone(),
        }],
        CleanerId::WindowsTemp => {
            if paths_equal(&host.temp, &host.windows_temp) {
                Vec::new()
            } else {
                vec![FsJob::Descend {
                    dir: host.windows_temp.clone(),
                }]
            }
        }
        CleanerId::Thumbnails => vec![FsJob::Prefix {
            dir: host
                .local_app_data
                .join("Microsoft")
                .join("Windows")
                .join("Explorer"),
            prefix: "thumbcache_".into(),
        }],
        CleanerId::ErrorReports => vec![
            FsJob::Descend {
                dir: host
                    .local_app_data
                    .join("Microsoft")
                    .join("Windows")
                    .join("WER"),
            },
            FsJob::Descend {
                dir: host
                    .program_data
                    .join("Microsoft")
                    .join("Windows")
                    .join("WER"),
            },
        ],
        CleanerId::Delivery => vec![
            FsJob::Descend {
                dir: host
                    .windows_dir
                    .join("SoftwareDistribution")
                    .join("DeliveryOptimization"),
            },
            FsJob::Descend {
                dir: host
                    .windows_dir
                    .join("ServiceProfiles")
                    .join("NetworkService")
                    .join("AppData")
                    .join("Local")
                    .join("Microsoft")
                    .join("Windows")
                    .join("DeliveryOptimization")
                    .join("Cache"),
            },
        ],
        CleanerId::Clipboard => vec![FsJob::Descend {
            dir: host
                .local_app_data
                .join("Microsoft")
                .join("Windows")
                .join("Clipboard"),
        }],
        CleanerId::Recent => vec![FsJob::Suffix {
            dir: recent_dir(host),
            suffix: ".lnk".into(),
        }],
        CleanerId::JumpLists => vec![
            FsJob::Descend {
                dir: recent_dir(host).join("AutomaticDestinations"),
            },
            FsJob::Descend {
                dir: recent_dir(host).join("CustomDestinations"),
            },
        ],
        CleanerId::OfficeRecent => vec![FsJob::Descend {
            dir: host
                .app_data
                .join("Microsoft")
                .join("Office")
                .join("Recent"),
        }],
        CleanerId::Activity => vec![FsJob::Descend {
            dir: host.local_app_data.join("ConnectedDevicesPlatform"),
        }],
        CleanerId::BrowserCache => browser_cache_jobs(host),
        CleanerId::BrowserHistory => browser_history_jobs(host),
        CleanerId::BrowserCookies => browser_cookie_jobs(host),
        CleanerId::Recycle
        | CleanerId::Dns
        | CleanerId::Advertising
        | CleanerId::RunMru
        | CleanerId::SearchMru
        | CleanerId::TypedPaths
        | CleanerId::OpenSave
        | CleanerId::UserAssist => Vec::new(),
    }
}

fn recent_dir(host: &HostLayout) -> PathBuf {
    host.app_data
        .join("Microsoft")
        .join("Windows")
        .join("Recent")
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['\\', '/']).as_ref())
}

fn clear_firefox(host: &HostLayout) -> Result<CleanStats, PlatformError> {
    let mut stats = CleanStats::default();
    let mut error = None;
    for profile in firefox_profiles(host) {
        let places = profile.join("places.sqlite");
        match firefox::clear_history(&places) {
            Ok(n) => stats.files += n,
            Err(err) if error.is_none() => error = Some(PlatformError::Other(err)),
            Err(_) => stats.skipped += 1,
        }
    }
    match error {
        Some(err) if stats.files == 0 => Err(err),
        _ => Ok(stats),
    }
}

const CHROMIUM_HISTORY: &[&str] = &[
    "History",
    "History-journal",
    "History-wal",
    "History-shm",
    "Visited Links",
    "Top Sites",
    "Top Sites-journal",
    "Shortcuts",
    "Shortcuts-journal",
];

const CHROMIUM_COOKIES: &[&str] = &[
    "Cookies",
    "Cookies-journal",
    "Network/Cookies",
    "Network/Cookies-journal",
];

const CHROMIUM_CACHE: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "Service Worker/CacheStorage",
];

fn browser_history_jobs(host: &HostLayout) -> Vec<FsJob> {
    let mut paths = Vec::new();
    for profile in chromium_profiles(host) {
        for name in CHROMIUM_HISTORY {
            paths.push(profile.join(name));
        }
    }
    vec![FsJob::Files { paths }]
}

fn browser_cookie_jobs(host: &HostLayout) -> Vec<FsJob> {
    let mut paths = Vec::new();
    for profile in chromium_profiles(host) {
        for name in CHROMIUM_COOKIES {
            paths.push(profile.join(name));
        }
    }
    for profile in firefox_profiles(host) {
        for name in ["cookies.sqlite", "cookies.sqlite-wal", "cookies.sqlite-shm"] {
            paths.push(profile.join(name));
        }
    }
    vec![FsJob::Files { paths }]
}

fn browser_cache_jobs(host: &HostLayout) -> Vec<FsJob> {
    let mut jobs = Vec::new();
    for profile in chromium_profiles(host) {
        for name in CHROMIUM_CACHE {
            jobs.push(FsJob::Descend {
                dir: profile.join(name),
            });
        }
    }
    for profile in firefox_profiles(host) {
        for name in ["cache2", "startupCache", "thumbnails"] {
            jobs.push(FsJob::Descend {
                dir: profile.join(name),
            });
        }
    }
    jobs
}

fn chromium_profiles(host: &HostLayout) -> Vec<PathBuf> {
    let roots = [
        host.local_app_data
            .join("Google")
            .join("Chrome")
            .join("User Data"),
        host.local_app_data
            .join("Microsoft")
            .join("Edge")
            .join("User Data"),
        host.local_app_data
            .join("BraveSoftware")
            .join("Brave-Browser")
            .join("User Data"),
        host.local_app_data.join("Chromium").join("User Data"),
        host.local_app_data.join("Vivaldi").join("User Data"),
        host.app_data.join("Opera Software").join("Opera Stable"),
        host.app_data.join("Opera Software").join("Opera GX Stable"),
    ];
    let mut out = Vec::new();
    for root in roots {
        push_profile(&root, &mut out);
        let Ok(rd) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in rd.flatten() {
            push_profile(&entry.path(), &mut out);
        }
    }
    out
}

fn push_profile(dir: &Path, out: &mut Vec<PathBuf>) {
    if !is_chromium_profile(dir) {
        return;
    }
    if !out.iter().any(|existing| existing == dir) {
        out.push(dir.to_path_buf());
    }
}

fn is_chromium_profile(dir: &Path) -> bool {
    ["History", "Preferences", "Bookmarks", "Cookies"]
        .iter()
        .any(|name| dir.join(name).is_file())
        || dir.join("Network").join("Cookies").is_file()
        || dir.join("Cache").is_dir()
}

fn firefox_profiles(host: &HostLayout) -> Vec<PathBuf> {
    let bases = [
        host.app_data
            .join("Mozilla")
            .join("Firefox")
            .join("Profiles"),
        host.local_app_data
            .join("Mozilla")
            .join("Firefox")
            .join("Profiles"),
    ];
    let mut out = Vec::new();
    for base in bases {
        let Ok(rd) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.join("places.sqlite").is_file() || path.join("cache2").is_dir() {
                if !out.iter().any(|existing| existing == &path) {
                    out.push(path);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn layout(root: &Path) -> HostLayout {
        HostLayout {
            temp: root.join("temp"),
            windows_temp: root.join("windows-temp"),
            local_app_data: root.join("local"),
            app_data: root.join("roaming"),
            home: root.join("home"),
            program_data: root.join("programdata"),
            windows_dir: root.join("windows"),
        }
    }

    fn touch(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut file = File::create(path).expect("create");
        file.write_all(bytes).expect("write");
    }

    #[test]
    fn browser_history_removes_history_and_keeps_bookmarks() {
        let root = tempfile::tempdir().expect("temp");
        let host = layout(root.path());
        let profile = host
            .local_app_data
            .join("Google")
            .join("Chrome")
            .join("User Data")
            .join("Default");
        touch(&profile.join("History"), b"history-bytes");
        touch(&profile.join("Bookmarks"), b"keep-me");
        touch(&profile.join("Cache").join("data.bin"), b"cached");
        let scanned = scan_cleaner(CleanerId::BrowserHistory, &host);
        assert_eq!(scanned.files, 1);
        assert_eq!(scanned.bytes, b"history-bytes".len() as u64);
        let cleaned = clean_cleaner(CleanerId::BrowserHistory, &host);
        assert!(cleaned.error.is_none());
        assert!(!profile.join("History").exists());
        assert_eq!(
            std::fs::read(profile.join("Bookmarks")).unwrap(),
            b"keep-me"
        );
        assert!(profile.join("Cache").join("data.bin").is_file());
        let cache = clean_cleaner(CleanerId::BrowserCache, &host);
        assert_eq!(cache.stats.files, 1);
        assert!(!profile.join("Cache").join("data.bin").exists());
    }

    #[test]
    fn recent_cleaner_only_removes_shortcuts() {
        let root = tempfile::tempdir().expect("temp");
        let host = layout(root.path());
        let recent = host
            .app_data
            .join("Microsoft")
            .join("Windows")
            .join("Recent");
        touch(&recent.join("doc.lnk"), b"lnk");
        touch(&recent.join("desktop.ini"), b"ini");
        touch(
            &recent
                .join("AutomaticDestinations")
                .join("a.automaticDestinations-ms"),
            b"jump",
        );
        let cleaned = clean_cleaner(CleanerId::Recent, &host);
        assert_eq!(cleaned.stats.files, 1);
        assert!(recent.join("desktop.ini").is_file());
        assert!(recent
            .join("AutomaticDestinations")
            .join("a.automaticDestinations-ms")
            .is_file());
        let jumps = clean_cleaner(CleanerId::JumpLists, &host);
        assert_eq!(jumps.stats.files, 1);
    }
}
