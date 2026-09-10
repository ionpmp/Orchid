# Built-in widgets

| Catalog name | `type_id` | Notes |
|--------------|-----------|--------|
| Weather | `weather` | Open-Meteo, multi-city |
| Moon | `moon` | Local phase |
| Jyotish | `jyotish` | [jyotish.md](../jyotish.md) |
| Clock | `clock` | World clocks |
| System | `system` | CPU, memory, disks, net, battery |
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

Notes are not files on disk. Calendar is local. Processes has no
Performance graphs (roadmap).
