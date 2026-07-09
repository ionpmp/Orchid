# Orchid English (en-US) message catalog.
#
# Consumed by the upcoming `orchid-i18n::LocaleManager`. Until that lands,
# the built-in widgets fall back to the English strings baked into their
# Rust code — the keys below mirror those defaults and will become the
# single source of truth once the bundle loader is wired up.

# ---- Widget registry metadata ----
widget-terminal-name = Terminal
widget-terminal-desc = Local, WSL, or SSH shells with proper PTY, ANSI colours, and scrollback

widget-weather-name = Weather
widget-weather-desc = Current conditions and 3-day forecast

widget-moon-name = Moon
widget-moon-desc = Current lunar phase, rise/set times, and celestial data

widget-system-name = System
widget-system-desc = CPU, memory, disk, network, and battery indicators

widget-rss-name = News Feed
widget-rss-desc = RSS and Atom news feeds

widget-recent-files-name = Recent Files
widget-recent-files-desc = Recently opened files across Orchid

widget-search-name = Universal Search
widget-search-desc = Search files, run commands, open settings

widget-media-name = Media Player
widget-media-desc = Now playing with transport controls

widget-password-name = Passwords
widget-password-desc = Access your password database

widget-viewer-name = Viewer
widget-viewer-desc = View images, documents, source files, and archives

# ---- Weather ----
weather-condition-clear = Clear
weather-condition-partly-cloudy = Partly cloudy
weather-condition-cloudy = Cloudy
weather-condition-overcast = Overcast
weather-condition-fog = Fog
weather-condition-drizzle = Drizzle
weather-condition-rain = Rain
weather-condition-snow = Snow
weather-condition-sleet = Sleet
weather-condition-thunderstorm = Thunderstorm
weather-condition-hail = Hail
weather-condition-windy = Windy
weather-condition-unknown = Unknown
weather-day-today = Today
weather-day-tomorrow = Tomorrow
weather-status-fresh = Up to date
weather-status-stale = Data may be out of date
weather-status-offline = Offline
weather-status-error = Error loading weather
weather-updated-just-now = Updated just now
weather-updated-minutes = Updated { $m }m ago
weather-updated-hours = Updated { $h }h ago
weather-updated-days = Updated { $d }d ago

# ---- Relative time (shared) ----
relative-just-now = just now
relative-minutes = { $m }m ago
relative-hours = { $h }h ago
relative-days = { $d }d ago

weather-loading = Loading weather…
weather-feels-like = Feels like { $temp }
weather-humidity-label = Humidity
weather-wind-label = Wind
weather-humidity-line = { $label } { $h }%
weather-wind-line = { $label } { $speed } km/h { $dir }
weather-wind-line-no-dir = { $label } { $speed } km/h

# ---- Moon ----
moon-phase-new = New Moon
moon-phase-waxing-crescent = Waxing Crescent
moon-phase-first-quarter = First Quarter
moon-phase-waxing-gibbous = Waxing Gibbous
moon-phase-full = Full Moon
moon-phase-waning-gibbous = Waning Gibbous
moon-phase-last-quarter = Last Quarter
moon-phase-waning-crescent = Waning Crescent
moon-illumination = { $pct }% illuminated
moon-age = Age: { $days } days
moon-distance = Distance: { $km } km
moon-next-full = Next full: { $date }
moon-next-new = Next new: { $date }
moon-moonrise = Moonrise: { $time }
moon-moonset = Moonset: { $time }
moon-sunrise = Sunrise: { $time }
moon-sunset = Sunset: { $time }
moon-libration = Libration: { $lat }°, { $lon }°
moon-loading = Calculating moon data…

# ---- System ----
system-cpu-label = CPU
system-memory-label = Memory
system-disk-label = Disk { $mount }
system-network-label = Network { $name }
system-battery-label = Battery
system-uptime-label = Uptime
system-battery-charging = Charging
system-battery-time-remaining = { $time } remaining
system-network-rate = ↑ { $up }/s  ↓ { $down }/s
system-loading = Loading system metrics…

# ---- RSS ----
rss-no-feeds = No feeds configured
rss-loading = Loading news…
rss-fetch-failed = Could not load feeds. Check your connection and try again.
rss-empty = No items in the configured feeds yet.
recent-files-empty = No recent files yet. Open files in the viewer or file manager to see them here.
rss-error-summary = { $n } of { $total } feeds failed to update
rss-item-published-minutes = { $m }m ago
rss-item-published-hours = { $h }h ago
rss-item-published-days = { $d }d ago

