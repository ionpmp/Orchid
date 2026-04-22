# orchid-ui

Slint-based UI layer for Orchid. This crate currently hosts:

- **Terminal widget helpers** (`widgets::terminal`):
  - `palette::palette_from_flavor` — maps Orchid Dark / Light theme flavours onto `orchid_terminal::TerminalPalette`.
  - `render::snapshot_to_cells` — converts a `GridSnapshot` into a flat grid of renderer-ready cells (RGBA resolved, INVERSE / HIDDEN / DIM honoured).
  - `clipboard::ArboardClipboard` — cross-platform clipboard wrapper that also implements `orchid_crypto::SecureClipboard` for the password-manager auto-clear flow.

The Slint component tree, the window bootstrap, the Theme global, the command-registry integration, and the in-app live terminal view are staged in a follow-up task together with the rest of the UI-shell infrastructure (startup window, theming, density, i18n wiring). The helpers above are complete and fully unit-tested; they stand up independently and let the follow-up task focus on the Slint integration.
