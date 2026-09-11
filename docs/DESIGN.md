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

When a mouse is in use, density switches to a compact mode, hover effects appear, and right-click context menus become available.

## Density Modes

- **Touch:** 48dp targets, larger fonts
- **Mouse:** 32dp targets, denser layout
- **Hybrid:** 40dp, the middle ground (default for 2-in-1)

Switches automatically based on detected input type, or manually.

## Discoverability

Gestures are invisible. This is the central problem of touch-first interfaces. Solutions:

- **Onboarding tour** on first launch
- **Hint mode (`Win+?`)** — overlay showing all gestures available in the current context
- **Command palette** displays the keyboard shortcut for every command
- **Periodic tips** in the notification center

## Screen Zones and Priorities

- **Hot** (lower third, center) — primary actions, dock, command palette
- **Warm** (lower edges) — secondary actions, side panels
- **Neutral** (center-upper) — content display
- **Cold** (upper corners) — statuses, indicators. **No primary actions here.**

## Left-Handed Adaptivity

- Edge swipes are configurable (sides can be swapped)
- All keyboard shortcuts have mirror versions
- One-handed mode shrinks the interface toward a chosen corner

## Visual Design Principles

- **Calm tech.** No screaming colors, no obtrusive animations.
- **Content over chrome.** Minimum UI frames and panels.
- **Semantic tokens, not colors.** `accent.brand`, not "blue". The cinema
  kit (`kit.slint`) derives surface ramps, control metrics, and glass cards
  from those seven raw theme colours so every theme stays touch-first.
- **System typography.** Segoe UI Variable on Windows 11, Segoe UI on Windows 10.

## Current shell surfaces (pre-alpha)

These are implemented today and should stay consistent with the principles above:

- **Workspace canvas** — 16×10 grid, widget frames, group tab stacks, dock
- **In-app window manager** — floating overlays, edge snap, taskbar, Ctrl+Tab
- **Viewers** — images, PDF, text, archives, media, HTML (WebView2), DOCX / `.orchid`
- **Browser widget** — WebView2 tabs
- **Overlays** — command palette, settings, notifications, onboarding, hints

Roadmap status: [`ROADMAP.md`](ROADMAP.md). Recent product notes: [`CHANGELOG.md`](../CHANGELOG.md).
