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
| Calendar | `calendar` | Local only (no CalDAV) |
| Browser | `browser` | WebView2: tabs, bookmarks, find, zoom |
| News Feed | `rss` | RSS/Atom |
| Universal Search | `universal-search` | [search.md](search.md) |
| Now Playing | `media-player` | Windows SMTC |
| Audio Player | `audio-player` | Library, queue, lyrics panel |
| Video Player | `video-player` | Library + queue |
| Passwords | `password-manager` | [passwords.md](passwords.md) |
| Viewer | `viewer` | [viewers.md](viewers.md) |
| Files | `file-manager` | [file-manager.md](file-manager.md) |
| Recent Files | `recent-files` | MRU |
| Terminal | `terminal` | [terminal.md](terminal.md) |

**Document Editor** / **Media Player** catalog tiles spawn Viewer instances.

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

Notes are not files on disk. Calendar is local. The Processes tab draws
CPU and memory for the selected process: the last 60 samples, oldest on
the left. CPU is that process's percent. Memory bars are scaled to the
peak working set in the window. The samples stay with the widget instance
and are not written to disk. Services, Startup, and Users have no graphs.
