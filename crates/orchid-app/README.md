# orchid-app

Thin `orchid.exe` entry: tracing, `OrchidPaths`, Tokio, Windows `mimalloc`,
`SLINT_BACKEND=winit-skia`, then `orchid_ui::OrchidApp`. Composition lives in
`orchid-ui`. A second process forwards argv paths over a named pipe and exits.

## Arguments

| Invocation | What it does |
|------------|----------------|
| `orchid` | Desktop window |
| `orchid <path>…` | Open those paths. A second process forwards them to the running instance |
| `orchid --tui [path]` | Text-mode folder list. Does not start the desktop window or talk to a running instance. See [`docs/user/tui.md`](../../docs/user/tui.md) |
| `orchid --restore-shell` | Write the previous per-user Winlogon `Shell` back and exit, before the single-instance check |

Build: [`docs/BUILDING.md`](../../docs/BUILDING.md). Install:
[`docs/admin/install.md`](../../docs/admin/install.md).
