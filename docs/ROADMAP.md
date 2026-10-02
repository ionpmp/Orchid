# Orchid Roadmap

Legend: `[~]` in progress · `[ ]` not started.

This file lists **planned** work only. Shipped behavior is in the
[user guide](user/README.md), [admin guide](admin/README.md), and
[`CHANGELOG.md`](../CHANGELOG.md). Last reviewed **2026-09-10** (`0.1.0`
pre-alpha, no tagged release).

`.orchid` spec (implemented Phases 1–5): [`ORCHID_FORMAT.md`](ORCHID_FORMAT.md).

## Current tree (not a backlog)

The desktop binary already includes the workspace shell, cinema control kit,
file manager, viewers (DOCX + native `.orchid`), WebView2 HTML/Browser,
terminal, search, password vault, encryption, managed folders, rclone (RC
keep-alive + CLI transfers), audio/video players, and `orchid-format` /
`orchid-embed`. Treat the guides as the source of truth for “does this exist?”.

---

## Remaining gaps in what already shipped

### i18n

- [~] Keep Fluent key parity across 11 locales
      (`python scripts/i18n_sync_keys.py`)

---

## v2.0

- [ ] TUI mode (ratatui)
- [ ] Mobile companion (Android / iOS)
- [ ] Plugin system (WASM, capability-based)
- [ ] Enterprise edition (centralized management)
