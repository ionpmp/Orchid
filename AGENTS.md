# Agent guide for Orchid

Short checklist for coding agents. Full contributor docs:
[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md).

## Documentation

| Audience | Location |
|----------|----------|
| Users | [`docs/user/`](docs/user/README.md) |
| Operators | [`docs/admin/`](docs/admin/README.md) |
| Planned work only | [`docs/ROADMAP.md`](docs/ROADMAP.md) |
| Crate map | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) |

When you **ship** a feature: document it in user/admin + `CHANGELOG.md`
Unreleased, and **remove** it from the roadmap. Do not describe future
work in the guides.

## Build & test

```bash
cargo fmt
cargo clippy --all-targets -- --deny warnings
cargo test
python scripts/i18n_sync_keys.py
```

MSRV is **1.98**. See [`docs/BUILDING.md`](docs/BUILDING.md) (`pdfium.dll`,
libmpv, WebView2).

## i18n / widgets / themes

- Fluent: `crates/orchid-i18n/locales/en-US/main.ftl` + 11 locales
- Widgets: `crates/orchid-widgets/src/builtin/<name>/` + Slint +
  `OrchidApp::bootstrap` when extra deps are required
- Themes: `crates/orchid-ui/src/theme/bundled.rs` + JSON in `themes_dir`
- Checklists: [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md)

## Scope hygiene

Small focused diffs. Do not commit secrets or local `pdfium.dll` / libmpv
blobs. Avoid drive-by refactors.
