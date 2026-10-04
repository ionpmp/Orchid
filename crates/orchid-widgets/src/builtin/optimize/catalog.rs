//! Reversible Windows settings shown by the Optimize widget.
//!
//! The set is the overlap of three familiar tools, kept to changes Windows
//! itself already supports:
//! - Windows Update policies (no reboot while signed in, install mode, active
//!   hours, delivery optimization, driver updates) from the Microsoft Update
//!   policy reference and Winaero Tweaker.
//! - The recommended privacy and suggestion switches from O&O ShutUp10 (advertising
//!   id, tailored experiences, required diagnostics, activity history, feedback,
//!   speech, typing, lock screen, Start, Settings, welcome, consumer features).
//! - Explorer and taskbar behavior from Winaero Tweaker (extensions, hidden
//!   files, This PC, recent files, full path, classic context menu, alignment,
//!   search, Widgets, Copilot, combining buttons, menu delay, startup delay).
//!
//! Defender, the firewall, SmartScreen, UAC, and the Windows Update service are
//! not in this list. Inbox apps are not removed.

/// Result of writing one plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyStatus {
    /// The plan is stored.
    Applied,
    /// The administrator prompt was dismissed, or Windows refused the write.
    Denied,
    /// The write failed for another reason.
    Failed,
    /// This system is not Windows.
    Unsupported,
}

/// Registry hive a tweak writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hive {
    /// `HKEY_CURRENT_USER`.
    Cu,
    /// `HKEY_LOCAL_MACHINE`.
    Lm,
}

impl Hive {
    /// `.reg` hive name.
    #[must_use]
    pub fn reg_name(self) -> &'static str {
        match self {
            Self::Cu => "HKEY_CURRENT_USER",
            Self::Lm => "HKEY_LOCAL_MACHINE",
        }
    }
}

/// One registry change. Paths and names are fixed; nothing here comes from the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegOp {
    /// Set a `REG_DWORD`.
    Dword {
        /// Hive.
        hive: Hive,
        /// Key path under the hive.
        key: &'static str,
        /// Value name.
        name: &'static str,
        /// Value.
        value: u32,
    },
    /// Set a `REG_SZ`.
    Sz {
        /// Hive.
        hive: Hive,
        /// Key path under the hive.
        key: &'static str,
        /// Value name.
        name: &'static str,
        /// Value.
        value: &'static str,
    },
    /// Delete a value. Missing values count as success.
    DeleteValue {
        /// Hive.
        hive: Hive,
        /// Key path under the hive.
        key: &'static str,
        /// Value name.
        name: &'static str,
    },
    /// Create the key and set its default value to an empty string.
    EmptyDefault {
        /// Hive.
        hive: Hive,
        /// Key path under the hive.
        key: &'static str,
    },
    /// Delete a key tree. A missing key counts as success.
    DeleteKey {
        /// Hive.
        hive: Hive,
        /// Key path under the hive.
        key: &'static str,
    },
}

impl RegOp {
    fn hive(self) -> Hive {
        match self {
            Self::Dword { hive, .. }
            | Self::Sz { hive, .. }
            | Self::DeleteValue { hive, .. }
            | Self::EmptyDefault { hive, .. }
            | Self::DeleteKey { hive, .. } => hive,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Dword { key, .. }
            | Self::Sz { key, .. }
            | Self::DeleteValue { key, .. }
            | Self::EmptyDefault { key, .. }
            | Self::DeleteKey { key, .. } => key,
        }
    }
}

/// How to recognise that a plan is the one currently stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// DWORD exists and equals `value`.
    DwordEq {
        /// Hive.
        hive: Hive,
        /// Key path under the hive.
        key: &'static str,
        /// Value name.
        name: &'static str,
        /// Expected value.
        value: u32,
    },
    /// String exists and equals `value`.
    SzEq {
        /// Hive.
        hive: Hive,
        /// Key path under the hive.
        key: &'static str,
        /// Value name.
        name: &'static str,
        /// Expected value.
        value: &'static str,
    },
    /// The key exists.
    KeyExists {
        /// Hive.
        hive: Hive,
        /// Key path under the hive.
        key: &'static str,
    },
}

/// One setting on a tab.
#[derive(Debug, Clone, Copy)]
pub struct TweakDef {
    /// Stable id sent by the UI.
    pub id: &'static str,
    /// Tab index. See [`TAB_UPDATES`].
    pub tab: u8,
    /// Fluent key for the title.
    pub title_key: &'static str,
    /// Fluent key for the explanation.
    pub detail_key: &'static str,
    /// Choice labels. Empty means a switch (plan 0 off, plan 1 on).
    pub option_keys: &'static [&'static str],
    /// `plans[i]` is applied when that choice is selected.
    pub plans: &'static [&'static [RegOp]],
    /// First matching probe list wins. Otherwise [`Self::fallback`].
    pub detect: &'static [(u8, &'static [Probe])],
    /// Index used when no probe list matches.
    pub fallback: u8,
    /// Machine policy. Applying it asks Windows for an administrator.
    pub needs_admin: bool,
    /// Explorer or the taskbar reads this after a restart.
    pub restarts_explorer: bool,
}

/// Updates tab.
pub const TAB_UPDATES: u8 = 0;
/// Privacy tab.
pub const TAB_PRIVACY: u8 = 1;
/// Explorer tab.
pub const TAB_EXPLORER: u8 = 2;
/// Taskbar and search tab.
pub const TAB_SHELL: u8 = 3;
/// Suggestions tab.
pub const TAB_QUIET: u8 = 4;
/// Performance tab.
pub const TAB_PERFORMANCE: u8 = 5;
/// Number of tabs.
pub const TAB_COUNT: u8 = 6;

const AU: &str = r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU";
const WU: &str = r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate";
const DELIVERY: &str = r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization";
const DATA: &str = r"SOFTWARE\Policies\Microsoft\Windows\DataCollection";
const POLICY_SYSTEM: &str = r"SOFTWARE\Policies\Microsoft\Windows\System";
const CLOUD: &str = r"SOFTWARE\Policies\Microsoft\Windows\CloudContent";

const ADV: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const CABINET: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\CabinetState";
const CLASSIC: &str = r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}";
const CLASSIC_INPROC: &str =
    r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32";
