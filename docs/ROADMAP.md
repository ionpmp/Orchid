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
- [x] Drag-and-drop — folder rows, breadcrumbs, cross-pane, OS drop (move/copy with Ctrl), FM→viewer content zone, cross-widget FM move, transfer progress + failure toast; canvas + FM content-zone hit-test; wheel-scroll during drag
- [x] Virtual folders (Recent, Categories, Network) — Recent, Starred, Tags, categories; localized breadcrumbs + empty states; network mounts from config.toml with rclone browse/write + `copyto`/`moveto` fast paths
- [x] Inline rename, tags, color labels — inline rename in list/grid; tag / colour / star via `orchid-fs::TagManager`
- [x] Quick filter
- [x] Encryption integration — encrypt / decrypt / reveal in UI; localized passphrase dialog + status toasts; retry on wrong password; age engine via `EncryptedFolderEngine`
- [x] Managed folders — sidebar with ingest stats, localized ingest failure toast, status bar stats, in-flight indicator + toast, add/remove in context menu

### Terminal
- [x] PTY backend — `orchid-terminal::pty` wraps `portable-pty` with async reader / writer tasks and live resize
- [x] Terminal emulation — custom `vte`-based emulator (SGR, cursor, erase, scroll regions, OSC 0/2/7, DSR). Migration to `alacritty_terminal` for advanced features (vi mode, regex scrollback search) is planned for v1.x
- [x] Tabs + splits — tab strip, split panes (▥/▤), draggable dividers, pane focus/close, keyboard shortcuts
- [x] PowerShell, cmd, WSL backends — all three plus `Custom` variant covered by `BackendSpec`
- [x] SSH sessions — `SshTarget` parses `ssh://` URIs and produces correct argv (jump hosts, identity files, extra args)
- [ ] Inline graphics (sixel + kitty) — deferred to v1.x

### Widgets
- [~] Infrastructure (layouts, workspaces, lifecycle) — `orchid-widgets` ships the full framework: `Widget` trait, `WidgetRegistry`, `WidgetManager` (create / move / resize / close, idle sweeper, persistence), `WorkspaceManager` (up to 9 workspaces, dense ordinals, switch-next/previous/by-ordinal), `LayoutEngine` (16×10 grid, auto-placement, collision, pixel snapshots), `GroupManager` (tab stacks persisted in a dedicated redb table), framework-wide events, and `build_command_set` of widget / workspace / group commands. `orchid-ui` exposes the renderer-agnostic `WidgetView` / `WidgetViewDispatcher` bridge. The Slint workspace dashboard (drag / resize / dock / switcher, app bootstrap) remains blocked on a dedicated UI-shell task (shared `Theme` global, `LocaleManager`, `StartupWindowController`).
- [ ] Widget: Weather
- [ ] Widget: Moon (astronomy)
- [ ] Widget: System indicators
- [x] Widget: Files (recent) — shared MRU store, dock widget, FM virtual Recent folder
- [~] Widget: Universal search — debouncer + aggregator wired; UI patch-on-update (no per-keystroke rebuild)
- [ ] Widget: Media player (audio/video)
- [x] Widget: RSS feed
- [~] Widget: Password manager — unlock UI (passphrase + Hello), search, copy, TOTP, add entry; lock vault button + command
- [x] Widget: Terminal — end-to-end with tab strip, split panes, draggable dividers, shortcuts, live raster, persisted layout

### Viewers
- [~] Images (PNG, JPEG, WebP, AVIF, HEIC, BMP, GIF, SVG, RAW) — `ImageViewer` + zoom/pan/rotate in viewer widget; HEIC/SVG/RAW pending
- [~] PDF (pdfium) — Pdfium-backed viewer with page navigation, fit width/page, zoom; requires bundled `pdfium.dll`
- [~] Text with syntax highlighting (Tree-sitter) — `TextViewer` + `SyntaxHighlighter` in viewer widget; edit mode pending
- [~] Archives (browse + extract) — browse + preview wired; extract selected/all to sibling folder in viewer toolbar

### Security
- [~] Password manager (KDBX4 format, custom UX) — vault unlock/lock UI (passphrase + Windows Hello); KDBX4 read/write, groups / entries / TOTP / search in `orchid-crypto::kdbx`
- [~] File and folder encryption (age-based) — engine + file-manager encrypt / decrypt / reveal wired; localized passphrase UX + Windows Hello on FM passphrase dialog
- [x] Biometric unlock via Windows Hello — password vault + FM encrypted-folder passphrase via DPAPI

### Storage
- [~] Content-addressed storage (BLAKE3 + FastCDC chunking) — `ChunkStore`, refcount table, orphan GC done in `orchid-crypto::content`; managed-folder policy layer pending in `orchid-fs`
- [~] Deduplication in managed folders — `Deduplicator` + add-to-managed in file manager; ingest failure UX + sidebar stats in UI

### Network Clients
- [~] SFTP / SMB / WebDAV / FTP via rclone — browse + read/write via `RcloneProvider`; credentials via config or rclone.conf remote; network virtual folder in FM sidebar

### Search
- [x] Tantivy indexing — `orchid-search::SearchEngine` with full schema, batched writer, commit/optimize/shutdown
- [x] File watcher for incremental updates — `IndexFsSubscriber` consumes `fs.created/modified/deleted/renamed/tags_changed` events, extracts text/PDF content, enqueues into `IndexScheduler`
- [~] Universal search (files + commands + settings) — settings sections open read-only config panel; full editor pending

### UX
- [~] Theming (light/dark, density modes, hot-reload) — theme, locale, and density hot-reload from config.toml
- [ ] Built-in themes (Orchid Light/Dark, Solarized, Nord, Catppuccin, High Contrast)
- [ ] Internationalization (11 languages, RTL)
- [ ] Adaptive layouts (profiles for different screens)
- [~] Gestures (touch, pen, mouse) — recogniser + default binding set done in `orchid-core`; real input plumbing pending in `orchid-ui`
- [x] Keyboard shortcuts + leader-key mode — `Shortcut` parsing, reserved-combo detection, user override application, and configurable leader-key chord dispatch (`Ctrl+Shift+Space` + letter)
- [x] Command palette — Ctrl+Shift+P overlay with fuzzy search and command dispatch
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