# ---- Universal Search ----
search-placeholder = Type to search files, commands, settings...
search-empty-state = Start typing to search
search-no-results = No results for "{ $query }"
search-no-results-short = No results
search-searching = Searching...
search-source-files = Files
search-source-commands = Commands
search-source-settings = Settings

# ---- Command palette ----
command-palette-placeholder = Run a command...
command-palette-empty = All commands

# ---- Registered commands ----
command.widget.create.name = Create widget
command.widget.create.desc = Add a new widget to the workspace
command.widget.create.arg.type = Widget type id (e.g. terminal, weather)

command.widget.close.name = Close widget
command.widget.close.desc = Close a widget instance

command.widget.move.name = Move widget
command.widget.resize.name = Resize widget
command.widget.focus_next.name = Focus next widget
command.widget.show_all.name = Show all widgets
command.widget.group.dissolve.name = Dissolve widget group

command.workspace.create.name = Create workspace
command.workspace.delete.name = Delete workspace
command.workspace.switch_to.name = Switch to workspace
command.workspace.switch_next.name = Next workspace
command.workspace.switch_previous.name = Previous workspace

command.terminal.split_horizontal.name = Split terminal horizontally
command.terminal.split_vertical.name = Split terminal vertically
command.terminal.tab_new.name = New terminal tab
command.terminal.close.name = Close terminal pane or tab
command.terminal.focus_next_pane.name = Focus next terminal pane
command.terminal.focus_previous_pane.name = Focus previous terminal pane
command.terminal.tab_next.name = Next terminal tab
command.terminal.tab_previous.name = Previous terminal tab

# ---- Settings (universal search) ----
settings.section.general = General
settings.section.appearance = Appearance
settings.section.input = Input
settings.section.shortcuts = Shortcuts
settings.section.locale = Locale
settings.section.privacy = Privacy

# ---- Settings panel ----
settings-panel-title = Settings
settings-panel-hint = Changes save automatically to config.toml. Shortcut overrides and leader bindings are read-only here — edit those in config.toml directly.
settings-panel-coming-soon = The full settings editor for this section is not available yet. Edit config.toml directly for now.
settings-panel-ok = Close
settings-open-in-editor = Open in editor
settings-open-config-file = Open config.toml

settings-value-yes = Yes
settings-value-no = No
settings-value-none = None
settings-value-default = Default
settings-value-disabled = Disabled
settings-value-system-default = System default
settings-value-hand-left = Left
settings-value-hand-right = Right
settings-value-pen-double-tap-none = None
settings-value-pen-double-tap-switch-tool = Switch tool
settings-value-pen-double-tap-erase = Erase
settings-value-sunday = Sunday
settings-value-monday = Monday

settings-field-auto-update = Auto update
settings-field-telemetry = Telemetry
settings-field-open-on-startup = Open on startup
settings-field-theme = Theme
settings-field-density = Density
settings-field-font-family = Font family
settings-field-font-scale = Font scale
settings-field-reduce-motion = Reduce motion
settings-field-follow-system-theme = Follow system theme
settings-field-dark-theme = Dark theme
settings-field-light-theme = Light theme
settings-field-primary-hand = Primary hand
settings-field-mirror-edge-swipes = Mirror edge swipes
settings-field-haptic-feedback = Haptic feedback
settings-field-palm-rejection = Palm rejection
settings-field-pen-double-tap = Pen double-tap
settings-field-shortcut-overrides = Shortcut overrides
settings-field-leader-key = Leader key
settings-field-leader-timeout = Leader timeout
settings-field-leader-bindings = Leader bindings
settings-field-language = Language
settings-field-date-format = Date format
settings-field-time-format = Time format
settings-field-first-day-of-week = First day of week
settings-field-record-action-history = Record action history
settings-field-history-retention-days = History retention (days)
settings-field-clear-clipboard-seconds = Clear clipboard after copy
settings-field-vault-auto-lock = Vault auto-lock (seconds)

command.settings.open.name = Open settings
command.settings.open.desc = Show the settings panel
command.settings.open_config_file.name = Open config
command.settings.open_config_file.desc = Open config.toml in the default editor
command.password.lock.name = Lock password vault
command.password.lock.desc = Clear the unlocked password database from memory

