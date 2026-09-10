# orchid-app

Thin `orchid.exe` entry: tracing, `OrchidPaths`, Tokio, Windows `mimalloc`,
`SLINT_BACKEND=winit-skia`, then `orchid_ui::OrchidApp`. Composition lives in
`orchid-ui`. A second process forwards argv paths over a named pipe.

Build: [`docs/BUILDING.md`](../../docs/BUILDING.md). Install:
[`docs/admin/install.md`](../../docs/admin/install.md).
