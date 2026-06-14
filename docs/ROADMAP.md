# Orchid Roadmap

Legend: `[x]` done · `[~]` in progress · `[ ]` not started.

## MVP (v0.1) — 6–8 months

### Core
- [x] **Workspace structure** (Cargo workspace, 11-crate split, shared workspace deps, dev-fast profile, Slint build wiring)
- [x] **State store & configuration** — `orchid-storage`
  - [x] `redb`-backed `StateStore` with typed `Read`/`Write` transactions
  - [x] Stored value types (`SchemaMeta`, `HistoryEntry`, `WidgetInstance`, `Workspace`, `FileTag`, `SessionState`, `CacheEntry`, …) via `bincode` 2.x
  - [x] Schema versioning + migration engine (`CURRENT_SCHEMA_VERSION = 1`)
  - [x] `HISTORY_BY_TIMESTAMP_INDEX` for ordered history iteration
  - [x] Cache age-eviction primitive (`evict_cache_older_than`)
  - [x] `OrchidConfig` TOML schema, `ConfigLoader` (atomic save + validation)
  - [x] `ConfigWatcher` — debounced hot-reload over `tokio::sync::broadcast`
  - [x] OS-aware paths (`OrchidPaths`) via `directories`
- [x] **Event bus, action system, command registry** — `orchid-core`
  - [x] Priority-ordered multi-producer/consumer `EventBus` (channel / async / sync subscribers, filter by type / source / predicate, slow-consumer policy, metrics, graceful shutdown)
  - [x] `Action` trait, `ActionContext`, `ActionOutcome`, panic-catching `ActionDispatcher` with before/after middleware
  - [x] `HistoryRecorder` middleware (auto-persists every dispatched action into `orchid-storage`, respects `privacy.record_action_history`)
  - [x] `CommandRegistry` + `CommandDescriptor` + `ActionFactory`, shortcut-override batch apply
  - [x] Shell-like `parse_command_line` (quoted strings, `--flag` / `--key=value` / `--key value`, registry-aware multi-word verb resolution)
  - [x] `Shortcut` parser with canonical round-trip + `is_reserved` (`Win+L`, `Win+Space`, `Ctrl+Alt+<letter>`)
  - [x] `CommandPalette` fuzzy search via `nucleo-matcher`
  - [x] Unified `InputEvent` (touch / mouse / keyboard / pen), ergonomic `ScreenZone`s
  - [x] `GestureRecognizer` (tap, double-tap, long-press via `tick`, swipe, edge-swipe, pinch, rotate, pan)
  - [x] `InputMapper` + `default_bindings` for spec-defined edge / multi-finger swipes
- [ ] Minimal Slint + Skia window + theming + i18n infrastructure

### File Manager
- [x] Dual-pane mode
- [x] Views (icons, list, details, gallery)
- [x] Tabs, breadcrumbs
- [ ] Drag-and-drop
- [~] Virtual folders (Recent, Categories, Network) — Recent, Starred, Tags, and category buckets implemented; network mounts pending
- [x] Inline rename, tags, color labels — inline rename in list/grid; tag / colour / star via `orchid-fs::TagManager`
- [x] Quick filter
- [~] Encryption integration — encrypt / decrypt / reveal in UI; age engine wired via `EncryptedFolderEngine`
- [~] Managed folders — add-to-managed action; full dedup policy UI pending

### Terminal
- [x] PTY backend — `orchid-terminal::pty` wraps `portable-pty` with async reader / writer tasks and live resize
- [x] Terminal emulation — custom `vte`-based emulator (SGR, cursor, erase, scroll regions, OSC 0/2/7, DSR). Migration to `alacritty_terminal` for advanced features (vi mode, regex scrollback search) is planned for v1.x
- [~] Tabs + splits — `orchid-terminal::layout` data model complete; UI rendering pending the Slint terminal view
- [x] PowerShell, cmd, WSL backends — all three plus `Custom` variant covered by `BackendSpec`
- [x] SSH sessions — `SshTarget` parses `ssh://` URIs and produces correct argv (jump hosts, identity files, extra args)
- [ ] Inline graphics (sixel + kitty) — deferred to v1.x

