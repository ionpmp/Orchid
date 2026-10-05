# Built-in widgets

| Catalog name | `type_id` | Notes |
|--------------|-----------|--------|
| Weather | `weather` | Open-Meteo, multi-city |
| Moon | `moon` | Local phase |
| Jyotish | `jyotish` | [jyotish.md](../jyotish.md) |
| Clock | `clock` | World clocks |
| System | `system` | CPU, memory, disks, net, battery, 60-sample graphs |
| Processes | `processes` | Processes / Services / Startup / Users |
| Optimize | `optimize` | Windows update, privacy, Explorer, and taskbar settings |
| Protection | `protect` | Traces, histories, free-space overwrite, outbound blocks |
| Calculator | `calculator` | `=expr` in universal search |
| Notes | `notes` | In-widget scratchpad |
| Agent | `agent` | Shared conversation; file writes wait for confirmation |
| Calendar | `calendar` | Local events, plus one CalDAV collection |
| Contacts | `contacts` | Local cards in `state.redb`, plus one CardDAV collection |
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

## Protection

Four tabs. **Traces** and **Histories** measure first, then delete only the
checked rows on the tab you are looking at. Cookies, Windows Temp, and the
advertising id stay off until you check them. Browser history keeps
bookmarks. If that browser is still open, its history, cache, and cookies
stay and the status says so; the other checked rows are still removed.
**Clean** stays off until a scan finishes, and then shows how much the
checked rows will remove.

**Free space** writes zeros over unused space on the selected disk and
leaves 64 MB free. One pass is enough. Further passes wear an SSD, and
Windows may TRIM the blocks afterward. Cancel removes the filler file.

**Network** blocks outbound traffic for a program with one Windows Firewall
rule named `Orchid Protect`. That needs an administrator. Open connections
can stay up until the program reconnects. Windows itself, security
processes, and Orchid are not offered as targets. **Add program** blocks an
executable that is not currently running, with the same limits.

## Optimize

One place for Windows settings that Settings scatters, plus a few Group Policy
switches Settings does not show. The catalog follows the useful overlap of
Winaero Tweaker, the recommended O&O ShutUp10 switches, and the Microsoft
Update policies: install mode, no restart while you are signed in, active
hours, staying on the current Windows version, Store app updates, update
sharing, driver updates, advertising and diagnostics, Explorer behavior, the
taskbar, suggestions, fast startup, accessibility prompts, mouse acceleration,
and startup programs.

**Changed** marks a row that is not the usual Windows value. **Restart Explorer**
on a row means that switch shows up after the Explorer restart button.
**As Windows** puts every switch back. **Don't reboot itself** pins the current
version, asks before downloading updates, blocks a restart while you are signed
in, and turns fast startup off. **Quiet desktop** turns off suggestions, widgets,
and Store auto-updates. Machine policies in a set share one administrator prompt.

**Startup** lists Run entries and the Startup folder. Turning one off keeps the
entry and tells Windows not to launch it. A machine entry asks for an
administrator.

A switch writes as soon as you change it. **Administrator** asks Windows to
confirm. Defender, the firewall, SmartScreen, sign-in, and the Windows Update
service are not changed, and inbox apps are not removed. Some menu timing
changes wait until the next sign-in.

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

## Contacts

Cards and the CardDAV account live in that widget's config inside
`state.redb`. A card stores a name, one email, one phone number, and a note. Other vCard
fields, including photos and groups, are ignored. One collection URL uses
basic authentication. Saving the account with an empty password keeps the
previous secret. Sync downloads at most 500 cards and replaces linked cards
with the server copy. A card that has not been uploaded stays on this
computer. A conflict response leaves that local card in place. There is no
address-book discovery and no OAuth.

## Notes

Notes stay in the widget config inside `state.redb`. They are not files
you can open from the file manager.

## Calendar

Local events stay on this computer. A collection URL, user, and password
at the bottom of the calendar sync CalDAV. Extra collection URLs in that
field, separated by a space or a new line, use the same account. Sync
covers 90 days ago through the next year.

A time written with `Z` is shown in the local offset. A named timezone is
stored as the numbers in the file. Daily, weekly, monthly, and yearly
repeats, with interval, count, until, and weekly weekdays, are shown on
each day in that window. A date listed in EXDATE is left out. Other rule
parts and exception forms are ignored, and a rule this build cannot expand
stays on the server. A multi-day event is shown on each day, up to 14 days
and 400 occurrences.

Editing or deleting one of those generated days stays on this computer and
does not change the series. The next sync restores the series days. Saving
or deleting a one-time synced event writes that change back. A local event
with no account stays local. The password is stored with the widget config
(DPAPI on Windows) and is not shown again. There is no server discovery
and no OAuth.

## Processes

The Processes tab draws CPU and memory for the selected process: the last
60 samples, oldest on the left. CPU is that process's percent. Memory bars
are scaled to the peak working set in the window. The samples stay with
the widget instance and are not written to disk. Services, Startup, and
Users have no graphs.