const ADS: &str = r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo";
const PRIVACY: &str = r"Software\Microsoft\Windows\CurrentVersion\Privacy";
const SIUF: &str = r"Software\Microsoft\Siuf\Rules";
const SPEECH: &str = r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy";
const INK: &str = r"Software\Microsoft\InputPersonalization";
const INK_STORE: &str = r"Software\Microsoft\InputPersonalization\TrainedDataStore";
const PERSONAL: &str = r"Software\Microsoft\Personalization\Settings";
const SEARCH_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\SearchSettings";
const SEARCH_POLICY: &str = r"Software\Policies\Microsoft\Windows\Explorer";
const CONTENT: &str = r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
const SCOOBE: &str = r"Software\Microsoft\Windows\CurrentVersion\UserProfileEngagement";
const THEME: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const DESKTOP: &str = r"Control Panel\Desktop";
const BACKGROUND: &str = r"Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications";
const SEARCH: &str = r"Software\Microsoft\Windows\CurrentVersion\Search";
const SERIALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize";

const fn dword(hive: Hive, key: &'static str, name: &'static str, value: u32) -> RegOp {
    RegOp::Dword {
        hive,
        key,
        name,
        value,
    }
}

const fn sz(hive: Hive, key: &'static str, name: &'static str, value: &'static str) -> RegOp {
    RegOp::Sz {
        hive,
        key,
        name,
        value,
    }
}

const fn del(hive: Hive, key: &'static str, name: &'static str) -> RegOp {
    RegOp::DeleteValue { hive, key, name }
}

const fn eq_dword(hive: Hive, key: &'static str, name: &'static str, value: u32) -> Probe {
    Probe::DwordEq {
        hive,
        key,
        name,
        value,
    }
}

const fn eq_sz(hive: Hive, key: &'static str, name: &'static str, value: &'static str) -> Probe {
    Probe::SzEq {
        hive,
        key,
        name,
        value,
    }
}

const UPDATE_AUTO: &[RegOp] = &[
    del(Hive::Lm, AU, "NoAutoUpdate"),
    del(Hive::Lm, AU, "AUOptions"),
];
const UPDATE_ASK: &[RegOp] = &[
    dword(Hive::Lm, AU, "NoAutoUpdate", 0),
    dword(Hive::Lm, AU, "AUOptions", 3),
];
const UPDATE_NOTIFY: &[RegOp] = &[
    dword(Hive::Lm, AU, "NoAutoUpdate", 0),
    dword(Hive::Lm, AU, "AUOptions", 2),
];
const UPDATE_MANUAL: &[RegOp] = &[
    dword(Hive::Lm, AU, "NoAutoUpdate", 1),
    del(Hive::Lm, AU, "AUOptions"),
];
const UPDATE_PLANS: &[&[RegOp]] = &[UPDATE_AUTO, UPDATE_ASK, UPDATE_NOTIFY, UPDATE_MANUAL];
const UPDATE_OPTIONS: &[&str] = &[
    "optimize-update-auto",
    "optimize-update-ask",
    "optimize-update-notify",
    "optimize-update-manual",
];
const UPDATE_DETECT: &[(u8, &[Probe])] = &[
    (3, &[eq_dword(Hive::Lm, AU, "NoAutoUpdate", 1)]),
    (2, &[eq_dword(Hive::Lm, AU, "AUOptions", 2)]),
    (1, &[eq_dword(Hive::Lm, AU, "AUOptions", 3)]),
];

const REBOOT_ON: &[RegOp] = &[
    dword(Hive::Lm, AU, "NoAutoRebootWithLoggedOnUsers", 1),
    dword(Hive::Lm, AU, "AlwaysAutoRebootAtScheduledTime", 0),
];
const REBOOT_OFF: &[RegOp] = &[
    del(Hive::Lm, AU, "NoAutoRebootWithLoggedOnUsers"),
    del(Hive::Lm, AU, "AlwaysAutoRebootAtScheduledTime"),
];
const REBOOT_PLANS: &[&[RegOp]] = &[REBOOT_OFF, REBOOT_ON];
const REBOOT_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(Hive::Lm, AU, "NoAutoRebootWithLoggedOnUsers", 1)],
)];

const HOURS_OFF: &[RegOp] = &[
    del(Hive::Lm, WU, "SetActiveHours"),
    del(Hive::Lm, WU, "ActiveHoursStart"),
    del(Hive::Lm, WU, "ActiveHoursEnd"),
];
const HOURS_DAY: &[RegOp] = &[
    dword(Hive::Lm, WU, "SetActiveHours", 1),
    dword(Hive::Lm, WU, "ActiveHoursStart", 8),
    dword(Hive::Lm, WU, "ActiveHoursEnd", 17),
];
const HOURS_EVENING: &[RegOp] = &[
    dword(Hive::Lm, WU, "SetActiveHours", 1),
    dword(Hive::Lm, WU, "ActiveHoursStart", 9),
    dword(Hive::Lm, WU, "ActiveHoursEnd", 21),
];
const HOURS_LATE: &[RegOp] = &[
    dword(Hive::Lm, WU, "SetActiveHours", 1),
    dword(Hive::Lm, WU, "ActiveHoursStart", 8),
    dword(Hive::Lm, WU, "ActiveHoursEnd", 23),
];
const HOURS_PLANS: &[&[RegOp]] = &[HOURS_OFF, HOURS_DAY, HOURS_EVENING, HOURS_LATE];
const HOURS_OPTIONS: &[&str] = &[
    "optimize-hours-default",
    "optimize-hours-day",
    "optimize-hours-evening",
    "optimize-hours-late",
];
const HOURS_DETECT: &[(u8, &[Probe])] = &[
    (
        1,
        &[
            eq_dword(Hive::Lm, WU, "SetActiveHours", 1),
            eq_dword(Hive::Lm, WU, "ActiveHoursStart", 8),
            eq_dword(Hive::Lm, WU, "ActiveHoursEnd", 17),
        ],
    ),
    (
        2,
        &[
            eq_dword(Hive::Lm, WU, "SetActiveHours", 1),
            eq_dword(Hive::Lm, WU, "ActiveHoursStart", 9),
            eq_dword(Hive::Lm, WU, "ActiveHoursEnd", 21),
        ],
    ),
    (
        3,
        &[
            eq_dword(Hive::Lm, WU, "SetActiveHours", 1),
            eq_dword(Hive::Lm, WU, "ActiveHoursStart", 8),
            eq_dword(Hive::Lm, WU, "ActiveHoursEnd", 23),
        ],
    ),
];

const DELIVERY_OFF: &[RegOp] = &[del(Hive::Lm, DELIVERY, "DODownloadMode")];
const DELIVERY_ON: &[RegOp] = &[dword(Hive::Lm, DELIVERY, "DODownloadMode", 0)];
const DELIVERY_PLANS: &[&[RegOp]] = &[DELIVERY_OFF, DELIVERY_ON];
const DELIVERY_DETECT: &[(u8, &[Probe])] =
    &[(1, &[eq_dword(Hive::Lm, DELIVERY, "DODownloadMode", 0)])];

