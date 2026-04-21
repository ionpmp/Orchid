# orchid-app

The Orchid desktop application binary. This crate is the single place that depends on every other `orchid-*` crate and stitches them into a running program: it initialises logging, constructs the runtime, mounts the Slint UI, and drives the main event loop.

Keeping the binary thin (wiring only, no business logic) makes every subsystem independently testable from its own crate.
