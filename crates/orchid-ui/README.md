# orchid-ui

Slint-based UI layer for Orchid. The crate hosts the desktop shell — window
controllers, the compiled Slint component tree, theme/locale globals, and the
bridge between `orchid-widgets` snapshots and Slint models.

## Application entry

- **`OrchidApp`** (`app`) — composition root. [`OrchidApp::bootstrap`] wires
  config, storage, the event bus, [`LocaleManager`], [`ThemeManager`],
  `WidgetManager`, `WorkspaceManager`, terminal sessions, and every built-in
  widget descriptor. [`OrchidApp::run_startup`] opens the onboarding window;
  [`OrchidApp::run_main`] opens the workspace dashboard.
- **Config hot-reload** — a `ConfigWatcher` subscription sets a flag on
  [`ConfigUpdated`]; the next UI tick in [`MainWindowController`] re-applies
  theme, locale, density, shortcut overrides, and rebuilds the workspace model.

## Slint shell (46+ components)

All UI is compiled from `ui/main.slint` via `build.rs`. The tree currently
includes 39 exported components and 7 shared globals (46 Slint units total):

- **Windows** — `MainWindow` (workspace + onboarding modes via `AppState.mode`);
  `StartupWindow` is a legacy alias of the same component
- **Globals** — `Theme`, `Strings`, `AppState`, `WidgetCatalog`,
  `CommandPaletteGlobal`, `SettingsGlobal`, `NavigationGlobal`
- **Workspace shell** — `WorkspaceView`, `WidgetFrame`, `WidgetDock`,
  `WorkspaceSwitcher`, `WidgetCatalogPanel`, `TerminalView`
- **Overlays** — `CommandPalettePanel`, `SettingsPanel`, `WorkspacePanel`,
  `NotificationCenter`
- **Built-in widgets** — terminal (tabs + split view), weather, moon, system,
  RSS, recent files, universal search, media player, password manager, viewer
  (image / PDF / text / archive), file manager (panes, sidebar, dialogs, …)

Every component reads design tokens from the shared [`Theme`] global and user
strings from [`Strings`] (populated from `orchid-i18n`).

## Window controllers

- **`MainWindowController`** — owns the generated `MainWindow` handle, wires
  every Slint callback, drives drag / resize state machines, rebuilds the
  workspace model from `WidgetSnapshotCache`, and syncs terminal raster output.
- **`StartupWindowController`** — thin wrapper around the same `MainWindow`
  Slint type; applies `Theme` / `Strings` / `AppState` once for the
  first-run / empty-state flow (`AppState.mode == 0`).

## Widget integration

- **`widgets::view`** — [`WidgetView`] / [`WidgetViewDispatcher`] convert a
  [`orchid_widgets::WidgetSnapshot`] into a renderer-facing [`SlintPayload`].
  Most types use the default [`SlintPayload::from_widget`] path; specialised
  views (e.g. terminal) override [`WidgetView::render`].
- **`widgets::terminal`** — full terminal stack:
  - `TerminalWidget` — `orchid_widgets::Widget` implementation wrapping an
    `orchid_terminal::TerminalSession`.
  - `terminal_descriptor(deps)` — ready-to-register `WidgetDescriptor`.
  - `TerminalWidgetView` — dispatcher adapter.
  - Helpers — `palette_from_theme`, `snapshot_to_cells`, `ArboardClipboard`.

The controller also maps framework payloads into typed Slint models
(`WeatherModel`, `FileManagerModel`, …) defined in `ui/workspace/defs.slint`
and rendered inside `WidgetFrame`.

## Theming

[`ThemeManager`] registers nine bundled colour themes (`orchid-dark` default,
`orchid-light`, Solarized, Nord, Catppuccin, high-contrast, … — see
`theme/bundled.rs`) plus any valid `.json` files under
`OrchidPaths::themes_dir`. [`MainWindowController::apply_theme`] pushes the
active tokens into the Slint `Theme` global (colours, typography, spacing,
radius). Changing `appearance.theme` in `config.toml` hot-reloads on the next
UI tick.

## Localization

[`LocaleManager`] (from `orchid-i18n`) resolves every UI string. The controller
calls [`LocaleManager::tr`] / [`tr_args`] when building `Strings`, dock labels,
settings fields, and widget catalog entries. Language changes via config reload
are applied without restarting the window.

## Example

```rust,no_run
use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;

# async fn demo() -> orchid_ui::Result<()> {
let paths = OrchidPaths::resolve()?;
let app = OrchidApp::bootstrap(paths).await?;
// app.run_startup()?;   // first-run window
// app.run_main()?;      // workspace dashboard
# Ok(())
# }
```