command.navigation.show_workspace_panel.name = Show workspace panel
command.navigation.show_workspace_panel.desc = Toggle the workspace sidebar
command.notification.show_center.name = Show notification center
command.notification.show_center.desc = Toggle the notification center overlay
command.dock.show.name = Show dock
command.dock.show.desc = Toggle the widget dock
command.search.show_universal.name = Universal search
command.search.show_universal.desc = Open or focus universal search

command.onboarding.toggle_hint_mode.name = Toggle hint mode
command.onboarding.toggle_hint_mode.desc = Show or hide gesture hints on the workspace

navigation-workspace-panel-title = Workspaces
notification-center-title = Notifications
notification-center-placeholder = No notifications yet.
notification-center-clear = Clear all
notification-center-tip-title = Tip
notification-center-tip-body = Swipe from the right edge or run “Show notification center” to open this panel.

# ---- Terminal tab bar ----
terminal-tooltip-split-h = Split horizontally (Ctrl+Shift+H)
terminal-tooltip-split-v = Split vertically (Ctrl+Shift+J)
terminal-tooltip-tab-new = New tab (Ctrl+Shift+T)

# ---- Media player ----
media-no-session = No media playing
media-loading = Loading media…
media-unsupported = Media controls are not available on this platform
media-play = Play
media-pause = Pause
media-next = Next
media-previous = Previous

# ---- Password manager ----
password-locked = Database is locked
password-unlock-label = Master password
password-unlock-placeholder = Enter master password
password-unlock-submit = Unlock
password-unlock-biometric = Windows Hello
password-unlock-biometric-prompt = Unlock password vault
password-search-placeholder = Search entries...
password-no-entries = No entries yet
password-copy-password = Copy password
password-copy-username = Copy username
password-copy-totp = Copy TOTP
password-open-url = Open URL
password-password-copied = Password copied (clears in 30s)
password-totp-copied = TOTP copied (clears in 30s)
password-totp-remaining = { $s }s


# ==== Viewer widget ====
widget-viewer-name = Viewer
widget-viewer-desc = Open files: images, PDF, text, archives
viewer-loading = Loading…
viewer-error = Cannot display this file
viewer-unsupported = Unsupported file type
viewer-image-fit-screen = Fit to screen
viewer-image-actual-size = Actual size
viewer-image-rotate = Rotate
viewer-image-flip-h = Flip horizontal
viewer-image-flip-v = Flip vertical
viewer-pdf-page-of = Page { $current } of { $total }
viewer-pdf-fit-width = Fit width
viewer-pdf-fit-page = Fit page
viewer-text-read-only = Read-only
viewer-text-editing = Editing
viewer-text-save = Save
viewer-text-dirty-indicator = Unsaved changes
viewer-text-unsaved-title = Unsaved changes
viewer-text-unsaved-body = Save changes before closing?
viewer-text-discard = Discard
viewer-archive-extract-all = Extract all
viewer-archive-extract-selected = Extract selected
viewer-archive-preview-binary = Binary file, { $size }

