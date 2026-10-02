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
| `grid` | `orchid` | `orchid` is the built-in grid (Sixel, Kitty, OSC 7, OSC 52). `alacritty` feeds new sessions through `alacritty_terminal`. That grid has no inline images, no OSC 52, and no OSC 7 directory. Other values stay on `orchid`. Open sessions are left as they are. The desktop app compiles this grid in. A build of the terminal crate without the `alacritty-grid` feature ignores `alacritty`. |

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
disables), `leader-timeout-ms` (1200), `leader-bindings`.

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

Not in the Settings panel. `included-roots` (empty → Documents),
`excluded-patterns`, `max-file-size-mib` (50), `extract-text`,
`extract-pdf`. Roots are Orchid `FsPath` strings
(`local:c:/Users/Alice/Documents`).
