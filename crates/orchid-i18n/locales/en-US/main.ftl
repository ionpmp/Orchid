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
weather-humidity-label = Humidity
weather-wind-label = Wind

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

# ---- RSS ----
rss-no-feeds = No feeds configured
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

# ---- Media player ----
media-no-session = No media playing
media-play = Play
media-pause = Pause
media-next = Next
media-previous = Previous

# ---- Password manager ----
password-locked = Database is locked
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
fm-quick-filter-placeholder = Filter…
fm-sidebar-favorites = Favorites
fm-sidebar-categories = Categories
fm-sidebar-managed = Managed folders
fm-network-placeholder = Network mounts are not configured yet. SFTP, SMB, and WebDAV support via rclone is planned.
fm-network-no-provider = No filesystem provider is registered for this network location.
fm-network-rclone-missing = rclone is not installed or not on PATH. Set RCLONE_BIN if needed.
fm-ingested = Ingested: { $name }
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
password-label-username = Username
password-label-password = Password
password-label-url = URL
password-label-notes = Notes
password-label-totp = TOTP
password-action-copy = Copy
password-action-open = Open

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