# ==== File manager widget ====
widget-fm-name = Files
widget-fm-desc = Browse, organize, and manage files
fm-nav-back = Back
fm-nav-forward = Forward
fm-nav-up = Up
fm-nav-home = Home
fm-view-icons = Icons
fm-view-list = List
fm-view-details = Details
fm-view-gallery = Gallery
fm-sort-name = Name
fm-sort-size = Size
fm-sort-modified = Modified
fm-sort-type = Type
fm-action-open = Open
fm-action-open-all = Open all
fm-action-open-with = Open with…
fm-action-open-default = Open with default app
fm-action-open-in-viewer = Open in Orchid Viewer
fm-action-copy = Copy
fm-action-cut = Cut
fm-action-paste = Paste
fm-action-rename = Rename
fm-action-delete = Delete
fm-action-new-folder = New folder
fm-action-new-tab = New tab
fm-action-close-tab = Close tab
fm-action-select-all = Select all
fm-action-deselect-all = Deselect all
fm-action-star = Star
fm-action-unstar = Unstar
fm-action-encrypt = Encrypt
fm-action-reveal = Reveal temporarily
fm-action-decrypt = Decrypt
fm-action-add-tag = Add tag…
fm-action-remove-tag = Remove tag
fm-action-color-label = Color label
fm-color-red = Red
fm-color-orange = Orange
fm-color-yellow = Yellow
fm-color-green = Green
fm-color-blue = Blue
fm-color-purple = Purple
fm-color-gray = Gray
fm-color-none = No color
fm-action-properties = Properties
fm-action-add-to-managed = Add to managed folder
fm-action-remove-from-managed = Remove from managed folders
fm-action-managed-policy = Managed folder policy
fm-managed-policy-title = Managed folder policy
fm-policy-max-size = Max size
fm-policy-retention = Retention
fm-policy-excludes = Exclude patterns
fm-policy-unlimited = Unlimited
fm-policy-forever = Keep forever
fm-policy-retention-days = { $days } days
fm-policy-none = None
fm-sidebar-managed-folder-policy = { $name } ({ $count } files, { $dedup } saved, policy)
fm-sidebar-managed-policy-only = { $name } (policy)
fm-rename-title = Rename
fm-rename-ok = OK
fm-rename-cancel = Cancel
fm-dual-pane-on = Dual pane
fm-dual-pane-off = Single pane
fm-show-hidden-on = Show hidden files
fm-show-hidden-off = Hide hidden files
fm-click-single-on = Single-click to open
fm-click-single-off = Double-click to open
fm-encrypt-title = Encrypt with passphrase
fm-reveal-title = Enter passphrase to reveal
fm-decrypt-title = Enter passphrase to decrypt
fm-info-close = Close
fm-properties-title = Properties
fm-tag-add-title = Add tag
fm-confirm-delete = Delete { $n } items?
fm-confirm-delete-permanent = Permanently delete { $n } items?
fm-status-items = { $n } items
fm-status-selected = { $n } selected
fm-status-total-size = { $size }
fm-status-bar = { $items } items, { $selected } selected
fm-status-managed = { $items } items, { $selected } selected · { $tracked } ingested, { $dedup } deduped
fm-encrypted = Encrypted: { $name }
fm-decrypted = Decrypted: { $name }
fm-managed-added = Added to managed folder
fm-managed-removed = Removed from managed folders
fm-encryption-unavailable = Encryption is not available
fm-passphrase-failed = Passphrase failed: { $reason }
fm-passphrase-invalid = Invalid passphrase
fm-passphrase-required = Passphrase is required
fm-decryption-failed = Decryption failed
fm-passphrase-encrypt-hint = Choose a strong passphrase. It cannot be recovered if lost.
fm-passphrase-decrypt-hint = Enter the passphrase used to encrypt these files.
fm-passphrase-reveal-hint = Files are decrypted to a temporary location for viewing.
fm-passphrase-biometric = Windows Hello
fm-passphrase-biometric-prompt = Unlock encrypted files
fm-revealed = Revealed: { $name }
fm-managed-unavailable = Managed folders are not available
fm-managed-no-selection = Select a folder to add to managed folders
fm-not-managed-folder = Not a managed folder
fm-managed-conflict = Managed folder conflict
fm-sidebar-managed-folder = { $name } ({ $count } files, { $dedup } saved)
fm-ingest-failed = Ingest failed: { $name }
fm-quick-filter-placeholder = Filter…
fm-sidebar-favorites = Favorites
fm-sidebar-categories = Categories
fm-sidebar-managed = Managed folders
fm-network-placeholder = No network mounts configured. Add [[file-manager.network-mounts]] entries in config.toml (SFTP, SMB, WebDAV, FTP via rclone).
fm-network-no-provider = No filesystem provider is registered for this network location.
fm-network-rclone-missing = rclone is not installed or not on PATH. Set RCLONE_BIN if needed.
fm-network-invalid-mount = This network mount is misconfigured. Check name and URI in config.toml.
fm-network-auth-failed = Authentication failed. Check username and password in config.toml.
fm-network-permission-denied = Permission denied on this network location.
fm-network-connection-failed = Could not connect to the network host. Check the URI and your network.
fm-ingested = Ingested: { $name }
fm-ingesting = Ingesting: { $name } ({ $count } active)
fm-ingesting-count = Ingesting { $count } files…
fm-copying = Copying: { $name } ({ $percent }%)
fm-moving = Moving: { $name } ({ $percent }%)
fm-transfer-failed = Transfer failed: { $reason }
fm-transfer-already-exists = A file with that name already exists
fm-transfer-virtual-dest = Cannot copy or move into a virtual folder
fm-clipboard-copy = { $count } entries ready to paste
fm-clipboard-cut = { $count } entries (cut) ready to paste
fm-sidebar-tags = Tags
fm-sidebar-recent = Recent
fm-sidebar-network = Network
fm-sidebar-network-all = All places
fm-category-images = Images
fm-category-documents = Documents
fm-category-video = Video
fm-category-audio = Audio
fm-category-archives = Archives
fm-virtual-recent = Recent
fm-virtual-starred = Starred
fm-virtual-tags = Tags
fm-virtual-recent-empty = No recent files yet. Open files to see them here.
fm-virtual-starred-empty = No starred files yet. Star items from the context menu.
fm-virtual-tags-empty = No tagged files yet. Add tags from the context menu.
fm-virtual-category-empty = No matching files found in this category.
fm-virtual-create-denied = Cannot create folders in a virtual location
fm-empty-folder = This folder is empty
fm-error-access = Cannot access this location


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = Welcome to Orchid
startup-subtitle = A touch-first computing environment
startup-version-label = Version { $version }
status-theme = Theme:
status-language = Language:
status-density = Density:
density-touch = Touch
density-mouse = Mouse
density-hybrid = Hybrid