const DRIVERS_OFF: &[RegOp] = &[del(Hive::Lm, WU, "ExcludeWUDriversInQualityUpdate")];
const DRIVERS_ON: &[RegOp] = &[dword(Hive::Lm, WU, "ExcludeWUDriversInQualityUpdate", 1)];
const DRIVERS_PLANS: &[&[RegOp]] = &[DRIVERS_OFF, DRIVERS_ON];
const DRIVERS_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(Hive::Lm, WU, "ExcludeWUDriversInQualityUpdate", 1)],
)];

const ADS_OFF: &[RegOp] = &[dword(Hive::Cu, ADS, "Enabled", 1)];
const ADS_ON: &[RegOp] = &[dword(Hive::Cu, ADS, "Enabled", 0)];
const ADS_PLANS: &[&[RegOp]] = &[ADS_OFF, ADS_ON];
const ADS_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, ADS, "Enabled", 0)])];

const TAILORED_OFF: &[RegOp] = &[dword(
    Hive::Cu,
    PRIVACY,
    "TailoredExperiencesWithDiagnosticDataEnabled",
    1,
)];
const TAILORED_ON: &[RegOp] = &[dword(
    Hive::Cu,
    PRIVACY,
    "TailoredExperiencesWithDiagnosticDataEnabled",
    0,
)];
const TAILORED_PLANS: &[&[RegOp]] = &[TAILORED_OFF, TAILORED_ON];
const TAILORED_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(
        Hive::Cu,
        PRIVACY,
        "TailoredExperiencesWithDiagnosticDataEnabled",
        0,
    )],
)];

const TELEMETRY_OFF: &[RegOp] = &[del(Hive::Lm, DATA, "AllowTelemetry")];
const TELEMETRY_ON: &[RegOp] = &[dword(Hive::Lm, DATA, "AllowTelemetry", 1)];
const TELEMETRY_PLANS: &[&[RegOp]] = &[TELEMETRY_OFF, TELEMETRY_ON];
const TELEMETRY_DETECT: &[(u8, &[Probe])] =
    &[(1, &[eq_dword(Hive::Lm, DATA, "AllowTelemetry", 1)])];

const ACTIVITY_OFF: &[RegOp] = &[
    del(Hive::Lm, POLICY_SYSTEM, "EnableActivityFeed"),
    del(Hive::Lm, POLICY_SYSTEM, "PublishUserActivities"),
    del(Hive::Lm, POLICY_SYSTEM, "UploadUserActivities"),
];
const ACTIVITY_ON: &[RegOp] = &[
    dword(Hive::Lm, POLICY_SYSTEM, "EnableActivityFeed", 0),
    dword(Hive::Lm, POLICY_SYSTEM, "PublishUserActivities", 0),
    dword(Hive::Lm, POLICY_SYSTEM, "UploadUserActivities", 0),
];
const ACTIVITY_PLANS: &[&[RegOp]] = &[ACTIVITY_OFF, ACTIVITY_ON];
const ACTIVITY_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(
        Hive::Lm,
        POLICY_SYSTEM,
        "PublishUserActivities",
        0,
    )],
)];

const FEEDBACK_OFF: &[RegOp] = &[del(Hive::Cu, SIUF, "NumberOfSIUFInPeriod")];
const FEEDBACK_ON: &[RegOp] = &[dword(Hive::Cu, SIUF, "NumberOfSIUFInPeriod", 0)];
const FEEDBACK_PLANS: &[&[RegOp]] = &[FEEDBACK_OFF, FEEDBACK_ON];
const FEEDBACK_DETECT: &[(u8, &[Probe])] =
    &[(1, &[eq_dword(Hive::Cu, SIUF, "NumberOfSIUFInPeriod", 0)])];

const SPEECH_OFF: &[RegOp] = &[dword(Hive::Cu, SPEECH, "HasAccepted", 1)];
const SPEECH_ON: &[RegOp] = &[dword(Hive::Cu, SPEECH, "HasAccepted", 0)];
const SPEECH_PLANS: &[&[RegOp]] = &[SPEECH_OFF, SPEECH_ON];
const SPEECH_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, SPEECH, "HasAccepted", 0)])];

const TYPING_OFF: &[RegOp] = &[
    dword(Hive::Cu, INK, "RestrictImplicitInkCollection", 0),
    dword(Hive::Cu, INK, "RestrictImplicitTextCollection", 0),
    dword(Hive::Cu, INK_STORE, "HarvestContacts", 1),
    dword(Hive::Cu, PERSONAL, "AcceptedPrivacyPolicy", 1),
];
const TYPING_ON: &[RegOp] = &[
    dword(Hive::Cu, INK, "RestrictImplicitInkCollection", 1),
    dword(Hive::Cu, INK, "RestrictImplicitTextCollection", 1),
    dword(Hive::Cu, INK_STORE, "HarvestContacts", 0),
    dword(Hive::Cu, PERSONAL, "AcceptedPrivacyPolicy", 0),
];
const TYPING_PLANS: &[&[RegOp]] = &[TYPING_OFF, TYPING_ON];
const TYPING_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(Hive::Cu, INK, "RestrictImplicitTextCollection", 1)],
)];

const EXT_OFF: &[RegOp] = &[dword(Hive::Cu, ADV, "HideFileExt", 1)];
const EXT_ON: &[RegOp] = &[dword(Hive::Cu, ADV, "HideFileExt", 0)];
const EXT_PLANS: &[&[RegOp]] = &[EXT_OFF, EXT_ON];
const EXT_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, ADV, "HideFileExt", 0)])];

const HIDDEN_OFF: &[RegOp] = &[dword(Hive::Cu, ADV, "Hidden", 2)];
const HIDDEN_ON: &[RegOp] = &[dword(Hive::Cu, ADV, "Hidden", 1)];
const HIDDEN_PLANS: &[&[RegOp]] = &[HIDDEN_OFF, HIDDEN_ON];
const HIDDEN_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, ADV, "Hidden", 1)])];

const THIS_PC_OFF: &[RegOp] = &[dword(Hive::Cu, ADV, "LaunchTo", 2)];
const THIS_PC_ON: &[RegOp] = &[dword(Hive::Cu, ADV, "LaunchTo", 1)];
const THIS_PC_PLANS: &[&[RegOp]] = &[THIS_PC_OFF, THIS_PC_ON];
const THIS_PC_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, ADV, "LaunchTo", 1)])];