### Widgets
- [~] Infrastructure (layouts, workspaces, lifecycle) — `orchid-widgets` ships the full framework: `Widget` trait, `WidgetRegistry`, `WidgetManager` (create / move / resize / close, idle sweeper, persistence), `WorkspaceManager` (up to 9 workspaces, dense ordinals, switch-next/previous/by-ordinal), `LayoutEngine` (16×10 grid, auto-placement, collision, pixel snapshots), `GroupManager` (tab stacks persisted in a dedicated redb table), framework-wide events, and `build_command_set` of widget / workspace / group commands. `orchid-ui` exposes the renderer-agnostic `WidgetView` / `WidgetViewDispatcher` bridge. The Slint workspace dashboard (drag / resize / dock / switcher, app bootstrap) remains blocked on a dedicated UI-shell task (shared `Theme` global, `LocaleManager`, `StartupWindowController`).
- [ ] Widget: Weather
- [ ] Widget: Moon (astronomy)
- [ ] Widget: System indicators
- [ ] Widget: Files (recent)
- [ ] Widget: Universal search
- [ ] Widget: Media player (audio/video)
- [ ] Widget: RSS feed
- [ ] Widget: Password manager
- [~] Widget: Terminal — `orchid-ui::TerminalWidget` implements the `Widget` trait end-to-end (spawns an `orchid_terminal::TerminalSession`, snapshots the grid into `WidgetPayload::Terminal`, restores after `Unloaded`). Slint terminal surface / dock entry / live painting pending the UI-shell task.

### Viewers
- [ ] Images (PNG, JPEG, WebP, AVIF, HEIC, BMP, GIF, SVG, RAW)
- [ ] PDF (pdfium)
- [ ] Text with syntax highlighting (Tree-sitter)
- [ ] Archives (browse + extract)

### Security
- [~] Password manager (KDBX4 format, custom UX) — KDBX4 read/write, groups / entries / TOTP / search done in `orchid-crypto::kdbx`; widget UI pending
- [~] File and folder encryption (age-based) — engine + file-manager encrypt / decrypt / reveal wired; biometric and polish pending
- [ ] Biometric unlock via Windows Hello

### Storage
- [~] Content-addressed storage (BLAKE3 + FastCDC chunking) — `ChunkStore`, refcount table, orphan GC done in `orchid-crypto::content`; managed-folder policy layer pending in `orchid-fs`
- [~] Deduplication in managed folders — `Deduplicator` + add-to-managed in file manager; full ingest UI pending

### Network Clients
- [ ] SFTP / SMB / WebDAV / FTP via rclone
- [ ] Virtual folders in file manager

### Search
- [x] Tantivy indexing — `orchid-search::SearchEngine` with full schema, batched writer, commit/optimize/shutdown
- [x] File watcher for incremental updates — `IndexFsSubscriber` consumes `fs.created/modified/deleted/renamed/tags_changed` events, extracts text/PDF content, enqueues into `IndexScheduler`
- [~] Universal search (files + commands + settings) — file search complete; command-palette hookup and settings-index integration pending

### UX
- [ ] Theming (light/dark, density modes, hot-reload) — density + theme keys live in `OrchidConfig`; renderer-side wiring pending
- [ ] Built-in themes (Orchid Light/Dark, Solarized, Nord, Catppuccin, High Contrast)
- [ ] Internationalization (11 languages, RTL)
- [ ] Adaptive layouts (profiles for different screens)
- [~] Gestures (touch, pen, mouse) — recogniser + default binding set done in `orchid-core`; real input plumbing pending in `orchid-ui`
- [~] Keyboard shortcuts + leader-key mode — `Shortcut` parsing, reserved-combo detection, and user override application done; leader-key mode + chord tracking pending
- [~] Command palette — fuzzy search engine and result types done in `orchid-core`; palette UI pending in `orchid-ui`
- [ ] Onboarding tour, hint mode

### Additional
- [ ] Jyotish module (optional, not in default widgets)

## v1.x — 9–18 months

- [ ] AI agents (Ollama + OpenAI API)
- [ ] Graphical resource monitor with history
- [ ] Extended notification system
- [ ] Built-in browser (WebView2)
- [ ] Lua scripting (mlua)
- [ ] Theme and widget marketplace

## v2.0 — Year 2

- [ ] Replace Winlogon\Shell as an option
- [ ] TUI mode (ratatui for SSH/low-spec machines)
- [ ] Mobile companion (Android/iOS)
- [ ] Plugin system (WASM, capability-based)
- [ ] Enterprise edition (centralized management)
