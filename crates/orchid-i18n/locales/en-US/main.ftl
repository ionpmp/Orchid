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
