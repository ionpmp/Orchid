# Orchid Roadmap

Legend: `[~]` in progress · `[ ]` not started.

This file lists **planned** work only. Shipped behavior is documented in the
[user guide](user/README.md), [admin guide](admin/README.md), and
[`CHANGELOG.md`](../CHANGELOG.md). Last reviewed against the tree on
**2026-09-05** (`0.1.0` workspace version, pre-alpha, no tagged release).

Native container spec (design only): [`ORCHID_FORMAT.md`](ORCHID_FORMAT.md).

## Current tree (not a backlog)

The desktop binary already includes a workspace shell, file manager, viewers
(including a Tier-1 DOCX editor and libmpv media), terminal, search, password
vault, encryption, managed folders, rclone mounts, and the built-in widget
set. Treat the guides as the source of truth for “does this exist?”.

Known **gaps in what already shipped** (polish / incomplete surfaces, not
new product lines) are under [MVP remaining](#mvp-remaining--toward-v01).

---

## MVP remaining — toward v0.1

### Password manager

- [ ] Edit existing entries and a dedicated generate-password UI (add-entry
      exists; module comments still mark edit/generate as later work)
- [ ] Group browser in the widget (KDBX groups exist in `orchid-crypto`; new
      entries are created in the root group)

### Viewers

- [ ] Embedded HTML (WebView2). Today: source preview + open in the system
      browser
- [ ] HDR framebuffer presentation for the image viewer (Slint remains 8-bit
      RGBA)
- [ ] PDF polish beyond page nav / fit / zoom (annotations, forms, text
      selection)

### Terminal

- [ ] Inline graphics (sixel + kitty) — deferred; see v1.x
- [ ] Migration to `alacritty_terminal` for vi mode and regex scrollback
      search — deferred; see v1.x

### Network & settings

- [ ] In-app OAuth wizard for Drive / OneDrive / Dropbox. Today: named
      `rclone-remote` in `rclone.conf` only
- [ ] Wire Settings fields that are stored but unused: haptic feedback, palm
      rejection, pen double-tap, first day of week
- [ ] Auto-update and telemetry. Keys exist in `config.toml`; the Settings UI
      shows them as disabled. No updater or telemetry pipeline yet

### Notifications

- [ ] OS (Windows) toasts. In-app notification center with persistence
      already exists

### Storage

- [ ] Reflink / NTFS hardlink strategy for managed-folder dedup so originals
      are not fully mirrored on disk (current design keeps the live file and
      also stores chunks)

### i18n

- [~] Keep Fluent key parity across all 11 locales as new strings land
      (`python scripts/i18n_sync_keys.py`)

---

## v1.x

- [ ] AI agents (Ollama + OpenAI API) on `BackgroundJobQueue` (already used
      for RSS / weather)
- [ ] Photo library intelligence (after AI agents)
  - [ ] Hierarchical tags (`places/italy/rome`, nested sidebar)
  - [ ] Opt-in auto-tagging (scene / objects / caption → tags, review before
        apply)
  - [ ] Face recognition and **People** view (local embeddings only) plus a
        People virtual folder
  - [ ] Event / date clustering and named events
  - [ ] Smart albums as saved-query virtual folders
- [ ] Graphical resource monitor with history (beyond the System / Processes
      widgets)
- [ ] Built-in browser (WebView2)
- [ ] Lua scripting (`mlua`)
- [ ] Theme and widget marketplace
- [ ] Terminal: sixel + kitty graphics; optional `alacritty_terminal` grid
- [ ] Auto-update
- [ ] Opt-in telemetry (off by default)
- [ ] OS notification toasts
- [ ] In-app cloud OAuth wizard
- [ ] Password vault group UI and entry editing
- [ ] Managed-folder reflink / hardlink ingest

## Native format (`.orchid`)

AI-native container (magic `ORCD`) for documents and media wrappers. Spec
first; no crate implements it yet. Details: [`ORCHID_FORMAT.md`](ORCHID_FORMAT.md).

- [ ] Phase 1 — Framing: self-describing regions + FlatBuffers TOC +
      Raw / Clean-Text / Structured (snapshot) + mmap + zstd; sealed mode
      only
- [ ] Phase 2 — Per-region `age` encryption + linked mode (`ChunkStore`) +
      CAS generation history
- [ ] Phase 3 — C2PA provenance region
- [ ] Phase 4 — CRDT structured region (multi-writer human + AI agent)
- [ ] Phase 5 — Local embeddings (`ort`) + hierarchical vectors + ANN hybrid
      search with Tantivy

## v2.0

- [ ] Optional replace of `Winlogon\Shell`
- [ ] TUI mode (ratatui for SSH / low-spec machines)
- [ ] Mobile companion (Android / iOS)
- [ ] Plugin system (WASM, capability-based)
- [ ] Enterprise edition (centralized management)