# ---- Workspace shell (task 11B) ----
startup-get-started = Get Started

# ---- Onboarding tour ----
onboarding-back = Back
onboarding-next = Next
onboarding-skip = Skip tour
onboarding-finish = Get started

onboarding-step-welcome-title = Welcome to Orchid
onboarding-step-welcome-body = Orchid is a touch-first workspace where gestures, commands, and widgets are three forms of the same action. This short tour shows the essentials.

onboarding-step-workspace-title = Your workspace
onboarding-step-workspace-body = Switch workspaces at the top, arrange widgets on the canvas, and add new ones from the dock at the bottom.

onboarding-step-palette-title = Command palette
onboarding-step-palette-body = Press Ctrl+Shift+P to run any command. Every entry shows its keyboard shortcut so you can learn as you go.

onboarding-step-gestures-title = Gestures and hints
onboarding-step-gestures-body = Swipe from screen edges for panels and the dock. Press Win+? anytime to toggle hint mode and see what is available in the current context.

onboarding-hint-workspace = Swipe from the left edge for workspaces
onboarding-hint-dock = Swipe up from the bottom edge for the dock
onboarding-hint-gestures = Win+? toggles these hints

workspace-default-name = Main
workspace-new = New workspace
workspace-unnamed = Workspace { $n }
dock-add-label = Add widget
catalog-title = Widget catalog
catalog-search-placeholder = Search widgets…
dock-widget-terminal = Terminal
dock-widget-weather = Weather
dock-widget-moon = Moon
dock-widget-system = System
dock-widget-rss = News
dock-widget-recent-files = Recent
dock-widget-search = Search
dock-widget-media = Media
dock-widget-password = Passwords
dock-widget-viewer = Viewer
dock-widget-fm = Files

viewer-no-file = No file open
viewer-loading-path = Loading: { $path }
viewer-error-with-reason = Cannot display this file: { $reason }
viewer-pdf-unavailable = PDF support is unavailable on this build.
viewer-archive-select-preview = Select a file to preview
viewer-archive-binary-preview = Binary file, { $size }

password-select-entry = Select an entry
password-label-title = Title
password-label-username = Username
password-label-password = Password
password-label-url = URL
password-label-notes = Notes
password-label-totp = TOTP
password-action-copy = Copy
password-action-open = Open
password-action-lock = Lock
password-action-add = Add
password-add-title = New entry
password-add-submit = Save
password-add-cancel = Cancel
password-generate = Generate
password-add-error-title = Title is required
password-entry-added = Entry saved

password-username-copied = Username copied

moon-age-label = Age
moon-distance-label = Distance
moon-next-full-label = Next full
moon-next-new-label = Next new
moon-moonrise-label = Moonrise
moon-moonset-label = Moonset
moon-sunrise-label = Sunrise
moon-sunset-label = Sunset
moon-libration-label = Libration

widget-title-terminal = Terminal
widget-close-tooltip = Close widget
widget-close-confirm = Close { $name }?
action-confirm-yes = Yes
action-confirm-no = No

fm-confirm-title = Confirm