const RECENT_OFF: &[RegOp] = &[
    dword(Hive::Cu, ADV, "ShowRecent", 1),
    dword(Hive::Cu, ADV, "ShowFrequent", 1),
];
const RECENT_ON: &[RegOp] = &[
    dword(Hive::Cu, ADV, "ShowRecent", 0),
    dword(Hive::Cu, ADV, "ShowFrequent", 0),
];
const RECENT_PLANS: &[&[RegOp]] = &[RECENT_OFF, RECENT_ON];
const RECENT_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, ADV, "ShowRecent", 0)])];

const PATH_OFF: &[RegOp] = &[dword(Hive::Cu, CABINET, "FullPath", 0)];
const PATH_ON: &[RegOp] = &[dword(Hive::Cu, CABINET, "FullPath", 1)];
const PATH_PLANS: &[&[RegOp]] = &[PATH_OFF, PATH_ON];
const PATH_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, CABINET, "FullPath", 1)])];

const MENU_OFF: &[RegOp] = &[RegOp::DeleteKey {
    hive: Hive::Cu,
    key: CLASSIC,
}];
const MENU_ON: &[RegOp] = &[RegOp::EmptyDefault {
    hive: Hive::Cu,
    key: CLASSIC_INPROC,
}];
const MENU_PLANS: &[&[RegOp]] = &[MENU_OFF, MENU_ON];
const MENU_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[Probe::KeyExists {
        hive: Hive::Cu,
        key: CLASSIC_INPROC,
    }],
)];

const NAV_OFF: &[RegOp] = &[dword(Hive::Cu, ADV, "NavPaneExpandToCurrentFolder", 0)];
const NAV_ON: &[RegOp] = &[dword(Hive::Cu, ADV, "NavPaneExpandToCurrentFolder", 1)];
const NAV_PLANS: &[&[RegOp]] = &[NAV_OFF, NAV_ON];
const NAV_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(Hive::Cu, ADV, "NavPaneExpandToCurrentFolder", 1)],
)];

const EXPLORER_ADS_OFF: &[RegOp] = &[dword(Hive::Cu, ADV, "ShowSyncProviderNotifications", 1)];
const EXPLORER_ADS_ON: &[RegOp] = &[dword(Hive::Cu, ADV, "ShowSyncProviderNotifications", 0)];
const EXPLORER_ADS_PLANS: &[&[RegOp]] = &[EXPLORER_ADS_OFF, EXPLORER_ADS_ON];
const EXPLORER_ADS_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(Hive::Cu, ADV, "ShowSyncProviderNotifications", 0)],
)];

const LEFT_OFF: &[RegOp] = &[dword(Hive::Cu, ADV, "TaskbarAl", 1)];
const LEFT_ON: &[RegOp] = &[dword(Hive::Cu, ADV, "TaskbarAl", 0)];
const LEFT_PLANS: &[&[RegOp]] = &[LEFT_OFF, LEFT_ON];
const LEFT_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, ADV, "TaskbarAl", 0)])];

const SEARCH_HIDDEN: &[RegOp] = &[dword(Hive::Cu, ADV, "SearchboxTaskbarMode", 0)];
const SEARCH_ICON: &[RegOp] = &[dword(Hive::Cu, ADV, "SearchboxTaskbarMode", 1)];
const SEARCH_BOX: &[RegOp] = &[dword(Hive::Cu, ADV, "SearchboxTaskbarMode", 2)];
const SEARCH_WIDE: &[RegOp] = &[dword(Hive::Cu, ADV, "SearchboxTaskbarMode", 3)];
const SEARCH_PLANS: &[&[RegOp]] = &[SEARCH_HIDDEN, SEARCH_ICON, SEARCH_BOX, SEARCH_WIDE];
const SEARCH_OPTIONS: &[&str] = &[
    "optimize-search-hidden",
    "optimize-search-icon",
    "optimize-search-box",
    "optimize-search-wide",
];
const SEARCH_DETECT: &[(u8, &[Probe])] = &[
    (0, &[eq_dword(Hive::Cu, ADV, "SearchboxTaskbarMode", 0)]),
    (1, &[eq_dword(Hive::Cu, ADV, "SearchboxTaskbarMode", 1)]),
    (2, &[eq_dword(Hive::Cu, ADV, "SearchboxTaskbarMode", 2)]),
    (3, &[eq_dword(Hive::Cu, ADV, "SearchboxTaskbarMode", 3)]),
];

const WIDGETS_OFF: &[RegOp] = &[dword(Hive::Cu, ADV, "TaskbarDa", 1)];
const WIDGETS_ON: &[RegOp] = &[dword(Hive::Cu, ADV, "TaskbarDa", 0)];
const WIDGETS_PLANS: &[&[RegOp]] = &[WIDGETS_OFF, WIDGETS_ON];
const WIDGETS_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_dword(Hive::Cu, ADV, "TaskbarDa", 0)])];

const COPILOT_OFF: &[RegOp] = &[dword(Hive::Cu, ADV, "ShowCopilotButton", 1)];
const COPILOT_ON: &[RegOp] = &[dword(Hive::Cu, ADV, "ShowCopilotButton", 0)];
const COPILOT_PLANS: &[&[RegOp]] = &[COPILOT_OFF, COPILOT_ON];
const COPILOT_DETECT: &[(u8, &[Probe])] =
    &[(1, &[eq_dword(Hive::Cu, ADV, "ShowCopilotButton", 0)])];

const COMBINE_ALWAYS: &[RegOp] = &[dword(Hive::Cu, ADV, "TaskbarGlomLevel", 0)];
const COMBINE_FULL: &[RegOp] = &[dword(Hive::Cu, ADV, "TaskbarGlomLevel", 1)];
const COMBINE_NEVER: &[RegOp] = &[dword(Hive::Cu, ADV, "TaskbarGlomLevel", 2)];
const COMBINE_PLANS: &[&[RegOp]] = &[COMBINE_ALWAYS, COMBINE_FULL, COMBINE_NEVER];
const COMBINE_OPTIONS: &[&str] = &[
    "optimize-combine-always",
    "optimize-combine-full",
    "optimize-combine-never",
];
const COMBINE_DETECT: &[(u8, &[Probe])] = &[
    (0, &[eq_dword(Hive::Cu, ADV, "TaskbarGlomLevel", 0)]),
    (1, &[eq_dword(Hive::Cu, ADV, "TaskbarGlomLevel", 1)]),
    (2, &[eq_dword(Hive::Cu, ADV, "TaskbarGlomLevel", 2)]),
];

