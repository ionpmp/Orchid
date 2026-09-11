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

### Search & embeddings

- [ ] Ship a quantized ONNX sentence model behind `orchid-embed`’s `ort`
      feature (today: `StubEmbedder` in universal-search hybrid)

### Viewers / terminal

- [ ] HDR framebuffer for images (Slint remains 8-bit RGBA)
- [ ] PDF annotations / forms / text selection
- [ ] Terminal inline graphics (sixel + kitty) and optional
      `alacritty_terminal` grid — v1.x

### Network & settings

- [ ] In-app OAuth wizard (Drive / OneDrive / Dropbox). Today: named
      `rclone-remote` only
- [ ] Pen + haptic stack (`haptic-feedback`, `palm-rejection`,
      `pen-double-tap-action`). Keys stay in `config.toml`; Settings hides
      the dead rows.
- [ ] Auto-update and telemetry pipelines (keys exist; UI shows Disabled)

### Notifications & storage

- [ ] OS (Windows) toasts — in-app center already exists
- [ ] Reflink / NTFS hardlink ingest for managed folders

### i18n

- [~] Keep Fluent key parity across 11 locales
      (`python scripts/i18n_sync_keys.py`)

---

## v1.x

- [ ] AI agents (Ollama + OpenAI API) on `BackgroundJobQueue`
- [ ] Photo library intelligence (hierarchical tags, opt-in auto-tag,
      **People** view / faces, events, smart albums)
- [ ] Graphical resource monitor with history
- [ ] Theme and widget marketplace
- [ ] Terminal sixel + kitty; optional `alacritty_terminal`
- [ ] Auto-update; opt-in telemetry (off by default)
- [ ] OS notification toasts
- [ ] In-app cloud OAuth wizard
- [ ] ORT embeddings in production search
- [ ] Managed-folder reflink / hardlink ingest

## v2.0

- [ ] Optional replace of `Winlogon\Shell`
- [ ] TUI mode (ratatui)
- [ ] Mobile companion (Android / iOS)
- [ ] Plugin system (WASM, capability-based)
- [ ] Enterprise edition (centralized management)
