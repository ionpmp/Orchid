# Orchid Design Philosophy

## Three Representations of One Action

The core idea of Orchid: **a gesture, a command, and a widget are three forms of the same thing**.

- Drag a file with your finger → a file-manager action runs (`fs.move` /
  copy; not every FM action is an `orc fs …` verb today)
- Type `orc widget create weather` in the palette → a widget appears
- Tap a widget control → the same action path a shortcut would use

This gives three levels of mastery over the system:
1. **Beginner** — taps the screen
2. **Experienced user** — uses gestures and shortcuts
3. **Expert** — writes `orc …` commands and remaps actions

## Touch-First, Not Touch-Only

Orchid is built for devices where touch is the primary input (Surface, 2-in-1, tablets), but that does not mean it is bad with a mouse and keyboard. On the contrary: the discipline of touch-first forces us to:

- Design large hit-targets (minimum 48dp in touch mode)
- Treat gestures as first-class input
- Respect the physical thumb-zone (lower third of the screen = comfort zone)
- Provide Density modes for mouse-driven adaptation

Hover and right-click context menus are available with a pointer. Density
does not change because a mouse moved.

## Density Modes

Settings → Appearance chooses the density. The three modes scale the UI:

- **Touch:** 1.2× (48 dp targets at the 40 dp baseline)
- **Hybrid:** 1.0×, the default. Below 1100 px the scale moves from 1.2×
  toward 1.0×. From 1100 px through 1600 px it stays 1.0×. Past 1600 px it
  moves toward 0.8×, reaching mouse scale at 2000 px
- **Mouse:** 0.8×

The choice is stored in `[appearance].density`. It is not inferred from the
last input device.

## Discoverability

Gestures are invisible. This is the central problem of touch-first interfaces. Solutions:

- **Onboarding tour** on first launch (four steps; can be skipped)
- **Hint mode (`Win+?`)** — overlay for gestures on the dock and workspace
- **Command palette** displays the keyboard shortcut for every command
- **One startup tip** in the notification center the first time the window opens

## Screen Zones and Priorities

- **Hot** (lower third, center) — primary actions, dock, command palette
- **Warm** (lower edges) — secondary actions, side panels
- **Neutral** (center-upper) — content display
- **Cold** (upper corners) — statuses, indicators. **No primary actions here.**

## Left-Handed Adaptivity

- `[input].primary-hand` and `mirror-edge-swipes` swap which edge opens the
  workspace panel and the notification center
- Screen zones in `orchid-core` score how comfortable a region is for a
  thumb. There is no separate one-handed layout that shrinks the window
  into a corner, and shortcuts are not mirrored automatically

## Visual Design Principles

- **Calm tech.** No screaming colors, no obtrusive animations.
- **Content over chrome.** Minimum UI frames and panels.
- **Semantic tokens, not colors.** `accent.brand`, not "blue". The cinema
  kit (`kit.slint`) derives surface ramps, control metrics, and glass cards
  from those seven raw theme colours so every theme stays touch-first.
- **System typography.** Segoe UI Variable on Windows 11, Segoe UI on Windows 10.

## Current shell surfaces (pre-alpha)

These are implemented today and should stay consistent with the principles above:

- **Workspace canvas** — 16×10 grid, widget frames, group tab stacks, dock,
  catalog (including Mail, Contacts, Agent, Optimize, Protection)
- **In-app window manager** — floating overlays (soft cap 8), edge snap,
  taskbar, Ctrl+Tab
- **Viewers** — images, PDF, text, archives, spreadsheets, slide cards,
  media, HTML (WebView2), DOCX / `.orchid`
- **Browser and Mail** — WebView2 (browser chrome, and HTML message bodies)
- **Terminal** — built-in VT grid or the optional Alacritty grid
- **Text mode** — `orchid --tui`, no desktop window
- **Overlays** — command palette, settings, notifications, onboarding, hints
- **Cinema kit** — `kit.slint` tokens, Tab focus, Space/Enter activation.
  AccessKit stays off on Windows

Roadmap status: [`ROADMAP.md`](ROADMAP.md). Recent product notes: [`CHANGELOG.md`](../CHANGELOG.md).