const HIGHLIGHTS_OFF: &[RegOp] = &[dword(
    Hive::Cu,
    SEARCH_SETTINGS,
    "IsDynamicSearchBoxEnabled",
    1,
)];
const HIGHLIGHTS_ON: &[RegOp] = &[dword(
    Hive::Cu,
    SEARCH_SETTINGS,
    "IsDynamicSearchBoxEnabled",
    0,
)];
const HIGHLIGHTS_PLANS: &[&[RegOp]] = &[HIGHLIGHTS_OFF, HIGHLIGHTS_ON];
const HIGHLIGHTS_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(
        Hive::Cu,
        SEARCH_SETTINGS,
        "IsDynamicSearchBoxEnabled",
        0,
    )],
)];

const BING_OFF: &[RegOp] = &[del(Hive::Cu, SEARCH_POLICY, "DisableSearchBoxSuggestions")];
const BING_ON: &[RegOp] = &[dword(
    Hive::Cu,
    SEARCH_POLICY,
    "DisableSearchBoxSuggestions",
    1,
)];
const BING_PLANS: &[&[RegOp]] = &[BING_OFF, BING_ON];
const BING_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(
        Hive::Cu,
        SEARCH_POLICY,
        "DisableSearchBoxSuggestions",
        1,
    )],
)];

const LOCK_OFF: &[RegOp] = &[
    dword(Hive::Cu, CONTENT, "RotatingLockScreenOverlayEnabled", 1),
    dword(Hive::Cu, CONTENT, "RotatingLockScreenEnabled", 1),
    dword(Hive::Cu, CONTENT, "SubscribedContent-338387Enabled", 1),
];
const LOCK_ON: &[RegOp] = &[
    dword(Hive::Cu, CONTENT, "RotatingLockScreenOverlayEnabled", 0),
    dword(Hive::Cu, CONTENT, "RotatingLockScreenEnabled", 0),
    dword(Hive::Cu, CONTENT, "SubscribedContent-338387Enabled", 0),
];
const LOCK_PLANS: &[&[RegOp]] = &[LOCK_OFF, LOCK_ON];
const LOCK_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(
        Hive::Cu,
        CONTENT,
        "SubscribedContent-338387Enabled",
        0,
    )],
)];

const START_OFF: &[RegOp] = &[
    dword(Hive::Cu, CONTENT, "SubscribedContent-338388Enabled", 1),
    dword(Hive::Cu, CONTENT, "SystemPaneSuggestionsEnabled", 1),
    dword(Hive::Cu, CONTENT, "SilentInstalledAppsEnabled", 1),
];
const START_ON: &[RegOp] = &[
    dword(Hive::Cu, CONTENT, "SubscribedContent-338388Enabled", 0),
    dword(Hive::Cu, CONTENT, "SystemPaneSuggestionsEnabled", 0),
    dword(Hive::Cu, CONTENT, "SilentInstalledAppsEnabled", 0),
];
const START_PLANS: &[&[RegOp]] = &[START_OFF, START_ON];
const START_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(Hive::Cu, CONTENT, "SilentInstalledAppsEnabled", 0)],
)];

const SETTINGS_OFF: &[RegOp] = &[
    dword(Hive::Cu, CONTENT, "SubscribedContent-338393Enabled", 1),
    dword(Hive::Cu, CONTENT, "SubscribedContent-353694Enabled", 1),
    dword(Hive::Cu, CONTENT, "SubscribedContent-353696Enabled", 1),
];
const SETTINGS_ON: &[RegOp] = &[
    dword(Hive::Cu, CONTENT, "SubscribedContent-338393Enabled", 0),
    dword(Hive::Cu, CONTENT, "SubscribedContent-353694Enabled", 0),
    dword(Hive::Cu, CONTENT, "SubscribedContent-353696Enabled", 0),
];
const SETTINGS_PLANS: &[&[RegOp]] = &[SETTINGS_OFF, SETTINGS_ON];
const SETTINGS_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(
        Hive::Cu,
        CONTENT,
        "SubscribedContent-338393Enabled",
        0,
    )],
)];

const WELCOME_OFF: &[RegOp] = &[
    dword(Hive::Cu, CONTENT, "SubscribedContent-310093Enabled", 1),
    dword(Hive::Cu, CONTENT, "SubscribedContent-338389Enabled", 1),
    dword(Hive::Cu, CONTENT, "SoftLandingEnabled", 1),
    dword(Hive::Cu, SCOOBE, "ScoobeSystemSettingEnabled", 1),
];
const WELCOME_ON: &[RegOp] = &[
    dword(Hive::Cu, CONTENT, "SubscribedContent-310093Enabled", 0),
    dword(Hive::Cu, CONTENT, "SubscribedContent-338389Enabled", 0),
    dword(Hive::Cu, CONTENT, "SoftLandingEnabled", 0),
    dword(Hive::Cu, SCOOBE, "ScoobeSystemSettingEnabled", 0),
];
const WELCOME_PLANS: &[&[RegOp]] = &[WELCOME_OFF, WELCOME_ON];
const WELCOME_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(
        Hive::Cu,
        CONTENT,
        "SubscribedContent-310093Enabled",
        0,
    )],
)];

const CONSUMER_OFF: &[RegOp] = &[
    del(Hive::Lm, CLOUD, "DisableWindowsConsumerFeatures"),
    del(Hive::Lm, CLOUD, "DisableSoftLanding"),
];
const CONSUMER_ON: &[RegOp] = &[
    dword(Hive::Lm, CLOUD, "DisableWindowsConsumerFeatures", 1),
    dword(Hive::Lm, CLOUD, "DisableSoftLanding", 1),
];
const CONSUMER_PLANS: &[&[RegOp]] = &[CONSUMER_OFF, CONSUMER_ON];
const CONSUMER_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(
        Hive::Lm,
        CLOUD,
        "DisableWindowsConsumerFeatures",
        1,
    )],
)];

const ANIM_OFF: &[RegOp] = &[
    dword(Hive::Cu, ADV, "TaskbarAnimations", 1),
    sz(Hive::Cu, DESKTOP, "MinAnimate", "1"),
];
const ANIM_ON: &[RegOp] = &[
    dword(Hive::Cu, ADV, "TaskbarAnimations", 0),
    sz(Hive::Cu, DESKTOP, "MinAnimate", "0"),
];
const ANIM_PLANS: &[&[RegOp]] = &[ANIM_OFF, ANIM_ON];
const ANIM_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_sz(Hive::Cu, DESKTOP, "MinAnimate", "0")])];

