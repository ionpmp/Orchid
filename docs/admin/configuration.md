# Configuration

File: `%APPDATA%\Orchid\Orchid\config\config.toml` (created on first launch,
hot-reloaded). Keys are kebab-case.

## `[general]`

| Key | Default | Behavior |
|-----|---------|----------|
| `auto-update` | `true` | **Not implemented.** Settings: Disabled |
| `telemetry` | `false` | **Not implemented.** Settings: Disabled |
| `open-on-startup` | `false` | Autostart helper |

## `[appearance]`

`theme` (default `orchid-dark`), `density` (`touch`/`mouse`/`hybrid`),
`font-family`, `font-scale` (`0.75..=2.0`), `reduce-motion`,
`follow-system-theme`, `dark-theme` / `light-theme`.

User JSON themes: `config\themes\` — `id`, `display_name`, `is_dark`,
`tokens.color` hex (`#RRGGBB` / `#RRGGBBAA`).

## `[input]`

Wired: `primary-hand`, `mirror-edge-swipes`. Stored but unused (no Settings
rows): `haptic-feedback`, `palm-rejection`, `pen-double-tap-action`.

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
