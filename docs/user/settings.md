# Settings and shortcuts

`Ctrl+,` or `settings open`. **Open config file** for keys the panel does
not edit. The file hot-reloads on the next UI tick.

Full key list: [admin/configuration.md](../admin/configuration.md).

## Settings panel

| Section | You can change | Shown but not wired |
|---------|----------------|---------------------|
| General | Open on startup | Auto-update, telemetry (Disabled) |
| Appearance | Theme, density, font, reduce motion, follow system | — |
| Input | Primary hand, mirror edge swipes | Haptics, palm, pen double-tap (TOML only; hidden here) |
| Shortcuts | Profile, remaps, leader key/timeout | Leader **binding map** (TOML only) |
| Locale | Language, date/time format, first day of week | — |
| Privacy | History, retention, clipboard clear, vault auto-lock | — |

Widget options stay on each widget.

## Profiles and leader key

`[shortcuts].profile`: `orchid`/`commander`, `windows`, `macos`, `linux`
(aliases exist). Overrides in Settings or TOML.

Default leader **Ctrl+Shift+Space** then: `p` palette, `s` settings, `l`
lock vault, `n`/`b` workspace next/prev. Empty `leader-key` disables it.

## Themes and language

Bundled: `orchid-dark` (default), `orchid-light`, `solarized-dark`,
`solarized-light`, `nord-dark`, `catppuccin-mocha`, `catppuccin-latte`,
`high-contrast-dark`, `high-contrast-light`. JSON under `config\themes\`.

Languages: `en-US`, `ru-RU`, `de-DE`, `fr-FR`, `es-ES`, `it-IT`, `pt-BR`,
`ja-JP`, `zh-CN`, `ko-KR`, `ar-SA`. Overlays:
`config\locales\<tag>\main.ftl`.