const TRANS_OFF: &[RegOp] = &[dword(Hive::Cu, THEME, "EnableTransparency", 1)];
const TRANS_ON: &[RegOp] = &[dword(Hive::Cu, THEME, "EnableTransparency", 0)];
const TRANS_PLANS: &[&[RegOp]] = &[TRANS_OFF, TRANS_ON];
const TRANS_DETECT: &[(u8, &[Probe])] =
    &[(1, &[eq_dword(Hive::Cu, THEME, "EnableTransparency", 0)])];

const DELAY_OFF: &[RegOp] = &[sz(Hive::Cu, DESKTOP, "MenuShowDelay", "400")];
const DELAY_ON: &[RegOp] = &[sz(Hive::Cu, DESKTOP, "MenuShowDelay", "0")];
const DELAY_PLANS: &[&[RegOp]] = &[DELAY_OFF, DELAY_ON];
const DELAY_DETECT: &[(u8, &[Probe])] = &[(1, &[eq_sz(Hive::Cu, DESKTOP, "MenuShowDelay", "0")])];

const BG_OFF: &[RegOp] = &[
    dword(Hive::Cu, BACKGROUND, "GlobalUserDisabled", 0),
    dword(Hive::Cu, SEARCH, "BackgroundAppGlobalToggle", 1),
];
const BG_ON: &[RegOp] = &[
    dword(Hive::Cu, BACKGROUND, "GlobalUserDisabled", 1),
    dword(Hive::Cu, SEARCH, "BackgroundAppGlobalToggle", 0),
];
const BG_PLANS: &[&[RegOp]] = &[BG_OFF, BG_ON];
const BG_DETECT: &[(u8, &[Probe])] = &[(
    1,
    &[eq_dword(Hive::Cu, BACKGROUND, "GlobalUserDisabled", 1)],
)];

const STARTUP_OFF: &[RegOp] = &[del(Hive::Cu, SERIALIZE, "StartupDelayInMSec")];
const STARTUP_ON: &[RegOp] = &[dword(Hive::Cu, SERIALIZE, "StartupDelayInMSec", 0)];
const STARTUP_PLANS: &[&[RegOp]] = &[STARTUP_OFF, STARTUP_ON];
const STARTUP_DETECT: &[(u8, &[Probe])] =
    &[(1, &[eq_dword(Hive::Cu, SERIALIZE, "StartupDelayInMSec", 0)])];

