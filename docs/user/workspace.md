# Workspace

The main window is a **canvas** of widgets plus overlays (palette, settings,
notifications, catalog). Chrome uses the **cinema** control kit (touch-sized
targets, glass cards, density-aware metrics) derived from theme tokens.

## Grid and density

Widgets sit on a **16×10** grid. Density (Settings → Appearance): **Touch**,
**Mouse**, or **Hybrid** (default; also scales slightly with canvas width).

## Catalog, dock, groups

- **Catalog** lists built-in types. **Document Editor** and **Media Player**
  launchers open a **Viewer**, not extra `type_id`s.
- Bottom edge-swipe shows the **dock**.
- Drop one widget **header onto another** to stack tabs. **Alt+drag**
  detaches. Inactive group tabs sleep.

## Floating windows

Undock to a floating overlay (soft cap **8**): minimize / maximize / restore,
in-app edge snap, taskbar, **Ctrl+Tab**. Drag the header onto a free grid
cell to redock. Viewers from the file manager usually start floating.

## Workspaces

Up to **nine**. Switch from the orb / panel, edge swipes, shortcuts, or
leader-key `n` / `b`. Hidden workspaces sleep their widgets.

## Overlays

| Overlay | How to open |
|---------|-------------|
| Command palette | `Ctrl+Shift+P` |
| Settings | `Ctrl+,` |
| Notification center | Edge swipe (right by default) |
| Workspace panel | Edge swipe (left by default) |
| Universal search | Top edge swipe |
| Hint mode | `Win+?` |

Three-finger swipe up → `widget show all`. Four-finger left/right switches
workspaces. Sides swap for left-hand / `mirror-edge-swipes`.

In-app notifications persist in redb (soft cap ~50). Settings → **Windows
notifications** (`[general].os-notifications`, off by default) also sends
each new item to the Windows Action Center. Unpackaged builds register a
Start Menu shortcut so the toast can carry Orchid’s AppUserModelID. If the
OS rejects the toast, the in-app center still keeps it.

## Keyboard and accessibility

Cinema kit buttons (`TouchButton`, `IconButton`, chips, switches, document
toolbar) take **Tab** focus, show a focus ring, and activate with
**Space** or **Enter**. Icon-only chrome uses the same string as the
tooltip for the accessible name (dock / undock / minimize / maximize /
restore / close / settings). Slint `accessible-*` roles are set on those
controls; the AccessKit backend stays off on Windows (winit focus panic).
