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
widget-weather-desc = Current conditions, multi-city, and swipeable forecast

widget-moon-name = Moon
widget-moon-desc = Current lunar phase, rise/set times, and celestial data

widget-system-name = System
widget-system-desc = CPU, memory, disk, network, and battery indicators
# ---- Shared size / duration formatting ----
byte-size-b = { $value } B
byte-size-kb = { $value } KB
byte-size-mb = { $value } MB
byte-size-gb = { $value } GB
byte-size-tb = { $value } TB
duration-days-hours = { $days }d { $hours }h
duration-hours-minutes = { $hours }h { $minutes }m
duration-minutes = { $minutes }m

# ---- Locale display names (endonyms) ----
locale-name-en-US = English (United States)
locale-name-ar-SA = العربية
locale-name-de-DE = Deutsch
locale-name-es-ES = Español
locale-name-fr-FR = Français
locale-name-it-IT = Italiano
locale-name-ja-JP = 日本語
locale-name-ko-KR = 한국어
locale-name-pt-BR = Português (Brasil)
locale-name-ru-RU = Русский
locale-name-zh-CN = 简体中文

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
weather-cities-title = Cities
weather-cities-close = Close
weather-city-search-placeholder = Search cities…
weather-city-add = Add city
weather-city-remove = Remove city
weather-city-no-results = No cities found
weather-city-searching = Searching…

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
weather-forecast-range = { $high } / { $low }
weather-precip-chance = { $pct }%

# ---- Wind directions ----
weather-wind-n = N
weather-wind-nne = NNE
weather-wind-ne = NE
weather-wind-ene = ENE
weather-wind-e = E
weather-wind-ese = ESE
weather-wind-se = SE
weather-wind-sse = SSE
weather-wind-s = S
weather-wind-ssw = SSW
weather-wind-sw = SW
weather-wind-wsw = WSW
weather-wind-w = W
weather-wind-wnw = WNW
weather-wind-nw = NW
weather-wind-nnw = NNW

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
system-status-warning = { $label } — elevated ({ $value })
system-status-critical = { $label } — critical ({ $value })

# ---- RSS ----
rss-no-feeds = No feeds configured
rss-loading = Loading news…
rss-fetch-failed = Could not load feeds. Check your connection and try again.
rss-empty = No items in the configured feeds yet.
rss-item-untitled = (Untitled)
recent-files-empty = No recent files yet. Open files in the viewer or file manager to see them here.
recent-files-open-hint = Open file
rss-open-item-hint = Open link
rss-error-summary = { $n } of { $total } feeds failed to update
rss-item-published-minutes = { $m }m ago
rss-item-published-hours = { $h }h ago
rss-item-published-days = { $d }d ago

# ---- Universal Search ----
search-placeholder = Type to search files, commands, settings...
search-empty-state = Start typing to search
search-no-results = No results for "{ $query }"
search-no-results-short = No results
search-sources-unconfigured = Search sources are not configured yet
search-error-with-reason = Search failed: { $reason }
search-open-hint = Open
search-searching = Searching...
search-source-files = Files
search-source-commands = Commands
search-source-settings = Settings
command-terminal-invocation = orc { $verb }

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
settings-config-reload-failed = Could not apply settings: { $reason }
settings-field-rejected = Could not change setting: { $reason }
settings-validation-failed = Settings could not be saved: { $reason }
settings-save-failed = Could not save settings: { $reason }
settings-error-theme-not-found = Theme not found: { $id }
settings-panel-hint = Changes save automatically to config.toml. Shortcut overrides and leader bindings are read-only here — edit those in config.toml directly.
settings-panel-coming-soon = The full settings editor for this section is not available yet. Edit config.toml directly for now.
settings-panel-ok = Close
settings-open-in-editor = Open in editor
settings-open-config-file = Open config.toml

settings-value-none = None
settings-value-leader-timeout = { $ms } ms
settings-shortcut-binding = { $key } → { $cmd }
settings-shortcut-list-separator = , 
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

command.navigation.show_workspace_panel.name = Show workspaces
command.navigation.show_workspace_panel.desc = Open the workspace switcher
command.notification.show_center.name = Show notification center
command.notification.show_center.desc = Toggle the notification center overlay
command.dock.show.name = Show widget catalog
command.dock.show.desc = Open the widget catalog
command.search.show_universal.name = Universal search
command.search.show_universal.desc = Open or focus universal search

command.onboarding.toggle_hint_mode.name = Toggle hint mode
command.onboarding.toggle_hint_mode.desc = Show or hide gesture hints on the workspace

