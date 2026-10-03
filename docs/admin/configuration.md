# Configuration

File: `%APPDATA%\Orchid\Orchid\config\config.toml` (created on first launch,
hot-reloaded). Keys are kebab-case.

## `[general]`

| Key | Default | Behavior |
|-----|---------|----------|
| `auto-update` | `true` | On startup, check the public GitHub releases API. A newer tag notifies. The command **Check for updates** also opens the release page. Nothing is downloaded or installed. |
| `telemetry` | `false` | Opt-in. When on, append `app-start` (version, OS family, language) to `data\telemetry.jsonl`. |
| `telemetry-endpoint` | empty | `https` URL that receives that JSON. Empty keeps it on disk. Other schemes are refused. Redirects are not followed. |
| `open-on-startup` | `false` | Autostart helper |
| `os-notifications` | `false` | Mirror in-app alerts as Windows Action Center toasts. Writes a Start Menu `Orchid.lnk` with AppUserModelID `IonPmp.Orchid` when enabled. Failures stay in the in-app center. |

## `[terminal]`

| Key | Default | Behavior |
|-----|---------|----------|
| `grid` | `orchid` | `orchid` is the built-in grid. `alacritty` feeds new sessions through `alacritty_terminal`. Both grids draw Sixel and direct Kitty images, accept OSC 52 copy, and store an OSC 7 directory. OSC 52 paste and zlib Kitty payloads are ignored. Other values stay on `orchid`. Open sessions are left as they are. The desktop app compiles this grid in. A build of the terminal crate without the `alacritty-grid` feature ignores `alacritty`. |

## `[appearance]`

`theme` (default `orchid-dark`), `density` (`touch`/`mouse`/`hybrid`),
`font-family`, `font-scale` (`0.75..=2.0`), `reduce-motion`,
`follow-system-theme`, `dark-theme` / `light-theme`.

User JSON themes: `config\themes\` — `id`, `display_name`, `is_dark`,
`tokens.color` hex (`#RRGGBB` / `#RRGGBBAA`).

Settings → Marketplace writes `market-ink`, `market-dawn`, `market-pine`,
or `market-ember` into that folder and sets `theme` to the new id. Remove
deletes the file only when the JSON id matches. It does not download widget
code; **Add widget** creates an instance of a built-in widget.

## `[input]`

`primary-hand`, `mirror-edge-swipes`, `palm-rejection` (default true: finger
contacts are ignored while a pen is down, including a finger that was already
moving), `pen-double-tap-action` (`none`, `switch-tool`, `erase`; default
`switch-tool`). Switch tool toggles whether the pen drives edge gestures.
Erase holds the pen so it does not drive gestures until the next double-tap.
`haptic-feedback` (default true) keeps Windows touch and pen tap feedback on
the Orchid window; off suppresses those four feedbacks. Devices without OS
tap feedback stay quiet either way. There is no ink canvas: the pen does not
draw.

## `[shortcuts]`

`profile`, `overrides`, `leader-key` (default `Ctrl+Shift+Space`; empty
disables), `leader-timeout-ms` (1200), `leader-bindings`. Settings shows
each letter as a command id. Clearing a row removes it. **Add leader
binding** takes `p=command-palette`. The next key after the leader uses
the saved map.

## `[locale]`

`language` (`en-US`), optional `date-format` / `time-format`.
`first-day-of-week` (`0` Sunday / `1` Monday) drives the calendar widget.

## `[privacy]`

`record-action-history`, `history-retention-days` (90, max 3650),
`clear-clipboard-seconds` (30), `vault-auto-lock-seconds` (300; `0` = never).

## `[onboarding]`

`completed`, `hint-mode-enabled`.

## `[file-manager]`

`network-mounts` — see [network.md](network.md). Runtime bookmarks are
`data\network-bookmarks.toml`.

## `[search]`

