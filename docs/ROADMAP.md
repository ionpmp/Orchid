# Orchid Roadmap

Legend: `[~]` in progress · `[ ]` not started.

This file lists **planned** work only. Shipped behavior is in the
[user guide](user/README.md), [admin guide](admin/README.md), and
[`CHANGELOG.md`](../CHANGELOG.md). Last reviewed **2026-10-05** (`0.1.0`
pre-alpha, no tagged release).

`.orchid` spec (implemented Phases 1–5): [`ORCHID_FORMAT.md`](ORCHID_FORMAT.md).

## Current tree (not a backlog)

The desktop binary already includes the workspace shell, cinema control kit,
file manager (Photos, cloud sign-in, managed-folder clone), viewers (DOCX,
spreadsheets, PDF forms, native `.orchid`), WebView2 HTML/Browser/Mail,
terminal (built-in and Alacritty grids), hybrid search, the agent, mail,
calendar (CalDAV), contacts (CardDAV), Optimize, Protection, the password
vault, encryption, rclone (RC keep-alive + CLI transfers), audio/video
players, policy locks, an optional per-user sign-in shell, text mode, and
`orchid-format` / `orchid-embed` / `orchid-mail`. Treat the guides as the
source of truth for “does this exist?”.

---

## Remaining gaps in what already shipped

### i18n

- [~] Keep Fluent key parity across 11 locales
      (`python scripts/i18n_sync_keys.py`)

---

## v2.0

- [ ] Mobile companion (Android / iOS)
- [ ] Plugin system (WASM, capability-based)
