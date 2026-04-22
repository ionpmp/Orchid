# orchid-ui

Slint-based UI layer for Orchid. The crate currently hosts the
**renderer-agnostic half** of the workspace dashboard:

- **Widget framework bridge** (`widgets::view`):
  - `WidgetView` / `WidgetViewDispatcher` — dispatch a
    `orchid_widgets::WidgetSnapshot` to a Slint-ready `SlintPayload` per
    type.
- **Terminal widget** (`widgets::terminal`):
  - `TerminalWidget` — the `orchid_widgets::Widget` implementation that
    wraps an `orchid_terminal::TerminalSession`, produces a
    `WidgetPayload::Terminal` snapshot every frame, and round-trips its
    state (backend kind + cwd hint + title) through bincode.
  - `terminal_descriptor(deps)` — ready-to-register
    `WidgetDescriptor` for the `"terminal"` type.
  - `TerminalWidgetView` — dispatcher adapter.
  - Plus the previous helpers (`palette_from_flavor`,
    `snapshot_to_cells`, `ArboardClipboard`) used by the UI renderer
    once it lands.

## What's **not** here yet

The Slint workspace shell — `MainWindow`, `WorkspaceView`,
`WidgetFrame`, `WidgetDock`, `WorkspaceSwitcher`, drag / resize state
machines, context menus — and the `OrchidApp::bootstrap` that mounts
them still depend on UI-shell infrastructure that has not been built in
this workspace (shared `Theme` global, `LocaleManager`,
`StartupWindowController`). That work is staged as a dedicated task;
this crate exposes a clean integration surface for it:

- plug a `WidgetManager` / `WorkspaceManager` / `GroupManager` into the
  controller the UI task adds,
- register `terminal_descriptor(deps)` on startup,
- wire `WidgetViewDispatcher::render(snapshot)` to push payloads into
  whatever Slint model the workspace view uses.
