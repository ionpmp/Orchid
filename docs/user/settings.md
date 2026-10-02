# Settings and shortcuts

`Ctrl+,` or `settings open`. **Open config file** for keys the panel does
not edit. The file hot-reloads on the next UI tick.

Full key list: [admin/configuration.md](../admin/configuration.md).

## Settings panel

| Section | You can change | Shown but not wired |
|---------|----------------|---------------------|
| General | Open on startup, Windows notifications, auto-update, telemetry | — |
| Terminal | Terminal grid (built-in or Alacritty) | — |
| Appearance | Theme, density, font, reduce motion, follow system | — |
| Input | Primary hand, mirror edge swipes, palm rejection, pen double-tap, haptic feedback | — |
| Shortcuts | Profile, remaps, leader key/timeout | Leader **binding map** (TOML only) |
| Locale | Language, date/time format, first day of week | — |
| Privacy | History, retention, clipboard clear, vault auto-lock | — |
| Marketplace | Install Ink, Dawn, Pine, or Ember; add a built-in widget | — |
| Agent | Enable Ollama or an OpenAI-compatible chat, endpoint, model, API key | — |
| Photos | Auto-tag from folder names; find faces in the open folder | — |

Widget options stay on each widget.

## Agent

Settings → Agent. Off until enabled. Universal Search `? your question`
sends one message to Ollama or an OpenAI-compatible server and posts the
reply as a notification. Set the model name. Leave the API key blank to
keep the saved key. **Clear key** removes it.

## Photos

Settings → Photos. **Files → Photos** groups `people/`, `event/`, and
`album/` tags. **Auto-tag from folder names** is off until enabled.
**Find faces in open folders** is also off until enabled. Windows then
stores face rectangles and tags those files `people/unnamed`. It does
not decide who the person is.

## Marketplace

Settings → Marketplace installs one of four palettes (Ink, Dawn, Pine,
Ember) as `config/themes/<id>.json` and switches `[appearance].theme` to it.
Remove deletes that file only when its id matches the catalog. Removing the
active palette switches back to `orchid-dark`. **Add widget** places a
built-in widget on the workspace. Orchid does not download widget code.

## Updates and telemetry

`[general].auto-update` (default on) asks GitHub for the latest
`ionpmp/Orchid` release when the window opens. A newer tag shows a
notification. **Check for updates** does the same and opens that release
page. Orchid does not download or replace its own files.

`[general].telemetry` is off by default. When on, each launch appends one
line to `data\telemetry.jsonl`: event `app-start`, the app version, the OS
family (`windows`, `macos`, `linux`), and the configured language. No paths
or file names. If `telemetry-endpoint` is an `https://` URL, that same JSON
is posted there and redirects are not followed. Any other endpoint is
refused. Turning telemetry off stops new lines and sends.

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
