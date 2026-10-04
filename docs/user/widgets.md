# Built-in widgets

| Catalog name | `type_id` | Notes |
|--------------|-----------|--------|
| Weather | `weather` | Open-Meteo, multi-city |
| Moon | `moon` | Local phase |
| Jyotish | `jyotish` | [jyotish.md](../jyotish.md) |
| Clock | `clock` | World clocks |
| System | `system` | CPU, memory, disks, net, battery, 60-sample graphs |
| Processes | `processes` | Processes / Services / Startup / Users |
| Calculator | `calculator` | `=expr` in universal search |
| Notes | `notes` | In-widget scratchpad |
| Agent | `agent` | Shared conversation; file writes wait for confirmation |
| Calendar | `calendar` | Local events, plus one CalDAV collection |
| Browser | `browser` | WebView2: tabs, bookmarks, find, zoom |
| News Feed | `rss` | RSS/Atom |
| Universal Search | `universal-search` | [search.md](search.md) |
| Now Playing | `media-player` | Windows SMTC |
| Audio Player | `audio-player` | Library, queue, lyrics panel |
| Video Player | `video-player` | Library + queue |
| Passwords | `password-manager` | [passwords.md](passwords.md) |
| Mail | `mail` | [mail.md](mail.md) |
| Viewer | `viewer` | [viewers.md](viewers.md) |
| Files | `file-manager` | [file-manager.md](file-manager.md) |
| Recent Files | `recent-files` | MRU |
| Terminal | `terminal` | [terminal.md](terminal.md) |

**Document Editor** / **Media Player** catalog tiles spawn Viewer instances.

## Agent

The Agent widget and Universal Search `?` share one transcript in
`data/agent-chat.json` (about 40 messages, on this computer). The model
can read a local text file, list one folder, and search the open file
index. **Write file** replaces the proposed path only after you confirm.
**Dismiss** drops the proposal. There is no shell. The agent stays off
until Settings → Agent is enabled and a model name is set.

## System

Each CPU, memory, disk, network, and battery row draws the last 60 samples,
oldest on the left. At the default 2 second refresh that is about two
minutes. Network bars are scaled to the peak in that window. The samples
stay with the widget instance and are not written to disk. Uptime has no
graph.

## Audio Player

Library roots, queue, shuffle/repeat, crossfade, playlists, M3U, SMTC,
Explorer drop. Lyrics: sidecar `.lrc`, else ID3 `SYLT`/`USLT` or
Vorbis/FLAC comments. **L** or the lyrics chip toggles a scrollable panel
(click a synced line to seek). Panel open state persists.

Needs libmpv. Mutually pauses with a media Viewer.

## Browser

Embedded WebView2. Needs the Evergreen runtime. Distinct from the HTML
**file** viewer.

## Notes / Calendar / Processes

Notes are not files on disk. Calendar events stay on this computer. A collection URL, user, and password at the bottom of the calendar sync CalDAV. Extra collection URLs, separated by a space or a new line, use the same account. Sync covers 90 days ago through the next year. A time written with `Z` is shown in the local offset. A named timezone is stored as the numbers in the file. Daily, weekly, monthly, and yearly repeats, with interval, count, until, and weekly weekdays, are shown on each day in that window. Other rule parts are ignored, and a rule this build cannot expand stays on the server. A multi-day event is shown on each day, up to 14 days and 400 occurrences. Editing or deleting one of those days stays on this computer and does not change the series; the next sync restores the series days. Saving or deleting a one-time synced event writes that change back. A local event with no account stays local. The password is stored with the widget config (DPAPI on Windows) and is not shown again. There is no server discovery and no OAuth. The Processes tab draws
CPU and memory for the selected process: the last 60 samples, oldest on
the left. CPU is that process's percent. Memory bars are scaled to the
peak working set in the window. The samples stay with the widget instance
and are not written to disk. Services, Startup, and Users have no graphs.
