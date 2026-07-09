# Agent guide for Orchid

Short checklist for coding agents working in this repository. Full contributor
docs live in [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md).

## Build & test

```bash
cargo fmt
cargo clippy --all-targets -- --deny warnings
cargo test
python scripts/i18n_sync_keys.py
```

Targeted crates when iterating:

```bash
cargo test -p orchid-search
cargo test -p orchid-viewers
cargo test -p orchid-widgets
cargo test -p orchid-storage
cargo test -p orchid-i18n
```

MSRV is **1.82**. Windows + VS Build Tools are required for a full app build;
see [`docs/BUILDING.md`](docs/BUILDING.md) (including `pdfium.dll` for PDF).

## i18n

- Source of truth: `crates/orchid-i18n/locales/en-US/main.ftl`
- Keep key parity across all 11 locales; run `python scripts/i18n_sync_keys.py`
- Prefer Fluent keys in payloads / errors; resolve with `LocaleManager` in UI
- Checklist: [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md#how-to-add-a-locale)

## Widgets

- Logic in `crates/orchid-widgets/src/builtin/<name>/`
- Strings in every `locales/*/main.ftl`
- Slint surface under `crates/orchid-ui/ui/widgets/` + wiring in `main_window.rs`
- Widget **groups** (tab stacks): drop header-on-header to stack; strip actions
  in `ui/workspace/group-tabs.slint`; Alt+drag detaches
- Checklist: [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md#how-to-add-a-widget)

## Themes

- Bundled: `crates/orchid-ui/src/theme/bundled.rs`
- User JSON: `OrchidPaths::themes_dir`
- Checklist: [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md#how-to-add-a-theme)

## Roadmap & architecture

- Backlog: [`docs/ROADMAP.md`](docs/ROADMAP.md) (`[x]` / `[~]` / `[ ]`)
- Crate map: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)

## Scope hygiene

- Prefer small, focused diffs; one logical task per PR
- Do not commit secrets, local `pdfium.dll` blobs, or unrelated scaffolding
- Match existing Rust / Slint style; avoid drive-by refactors