navigation-workspace-panel-title = Workspaces
notification-center-title = Notifications
notification-center-placeholder = No notifications yet.
notification-center-clear = Clear all
notification-center-dismiss = Dismiss
notification-center-tip-title = Tip
notification-center-tip-body = Swipe from the right edge or run “Show notification center” to open this panel.

# ---- Terminal tab bar ----
terminal-tooltip-split-h = Split horizontally (Ctrl+Shift+H)
terminal-tooltip-split-v = Split vertically (Ctrl+Shift+J)
terminal-tooltip-split-drag = Drag to resize panes
terminal-tooltip-tab-new = New tab (Ctrl+Shift+T)
terminal-tooltip-tab-close = Close tab
terminal-tooltip-pane-close = Close pane

# ---- Media player ----
media-no-session = No media playing
media-loading = Loading media…
media-unsupported = Media controls are not available on this platform
media-play = Play
media-pause = Pause
media-next = Next
media-previous = Previous
media-control-failed = Media control failed
media-control-rejected = Media control was rejected by the system

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
viewer-loading = Loading…
viewer-error = Cannot display this file
viewer-unsupported = Unsupported file type
viewer-image-fit-screen = Fit to screen
viewer-image-actual-size = Actual size
viewer-image-rotate = Rotate
viewer-image-flip-h = Flip horizontal
viewer-image-flip-v = Flip vertical
viewer-image-zoom-in = Zoom in
viewer-image-zoom-out = Zoom out
viewer-image-rotate-cw = Rotate clockwise
viewer-image-rotate-ccw = Rotate counter-clockwise
viewer-archive-root = (root)
viewer-archive-parent = Parent folder
viewer-pdf-page-of = Page { $current } of { $total }
viewer-pdf-fit-width = Fit width
viewer-pdf-fit-page = Fit page
viewer-pdf-go = Go
viewer-pdf-copy-text = Copy text
viewer-pdf-copied = Page text copied
viewer-pdf-copy-empty = No text on this page
viewer-pdf-copy-failed = Could not copy text
viewer-pdf-prev-page = Previous page
viewer-pdf-next-page = Next page
viewer-pdf-info = PDF · page { $current } / { $total } · { $width } × { $height } px · { $zoom }%
viewer-image-info = { $width } × { $height } · { $size } · { $format }
viewer-image-format-png = PNG
viewer-image-format-jpeg = JPEG
viewer-image-format-webp = WebP
viewer-image-format-bmp = BMP
viewer-image-format-gif = GIF
viewer-image-format-tiff = TIFF
viewer-image-format-avif = AVIF
viewer-image-format-tga = TGA
viewer-image-format-svg = SVG
viewer-image-format-heic = HEIC
viewer-image-format-raw = RAW
viewer-image-format-image = Image
viewer-archive-info = { $format }, { $count } entries
viewer-archive-format-zip = ZIP
viewer-archive-format-7z = 7z
viewer-archive-format-tar = TAR
viewer-archive-format-tar-gz = TAR.GZ
viewer-archive-format-tar-xz = TAR.XZ
viewer-archive-extracted-selected = Extracted to { $path }
viewer-archive-extracted-all = Extracted { $count } entries to { $path }
viewer-archive-nothing-selected = Nothing selected to extract
viewer-archive-cannot-extract-folder = Cannot extract a folder
viewer-action-failed = Viewer action failed: { $reason }
viewer-multi-open-capped = Opened { $opened } of { $cap } files ({ $skipped } skipped to keep the UI responsive)
viewer-text-save-failed = Could not save the file: { $reason }
viewer-text-read-only = Read-only
viewer-text-editing = Editing
viewer-text-save = Save (Ctrl+S)
viewer-text-lines = { $count } lines
viewer-text-line-ending-lf = LF
viewer-text-line-ending-crlf = CRLF
viewer-encoding-utf-8 = UTF-8
viewer-encoding-utf-16le = UTF-16 LE
viewer-encoding-utf-16be = UTF-16 BE
viewer-encoding-windows-1252 = Windows-1252
viewer-encoding-windows-1251 = Windows-1251
viewer-encoding-iso-8859-1 = ISO-8859-1
viewer-encoding-iso-8859-5 = ISO-8859-5
viewer-encoding-shift-jis = Shift_JIS
viewer-encoding-euc-jp = EUC-JP
viewer-encoding-euc-kr = EUC-KR
viewer-encoding-gbk = GBK
viewer-encoding-gb18030 = GB18030
viewer-encoding-big5 = Big5
viewer-encoding-koi8-r = KOI8-R
viewer-syntax-plaintext = Plain text
viewer-syntax-rust = Rust
viewer-syntax-python = Python
viewer-syntax-toml = TOML
viewer-syntax-json = JSON
viewer-syntax-markdown = Markdown
viewer-syntax-javascript = JavaScript
viewer-syntax-typescript = TypeScript
viewer-syntax-tsx = TSX
viewer-syntax-bash = Shell
viewer-syntax-c = C
viewer-syntax-cpp = C++
viewer-syntax-csharp = C#
viewer-syntax-css = CSS
viewer-syntax-go = Go
viewer-syntax-html = HTML
viewer-syntax-java = Java
viewer-syntax-kotlin = Kotlin
viewer-syntax-lua = Lua
viewer-syntax-php = PHP
viewer-syntax-ruby = Ruby
viewer-syntax-sql = SQL
viewer-syntax-swift = Swift
viewer-syntax-xml = XML
viewer-syntax-yaml = YAML
viewer-syntax-ini = INI
viewer-syntax-dockerfile = Dockerfile
viewer-syntax-perl = Perl
viewer-text-dirty-indicator = Unsaved changes
viewer-text-unsaved-title = Unsaved changes
viewer-text-unsaved-body = Save changes before closing?
viewer-text-discard = Discard
viewer-archive-extract-all = Extract all
viewer-archive-extract-selected = Extract selected