const TWEAKS: &[TweakDef] = &[
    TweakDef {
        id: "update-mode",
        tab: TAB_UPDATES,
        title_key: "optimize-update-mode",
        detail_key: "optimize-update-mode-detail",
        option_keys: UPDATE_OPTIONS,
        plans: UPDATE_PLANS,
        detect: UPDATE_DETECT,
        fallback: 0,
        needs_admin: true,
        restarts_explorer: false,
    },
    TweakDef {
        id: "no-reboot",
        tab: TAB_UPDATES,
        title_key: "optimize-no-reboot",
        detail_key: "optimize-no-reboot-detail",
        option_keys: &[],
        plans: REBOOT_PLANS,
        detect: REBOOT_DETECT,
        fallback: 0,
        needs_admin: true,
        restarts_explorer: false,
    },
    TweakDef {
        id: "active-hours",
        tab: TAB_UPDATES,
        title_key: "optimize-active-hours",
        detail_key: "optimize-active-hours-detail",
        option_keys: HOURS_OPTIONS,
        plans: HOURS_PLANS,
        detect: HOURS_DETECT,
        fallback: 0,
        needs_admin: true,
        restarts_explorer: false,
    },
    TweakDef {
        id: "delivery",
        tab: TAB_UPDATES,
        title_key: "optimize-delivery",
        detail_key: "optimize-delivery-detail",
        option_keys: &[],
        plans: DELIVERY_PLANS,
        detect: DELIVERY_DETECT,
        fallback: 0,
        needs_admin: true,
        restarts_explorer: false,
    },
    TweakDef {
        id: "drivers",
        tab: TAB_UPDATES,
        title_key: "optimize-drivers",
        detail_key: "optimize-drivers-detail",
        option_keys: &[],
        plans: DRIVERS_PLANS,
        detect: DRIVERS_DETECT,
        fallback: 0,
        needs_admin: true,
        restarts_explorer: false,
    },
    TweakDef {
        id: "ads-id",
        tab: TAB_PRIVACY,
        title_key: "optimize-ads-id",
        detail_key: "optimize-ads-id-detail",
        option_keys: &[],
        plans: ADS_PLANS,
        detect: ADS_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "tailored",
        tab: TAB_PRIVACY,
        title_key: "optimize-tailored",
        detail_key: "optimize-tailored-detail",
        option_keys: &[],
        plans: TAILORED_PLANS,
        detect: TAILORED_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "telemetry",
        tab: TAB_PRIVACY,
        title_key: "optimize-telemetry",
        detail_key: "optimize-telemetry-detail",
        option_keys: &[],
        plans: TELEMETRY_PLANS,
        detect: TELEMETRY_DETECT,
        fallback: 0,
        needs_admin: true,
        restarts_explorer: false,
    },
    TweakDef {
        id: "activity",
        tab: TAB_PRIVACY,
        title_key: "optimize-activity",
        detail_key: "optimize-activity-detail",
        option_keys: &[],
        plans: ACTIVITY_PLANS,
        detect: ACTIVITY_DETECT,
        fallback: 0,
        needs_admin: true,
        restarts_explorer: false,
    },
    TweakDef {
        id: "feedback",
        tab: TAB_PRIVACY,
        title_key: "optimize-feedback",
        detail_key: "optimize-feedback-detail",
        option_keys: &[],
        plans: FEEDBACK_PLANS,
        detect: FEEDBACK_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "speech",
        tab: TAB_PRIVACY,
        title_key: "optimize-speech",
        detail_key: "optimize-speech-detail",
        option_keys: &[],
        plans: SPEECH_PLANS,
        detect: SPEECH_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "typing",
        tab: TAB_PRIVACY,
        title_key: "optimize-typing",
        detail_key: "optimize-typing-detail",
        option_keys: &[],
        plans: TYPING_PLANS,
        detect: TYPING_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "extensions",
        tab: TAB_EXPLORER,
        title_key: "optimize-extensions",
        detail_key: "optimize-extensions-detail",
        option_keys: &[],
        plans: EXT_PLANS,
        detect: EXT_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "hidden",
        tab: TAB_EXPLORER,
        title_key: "optimize-hidden",
        detail_key: "optimize-hidden-detail",
        option_keys: &[],
        plans: HIDDEN_PLANS,
        detect: HIDDEN_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "this-pc",
        tab: TAB_EXPLORER,
        title_key: "optimize-this-pc",
        detail_key: "optimize-this-pc-detail",
        option_keys: &[],
        plans: THIS_PC_PLANS,
        detect: THIS_PC_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "recent",
        tab: TAB_EXPLORER,
        title_key: "optimize-recent",
        detail_key: "optimize-recent-detail",
        option_keys: &[],
        plans: RECENT_PLANS,
        detect: RECENT_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "full-path",
        tab: TAB_EXPLORER,
        title_key: "optimize-full-path",
        detail_key: "optimize-full-path-detail",
        option_keys: &[],
        plans: PATH_PLANS,
        detect: PATH_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "classic-menu",
        tab: TAB_EXPLORER,
        title_key: "optimize-classic-menu",
        detail_key: "optimize-classic-menu-detail",
        option_keys: &[],
        plans: MENU_PLANS,
        detect: MENU_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "nav-expand",
        tab: TAB_EXPLORER,
        title_key: "optimize-nav-expand",
        detail_key: "optimize-nav-expand-detail",
        option_keys: &[],
        plans: NAV_PLANS,
        detect: NAV_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "explorer-ads",
        tab: TAB_EXPLORER,
        title_key: "optimize-explorer-ads",
        detail_key: "optimize-explorer-ads-detail",
        option_keys: &[],
        plans: EXPLORER_ADS_PLANS,
        detect: EXPLORER_ADS_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "taskbar-left",
        tab: TAB_SHELL,
        title_key: "optimize-taskbar-left",
        detail_key: "optimize-taskbar-left-detail",
        option_keys: &[],
        plans: LEFT_PLANS,
        detect: LEFT_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "search-mode",
        tab: TAB_SHELL,
        title_key: "optimize-search-mode",
        detail_key: "optimize-search-mode-detail",
        option_keys: SEARCH_OPTIONS,
        plans: SEARCH_PLANS,
        detect: SEARCH_DETECT,
        fallback: 1,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "hide-widgets",
        tab: TAB_SHELL,
        title_key: "optimize-hide-widgets",
        detail_key: "optimize-hide-widgets-detail",
        option_keys: &[],
        plans: WIDGETS_PLANS,
        detect: WIDGETS_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "hide-copilot",
        tab: TAB_SHELL,
        title_key: "optimize-hide-copilot",
        detail_key: "optimize-hide-copilot-detail",
        option_keys: &[],
        plans: COPILOT_PLANS,
        detect: COPILOT_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "combine",
        tab: TAB_SHELL,
        title_key: "optimize-combine",
        detail_key: "optimize-combine-detail",
        option_keys: COMBINE_OPTIONS,
        plans: COMBINE_PLANS,
        detect: COMBINE_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: true,
    },
    TweakDef {
        id: "search-highlights",
        tab: TAB_SHELL,
        title_key: "optimize-search-highlights",
        detail_key: "optimize-search-highlights-detail",
        option_keys: &[],
        plans: HIGHLIGHTS_PLANS,
        detect: HIGHLIGHTS_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "bing-search",
        tab: TAB_SHELL,
        title_key: "optimize-bing-search",
        detail_key: "optimize-bing-search-detail",
        option_keys: &[],
        plans: BING_PLANS,
        detect: BING_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "lock-screen",
        tab: TAB_QUIET,
        title_key: "optimize-lock-screen",
        detail_key: "optimize-lock-screen-detail",
        option_keys: &[],
        plans: LOCK_PLANS,
        detect: LOCK_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "start-suggest",
        tab: TAB_QUIET,
        title_key: "optimize-start-suggest",
        detail_key: "optimize-start-suggest-detail",
        option_keys: &[],
        plans: START_PLANS,
        detect: START_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "settings-suggest",
        tab: TAB_QUIET,
        title_key: "optimize-settings-suggest",
        detail_key: "optimize-settings-suggest-detail",
        option_keys: &[],
        plans: SETTINGS_PLANS,
        detect: SETTINGS_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "welcome",
        tab: TAB_QUIET,
        title_key: "optimize-welcome",
        detail_key: "optimize-welcome-detail",
        option_keys: &[],
        plans: WELCOME_PLANS,
        detect: WELCOME_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "consumer",
        tab: TAB_QUIET,
        title_key: "optimize-consumer",
        detail_key: "optimize-consumer-detail",
        option_keys: &[],
        plans: CONSUMER_PLANS,
        detect: CONSUMER_DETECT,
        fallback: 0,
        needs_admin: true,
        restarts_explorer: false,
    },
    TweakDef {
        id: "animations",
        tab: TAB_PERFORMANCE,
        title_key: "optimize-animations",
        detail_key: "optimize-animations-detail",
        option_keys: &[],
        plans: ANIM_PLANS,
        detect: ANIM_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "transparency",
        tab: TAB_PERFORMANCE,
        title_key: "optimize-transparency",
        detail_key: "optimize-transparency-detail",
        option_keys: &[],
        plans: TRANS_PLANS,
        detect: TRANS_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "menu-delay",
        tab: TAB_PERFORMANCE,
        title_key: "optimize-menu-delay",
        detail_key: "optimize-menu-delay-detail",
        option_keys: &[],
        plans: DELAY_PLANS,
        detect: DELAY_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "background-apps",
        tab: TAB_PERFORMANCE,
        title_key: "optimize-background-apps",
        detail_key: "optimize-background-apps-detail",
        option_keys: &[],
        plans: BG_PLANS,
        detect: BG_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
    TweakDef {
        id: "startup-delay",
        tab: TAB_PERFORMANCE,
        title_key: "optimize-startup-delay",
        detail_key: "optimize-startup-delay-detail",
        option_keys: &[],
        plans: STARTUP_PLANS,
        detect: STARTUP_DETECT,
        fallback: 0,
        needs_admin: false,
        restarts_explorer: false,
    },
];

/// Every tweak, in tab order.
#[must_use]
pub fn tweaks() -> &'static [TweakDef] {
    TWEAKS
}

/// Look up a tweak by the id the UI sends.
#[must_use]
pub fn tweak_by_id(id: &str) -> Option<&'static TweakDef> {
    tweaks().iter().find(|tweak| tweak.id == id)
}

/// Which plan matches the probes. Unknown ids yield `None`.
pub fn select_index(def: &TweakDef, mut matches: impl FnMut(&Probe) -> bool) -> u8 {
    for (index, probes) in def.detect {
        if probes.iter().all(|probe| matches(probe)) {
            return *index;
        }
    }
    def.fallback
}

