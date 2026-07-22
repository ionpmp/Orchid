# Contributing to Orchid

Thank you for your interest in the project! Please read this document before contributing.

## Code of Conduct

Be respectful. We are building a product for a diverse audience and expect the same from contributors. See [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## How to Help

### Report a Bug

1. Check that there is no existing issue for it
2. Open a new issue using the "Bug Report" template
3. Include your Windows version, Orchid version, and reproduction steps

### Propose a Feature

1. Open a Discussion to evaluate fit and design
2. Once aligned, file an issue using the "Feature Request" template

### Submit Code

1. Fork the repository
2. Create a branch: `git checkout -b feat/my-feature` or `fix/issue-123`
3. Write code following the guidelines below
4. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
5. Open a Pull Request describing your changes

## Code Standards

### Rust

- **Edition:** 2021
- **MSRV:** 1.97
- **Formatter:** `rustfmt` with settings from `rustfmt.toml`
- **Linter:** `clippy` with `-D warnings`
- **Naming:** idiomatic Rust (snake_case for functions/modules, PascalCase for types)

### Commits

We use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` — new functionality
- `fix:` — bug fix
- `docs:` — documentation changes
- `refactor:` — refactoring without behavior change
- `perf:` — performance optimization
- `test:` — adding or changing tests
- `chore:` — dependency updates, infrastructure
- `build:` — build system changes

Example: `feat(terminal): add sixel graphics support`

### Pull Requests

- One PR = one logical task
- Description: what changes and why
- If UX changes — attach screenshots or video
- Link PRs to issues: `Closes #123`

### Tests

- Unit tests — alongside code, in `#[cfg(test)] mod tests`
- Integration tests — in `tests/` of each crate
- UI tests — in `crates/orchid-ui/tests/`

## Architecture

See [`ARCHITECTURE.md`](ARCHITECTURE.md). Discuss major changes in an issue or discussion before implementing.

## Extension checklists

Use these when adding a locale, theme, or widget. Keep changes scoped to one
logical task per pull request.

### How to add a locale

1. Copy `crates/orchid-i18n/locales/en-US/main.ftl` to
   `crates/orchid-i18n/locales/<tag>/main.ftl` (use a valid BCP-47 tag, e.g.
   `nl-NL`).
2. Translate every message id; keep placeholder names (`{ $version }`, etc.)
   identical to the English source. Message IDs must be Fluent-legal
   (`[a-zA-Z][a-zA-Z0-9_-]*` only — no dots).
3. Register the catalogue in `crates/orchid-i18n/src/lib.rs`:
   - add a `const <TAG>_FTL: &str = include_str!(...)` entry,
   - append `("<tag>", <TAG>_FTL)` to the array inside
     `LocaleManager::new`.
4. Run `cargo test -p orchid-i18n` and spot-check strings in the settings
   panel or startup window.
5. Run `python scripts/i18n_sync_keys.py` to confirm every locale still
   matches the en-US key set (exits non-zero when keys are missing).
6. *(Optional, no rebuild required)* Users can also drop overrides at
   `{config_dir}/locales/<tag>/main.ftl`; those files overlay the bundled
   catalogue at runtime.

### How to add a theme

**Built-in (bundled):**

1. Add a `*_theme()` factory in `crates/orchid-ui/src/theme/bundled.rs`
   returning a `Theme` with a unique `meta.id`, display name, `is_dark`, and
   a full `DesignTokens` set.
2. Append the factory to `all_bundled_themes()` in the same file.
3. Set `appearance.theme` in `config.toml` to the new id and verify hot-reload
   (or restart once).

**User-supplied (JSON in `themes_dir`):**

1. Author a JSON file matching the `ThemeDocument` schema in
   `crates/orchid-ui/src/theme/loader.rs` (metadata + `tokens.color` hex
   strings; typography / spacing / radius default when omitted).
2. Place it under `OrchidPaths::themes_dir` (created on first run, typically
   `{config_dir}/themes/<id>.json`). [`ThemeManager::new`] loads every valid
   `.json` in that directory at startup.
3. Reference the theme id in `config.toml` under `[appearance]`.

Bundled themes are preferred for contributions that ship with Orchid; JSON
themes are ideal for user-specific palettes that do not require a rebuild.

### How to add a widget

A widget spans three crates. Follow an existing built-in (e.g. `weather`) as a
template.

1. **`orchid-widgets` — logic and lifecycle**
   - Add `crates/orchid-widgets/src/builtin/<name>/` with a `Widget`
     implementation, config/state types, and a `descriptor()` returning
     `WidgetDescriptor` (stable `type_id`, i18n keys, factory).
   - Export the module from `builtin/mod.rs` if needed.
   - Extend `WidgetPayload` / snapshot types when the widget needs a
     structured payload.
   - Register the descriptor in `OrchidApp::bootstrap`
     (`crates/orchid-ui/src/app.rs`).

2. **`orchid-i18n` — strings**
   - Add `widget-<name>-name`, `widget-<name>-desc`, and any widget-specific
     keys to every `locales/*/main.ftl` (at minimum `en-US`).

3. **`orchid-ui` — Slint surface**
   - Add a Slint component under `ui/widgets/` (import `Theme` from
     `theme_global.slint`).
   - Define a model struct in `ui/workspace/defs.slint` if the widget needs
     typed fields beyond generic text rows.
   - Wire the component into `ui/workspace/widget-frame.slint`.
   - In `MainWindowController` (`src/window/main_window.rs`):
     - add a `build_<name>_model` helper,
     - include the type in `is_known_widget_type` and `dock_types_vec`,
     - handle any widget-specific callbacks.
   - *(Optional)* Implement `WidgetView` and register it on a
     `WidgetViewDispatcher` when the default `SlintPayload::from_widget`
     conversion is not enough (see `TerminalWidgetView`).

4. **Tests** — add or extend integration tests under
   `crates/orchid-ui/tests/` and `crates/orchid-widgets/tests/` as appropriate.

## License

By submitting code, you agree that it will be distributed under AGPL-3.0 (see [`LICENSE`](../LICENSE)).