Settings → Search, and `[search]` in `config.toml`. `included-roots`
(empty → Documents), `excluded-patterns`, `max-file-size-mib` (50, range
1–4096), `extract-text`, `extract-pdf`. In the panel, lists are separated
by semicolons. Roots are Orchid `FsPath` strings
(`local:c:/Users/Alice/Documents`). Saving roots, exclusions, or the size
limit updates the running index. `sentence-model` is an optional path
to a replacement ONNX graph (`features` in, `embedding` out). Empty uses
the compiled-in quantized model in the desktop app. That path, and which
extractors were built, are read when the index opens.

## `[agent]`

Settings → Agent, and `[agent]` in `config.toml`. `enabled` (false),
`backend` (`ollama` or `openai`), `endpoint` (empty →
`http://127.0.0.1:11434` or `https://api.openai.com/v1`), `model`,
`api-key`. Universal Search sends `? your question` and posts the reply
as a notification. The key is a DPAPI blob after Orchid saves it.
Redirects are not followed.

## `[photos]`

Settings → Photos, and `[photos]` in `config.toml`. `auto-tag` (false)
tags an image from a `People/Name` or `Events/…` folder when that folder
is opened. `detect-faces` (false) asks Windows for face rectangles in
images in the open folder and tags matches `people/unnamed`. It does not
name the person. Rectangles are stored in `data/photo-faces.json` and
drawn on the open image.
Files → Photos groups `people/`, `event/`, and `album/` tags.

## `[shell]`

Settings → Shell, and `[shell]` in `config.toml`. `replace` (false) makes
the next sign-in for this Windows user open Orchid instead of Explorer.
`previous` stores the HKCU `Shell` value to write back. Empty deletes that
value so the machine default is used. Orchid writes only
`HKCU\Software\Microsoft\Windows NT\CurrentVersion\Winlogon`.
`orchid.exe --restore-shell` clears `replace` and puts the previous shell
back before the single-instance check, so it still works while Orchid is
the running shell. Turning the switch on or off, and `--restore-shell`,
append a line to `audit.log`.

## `[policy]`

Settings → Policy, and `[policy]` in `config.toml`. `url` is an optional
https address. Empty uses only the local file. At startup Orchid reads
that address into `policy.toml` in the same directory. A network error, a
redirect, or a document that does not parse leaves the previous file.
The audit log is `audit.log` beside `config.toml`. It records policy
apply, update checks, and shell changes, and it is not sent anywhere.

`policy.toml` marks settings read-only. It does not change their values:

```toml
[lock]
auto-update = true
telemetry = true
telemetry-endpoint = false
open-on-startup = true
os-notifications = false
theme = false
language = false
shell-replace = true
policy-url = false
```

A missing key stays editable. Unknown keys are ignored. Search fields use
`search-roots`, `search-excludes`, `search-max-mib`, `search-extract-text`,
`search-extract-pdf`, and `search-model`. Appearance adds `density`,
`font-family`, `font-scale`, `reduce-motion`, `follow-system-theme`,
`dark-theme`, and `light-theme`. Locale adds `date-format`, `time-format`,
and `first-day-of-week`. Privacy adds `record-action-history`,
`history-retention-days`, `clear-clipboard-seconds`, and
`vault-auto-lock-seconds`. The terminal grid is `terminal-grid`. Input adds
`primary-hand`, `mirror-edge-swipes`, `haptic-feedback`, `palm-rejection`,
and `pen-double-tap`. Photos adds `photos-auto-tag` and
`photos-detect-faces`. Agent adds `agent-enabled`, `agent-backend`,
`agent-endpoint`, `agent-model`, and `agent-key` (the key field and Clear
key). Shortcuts and Marketplace stay editable. Editing `config.toml` by
hand still changes a locked value; the Settings panel refuses the change.

## Text mode

`orchid --tui [path]` lists a local folder in the terminal. See
[user/tui.md](../user/tui.md). It reads `[locale].language` and does not
write config.
