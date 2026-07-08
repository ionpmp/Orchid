# orchid-i18n

Localization for Orchid. Message catalogues are [Fluent](https://projectfluent.org/)
(`.ftl`) files; the crate loads them into `fluent_bundle::FluentBundle` instances
and exposes a small [`LocaleManager`] API for the Slint UI layer.

## Catalogues

Each locale lives under `locales/<tag>/main.ftl` inside this crate. Keys use
kebab-case (e.g. `startup-welcome`, `widget-weather-name`). Placeholders follow
Fluent syntax (`{ $version }`).

Eleven locales are bundled at compile time via `include_str!`:

| Tag | Language |
|-----|----------|
| `en-US` | English (fallback) |
| `ru-RU` | Russian |
| `de-DE` | German |
| `fr-FR` | French |
| `es-ES` | Spanish |
| `it-IT` | Italian |
| `pt-BR` | Brazilian Portuguese |
| `ja-JP` | Japanese |
| `zh-CN` | Simplified Chinese |
| `ko-KR` | Korean |
| `ar-SA` | Arabic |

## Runtime overlay

[`LocaleManager::new`] always registers every bundled catalogue. When
`extra_dir` is set (typically `OrchidPaths::locales_dir`, i.e.
`{config_dir}/locales/`), the manager overlays
`<extra_dir>/<tag>/main.ftl` on top of the bundled copy for each language.
Missing overlay files are ignored; parse / I/O errors are logged and skipped.

This lets users patch or extend translations without rebuilding Orchid.

## LocaleManager API

| Method | Purpose |
|--------|---------|
| [`LocaleManager::new`] | Build manager; register all bundled locales + optional overlay dir |
| [`LocaleManager::current`] / [`set_current`] | Read or switch the active locale |
| [`LocaleManager::available_locales`] | List registered locales |
| [`LocaleManager::tr`] | Resolve a key in the current locale |
| [`LocaleManager::tr_args`] | Same, with Fluent placeholder arguments |
| [`default_language`] | Returns `en-US` (fallback when a key is missing) |

Missing keys fall back to `en-US`, then echo the key so untranslated strings
are visible during development.

## Example

```rust
use orchid_i18n::{default_language, FluentArgs, LocaleId, LocaleManager};

let mgr = LocaleManager::new(default_language(), None).unwrap();
assert!(mgr.tr("startup-welcome").contains("Welcome"));

mgr.set_current(LocaleId::parse("ru-RU").unwrap());
let welcome = mgr.tr("startup-welcome");

let version = mgr.tr_args(
    "startup-version-label",
    &FluentArgs::new().with("version", "1.0.0"),
);
```

## Scope

This crate owns catalogue loading and string resolution only. Layout direction
(RTL vs LTR) and pushing resolved strings into Slint globals live in
`orchid-ui` (`MainWindowController`, `StartupWindowController`).