# ==== File manager widget ====
widget-fm-name = Files
widget-fm-desc = Browse, organize, and manage files
fm-nav-back = Back
fm-nav-back-disabled = No history to go back
fm-nav-forward = Forward
fm-nav-forward-disabled = No history to go forward
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
fm-properties-kind-folder = Folder
fm-properties-kind-file = File
fm-properties-type = Type: { $kind }
fm-properties-size = Size: { $size }
fm-properties-modified = Modified: { $modified }
fm-properties-mime = MIME: { $mime }
fm-tag-add-title = Add tag
fm-confirm-delete = Delete { $n } items?
fm-confirm-delete-permanent = Permanently delete { $n } items?
fm-loading = Loading…
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
fm-action-failed = File action failed: { $reason }
fm-invalid-folder-name = Invalid folder name
fm-no-provider-parent = Cannot access the parent folder
fm-no-parent-folder = No parent folder
fm-selection-multiple-folders = Selection spans multiple folders
fm-invalid-rename-target = Invalid rename target
fm-cannot-rename-root = Cannot rename the root
fm-no-provider-path = Cannot access this path
fm-empty-tag = Tag name cannot be empty
fm-drop-not-directory = Drop target is not a folder
fm-drop-unavailable = Drop target is unavailable
fm-type-ext-file = { $ext } file
fm-transfer-already-exists = A file with that name already exists
fm-transfer-virtual-dest = Cannot copy or move into a virtual folder
fm-clipboard-copy = { $count } entries ready to paste
fm-clipboard-cut = { $count } entries (cut) ready to paste
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
fm-entry-encrypted-hint = Encrypted file
fm-entry-managed-hint = Managed folder
fm-error-access = Cannot access this location
fm-error-not-found = File or folder not found
fm-error-disk-full = Not enough disk space
fm-error-in-use = File is in use by another program
fm-error-io = File operation failed: { $reason }
fm-error-invalid-tab = Invalid tab
fm-error-invalid-sort = Invalid sort column
fm-error-unavailable = File manager is unavailable


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = Welcome to Orchid
startup-subtitle = A touch-first computing environment
startup-version-label = Version { $version }
status-theme = Theme:
status-language = Language:
status-density = Density:
theme-name-orchid-dark = Orchid Dark
theme-name-orchid-light = Orchid Light
theme-name-solarized-dark = Solarized Dark
theme-name-solarized-light = Solarized Light
theme-name-nord-dark = Nord Dark
theme-name-catppuccin-mocha = Catppuccin Mocha
theme-name-catppuccin-latte = Catppuccin Latte
theme-name-high-contrast-dark = High Contrast Dark
theme-name-high-contrast-light = High Contrast Light

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
onboarding-step-progress = Step { $current } of { $total }

onboarding-step-welcome-title = Welcome to Orchid
onboarding-step-welcome-body = Orchid is a touch-first workspace where gestures, commands, and widgets are three forms of the same action. This short tour shows the essentials.

onboarding-step-workspace-title = Your workspace
onboarding-step-workspace-body = Switch workspaces from the corner control, arrange widgets on the canvas, and add new ones from the widget catalog.

onboarding-step-palette-title = Command palette
onboarding-step-palette-body = Press Ctrl+Shift+P to run any command. Every entry shows its keyboard shortcut so you can learn as you go.

onboarding-step-gestures-title = Gestures and hints
onboarding-step-gestures-body = Swipe from screen edges for notifications and the workspace switcher. Long-press the canvas or swipe up with three fingers for the widget catalog. Press Win+? anytime to toggle hint mode.