/// Plan for a UI index, clamped to the tweak. `None` when `id` is unknown.
#[must_use]
pub fn plan_for(id: &str, index: i32) -> Option<&'static [RegOp]> {
    let def = tweak_by_id(id)?;
    if def.plans.is_empty() {
        return None;
    }
    let max = def.plans.len() - 1;
    let index = usize::try_from(index).unwrap_or(0).min(max);
    Some(def.plans[index])
}

/// `.reg` text for `ops`. Used for the administrator import and for tests.
#[must_use]
pub fn render_reg(ops: &[RegOp]) -> String {
    let mut out = String::from("Windows Registry Editor Version 5.00\r\n");
    for op in ops {
        match *op {
            RegOp::DeleteKey { hive, key } => {
                out.push_str(&format!("\r\n[-{}\\{key}]\r\n", hive.reg_name()));
            }
            RegOp::EmptyDefault { hive, key } => {
                out.push_str(&format!("\r\n[{}\\{key}]\r\n@=\"\"\r\n", hive.reg_name()));
            }
            RegOp::Dword {
                hive,
                key,
                name,
                value,
            } => {
                out.push_str(&format!(
                    "\r\n[{}\\{key}]\r\n\"{name}\"=dword:{value:08x}\r\n",
                    hive.reg_name()
                ));
            }
            RegOp::Sz {
                hive,
                key,
                name,
                value,
            } => {
                let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                out.push_str(&format!(
                    "\r\n[{}\\{key}]\r\n\"{name}\"=\"{escaped}\"\r\n",
                    hive.reg_name()
                ));
            }
            RegOp::DeleteValue { hive, key, name } => {
                out.push_str(&format!(
                    "\r\n[{}\\{key}]\r\n\"{name}\"=-\r\n",
                    hive.reg_name()
                ));
            }
        }
    }
    out
}

/// Apply `ops` on this machine. On other systems this returns [`ApplyStatus::Unsupported`].
pub fn apply_ops(ops: &[RegOp]) -> ApplyStatus {
    crate::builtin::optimize::platform::apply_ops(ops)
}

/// Read whether `probe` matches the live registry.
#[must_use]
pub fn probe_matches(probe: &Probe) -> bool {
    crate::builtin::optimize::platform::probe_matches(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LM_PREFIXES: &[&str] = &[
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate",
        r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization",
        r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        r"SOFTWARE\Policies\Microsoft\Windows\System",
        r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
    ];

    const CU_PREFIXES: &[&str] = &[
        r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
        r"Software\Microsoft\Windows\CurrentVersion\Privacy",
        r"Software\Microsoft\Siuf\Rules",
        r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy",
        r"Software\Microsoft\InputPersonalization",
        r"Software\Microsoft\Personalization\Settings",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\CabinetState",
        r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}",
        r"Software\Microsoft\Windows\CurrentVersion\SearchSettings",
        r"Software\Policies\Microsoft\Windows\Explorer",
        r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
        r"Software\Microsoft\Windows\CurrentVersion\UserProfileEngagement",
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        r"Control Panel\Desktop",
        r"Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications",
        r"Software\Microsoft\Windows\CurrentVersion\Search",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize",
    ];

    #[test]
    fn ids_are_unique_and_plans_match_the_control() {
        let mut seen = std::collections::BTreeSet::new();
        for tweak in tweaks() {
            assert!(seen.insert(tweak.id), "duplicate {}", tweak.id);
            assert!(tweak.tab < TAB_COUNT, "{}", tweak.id);
            assert!(!tweak.plans.is_empty(), "{}", tweak.id);
            if tweak.option_keys.is_empty() {
                assert_eq!(tweak.plans.len(), 2, "{}", tweak.id);
            } else {
                assert_eq!(tweak.plans.len(), tweak.option_keys.len(), "{}", tweak.id);
            }
            for (index, _) in tweak.detect {
                assert!(
                    (*index as usize) < tweak.plans.len(),
                    "{} detect {index}",
                    tweak.id
                );
            }
            assert!(
                (tweak.fallback as usize) < tweak.plans.len(),
                "{}",
                tweak.id
            );
            let mut hives = std::collections::BTreeSet::new();
            for plan in tweak.plans {
                for op in *plan {
                    hives.insert(op.hive());
                    let prefixes = match op.hive() {
                        Hive::Lm => LM_PREFIXES,
                        Hive::Cu => CU_PREFIXES,
                    };
                    assert!(
                        prefixes.iter().any(|prefix| op.key().starts_with(prefix)),
                        "{} writes an unexpected key {}",
                        tweak.id,
                        op.key()
                    );
                }
            }
            assert!(hives.len() <= 1, "{} mixes hives", tweak.id);
            let machine = hives.contains(&Hive::Lm);
            assert_eq!(tweak.needs_admin, machine, "{}", tweak.id);
        }
    }

    #[test]
    fn manual_updates_do_not_stop_the_service() {
        let text = render_reg(plan_for("update-mode", 3).unwrap());
        assert!(text.contains("\"NoAutoUpdate\"=dword:00000001"));
        let lower = text.to_ascii_lowercase();
        for banned in [
            "wuauserv",
            "defender",
            "firewall",
            "smartscreen",
            "disableantispyware",
            "consentpromptbehavioradmin",
        ] {
            assert!(!lower.contains(banned), "{banned}");
        }
    }

    #[test]
    fn no_reboot_policy_is_the_logged_on_user_value() {
        let text = render_reg(plan_for("no-reboot", 1).unwrap());
        assert!(text.contains("NoAutoRebootWithLoggedOnUsers\"=dword:00000001"));
        assert!(text.contains("AlwaysAutoRebootAtScheduledTime\"=dword:00000000"));
        let off = render_reg(plan_for("no-reboot", 0).unwrap());
        assert!(off.contains("\"NoAutoRebootWithLoggedOnUsers\"=-"));
    }

    #[test]
    fn unknown_id_is_ignored_and_choice_is_clamped() {
        assert!(plan_for("not-a-tweak", 1).is_none());
        let manual = plan_for("update-mode", 99).unwrap();
        assert_eq!(manual, plan_for("update-mode", 3).unwrap());
    }

    #[test]
    fn select_index_uses_the_first_match() {
        let update = tweak_by_id("update-mode").unwrap();
        let manual = select_index(update, |probe| {
            matches!(
                probe,
                Probe::DwordEq {
                    name: "NoAutoUpdate",
                    value: 1,
                    ..
                }
            )
        });
        assert_eq!(manual, 3);
        let untouched = select_index(update, |_| false);
        assert_eq!(untouched, 0);
    }
}