onboarding-hint-workspace = Hover or tap the corner control for workspaces
onboarding-hint-dock = Long-press the canvas or swipe up with three fingers for widgets
onboarding-hint-gestures = Win+? toggles these hints

workspace-default-name = Main
workspace-new = New workspace
workspace-placement-blocked-title = Cannot place widget here
workspace-placement-blocked-body = That spot overlaps another widget or leaves the grid. Try a free cell.
group-tooltip-dissolve = Ungroup widgets
group-tooltip-move-left = Move tab left
group-tooltip-move-right = Move tab right
group-tooltip-close-tab = Remove from group
group-hint-alt-detach = Alt+drag to detach from group
workspace-unnamed = Workspace { $n }
dock-add-label = Add widget
catalog-title = Widget catalog
catalog-search-placeholder = Search widgets…
catalog-no-results = No matching widgets
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
viewer-error-file-too-large = This file is too large to open
viewer-error-image-decode = Could not decode this image
viewer-error-pdf-render = Could not render this PDF page
viewer-error-pdf-empty = This PDF has no pages
viewer-error-parse-text = Could not read this text file
viewer-error-syntax-grammar = Syntax highlighting is unavailable for this language
viewer-error-archive-entry-not-found = Archive entry not found
viewer-error-thumbnail = Could not generate a thumbnail
viewer-error-unavailable = Viewer is unavailable
viewer-error-no-archive = No archive is open
viewer-error-io = A file error occurred
viewer-error-unknown = An unexpected viewer error occurred
viewer-pdf-unavailable = PDF support is unavailable on this build.
viewer-image-heic-unsupported = HEIC images are not supported yet
viewer-image-raw-unsupported = RAW images are not supported yet
viewer-archive-select-preview = Select a file to preview
viewer-archive-binary-preview = Binary file, { $size }

password-select-entry = Select an entry
password-label-title = Title
password-label-username = Username
password-label-password = Password
password-label-url = URL
password-label-notes = Notes
password-label-totp = TOTP
password-action-lock = Lock
password-action-add = Add
password-add-title = New entry
password-add-submit = Save
password-add-cancel = Cancel
password-generate = Generate
password-add-error-title = Title is required
password-add-error-duplicate = An entry with this title already exists
password-error-invalid-master = Incorrect master password
password-error-biometric-cancelled = Biometric unlock was cancelled
password-error-biometric-unavailable = Biometric unlock is unavailable
password-error-biometric-failed = Biometric unlock failed
password-error-no-master-key = No biometric key is stored for this vault
password-error-db-open = Could not open the password database
password-error-vault-locked = Vault is locked
password-error-unavailable = Password vault is unavailable
password-error-entry-not-found = Entry not found
password-error-with-reason = Password vault error: { $reason }
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
widget-resize-tooltip = Resize widget
widget-close-confirm = Close { $name }?
action-confirm-yes = Yes
action-confirm-no = No

fm-confirm-title = Confirm
# ---- Widget settings dialog ----
widget-settings-tooltip = Widget settings
widget-settings-title = Widget settings
widget-settings.weather.units = Temperature unit
widget-settings.weather.units.celsius = Celsius (°C)
widget-settings.weather.units.fahrenheit = Fahrenheit (°F)
widget-settings.weather.refresh = Refresh interval (minutes)
widget-settings.moon.location-name = Location name
widget-settings.moon.latitude = Latitude
widget-settings.moon.longitude = Longitude
widget-settings.moon.show-sunrise-sunset = Show sunrise / sunset
widget-settings.moon.show-libration = Show libration
widget-settings.system.show-cpu = Show CPU
widget-settings.system.show-memory = Show memory
widget-settings.system.show-disks = Show disks
widget-settings.system.show-network = Show network
widget-settings.system.show-battery = Show battery
widget-settings.system.show-uptime = Show uptime
widget-settings.system.refresh = Refresh interval (seconds)
widget-settings.rss.feed-name = Feed name
widget-settings.rss.feed-url = Feed URL
widget-settings.rss.max-items = Max items
widget-settings.rss.refresh = Refresh interval (minutes)
widget-settings.rss.open-in-browser = Open links in browser
widget-settings.fm.dual-pane = Dual pane
widget-settings.fm.show-hidden = Show hidden files
widget-settings.fm.single-click-open = Open with single click
widget-settings.fm.show-extensions = Show extensions
widget-settings.fm.confirm-delete = Confirm before delete
widget-settings.fm.delete-to-recycle = Move deleted files to Recycle Bin
widget-settings.fm.thumbnail-size = Thumbnail size
widget-settings.fm.thumbnail-size.small = Small
widget-settings.fm.thumbnail-size.medium = Medium
widget-settings.fm.thumbnail-size.large = Large
