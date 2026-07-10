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
  - [x] History age-eviction primitive (`evict_history_older_than`, driven by `privacy.history_retention_days` on startup + hot-config change)
  - [x] `OrchidConfig` TOML schema, `ConfigLoader` (atomic save + validation)
  - [x] `ConfigWatcher` — debounced hot-reload over `tokio::sync::broadcast`
  - [x] OS-aware paths (`OrchidPaths`) via `directories`
- [x] **Event bus, action system, command registry** — `orchid-core`
  - [x] Priority-ordered multi-producer/consumer `EventBus` (channel / async / sync subscribers, filter by type / source / predicate, slow-consumer policy, metrics, graceful shutdown)
  - [x] `Action` trait, `ActionContext`, `ActionOutcome`, panic-catching `ActionDispatcher` with before/after middleware
  - [x] `HistoryRecorder` middleware (auto-persists every dispatched action into `orchid-storage`, respects `privacy.record_action_history`; wired on `MainWindowController` `ActionDispatcher` bootstrap + hot-config toggle; `privacy.history_retention_days` pruning on startup and when retention changes)
  - [x] `CommandRegistry` + `CommandDescriptor` + `ActionFactory`, shortcut-override batch apply
  - [x] Shell-like `parse_command_line` (quoted strings, `--flag` / `--key=value` / `--key value`, registry-aware multi-word verb resolution)
  - [x] `Shortcut` parser with canonical round-trip + `is_reserved` (`Win+L`, `Win+Space`, `Ctrl+Alt+<letter>`)
  - [x] `CommandPalette` fuzzy search via `nucleo-matcher`
  - [x] Unified `InputEvent` (touch / mouse / keyboard / pen), ergonomic `ScreenZone`s
  - [x] `GestureRecognizer` (tap, double-tap, long-press via `tick`, swipe, edge-swipe, pinch, rotate, pan)
  - [x] `InputMapper` + `default_bindings` for spec-defined edge / multi-finger swipes
- [x] Minimal Slint + Skia window + theming + i18n infrastructure

### File Manager
- [x] Dual-pane mode
- [x] Views (icons, list, details, gallery)
- [x] Tabs, breadcrumbs
- [x] Drag-and-drop — folder rows, breadcrumbs, cross-pane, OS drop (move/copy with Ctrl), FM→viewer content zone (multi-file opens a viewer per path, soft cap 8 + one rebuild + auto-place), cross-widget FM move, transfer progress + failure toast; canvas + FM content-zone hit-test; wheel-scroll during drag; Enter/single-click open uses real `is_dir` from FM snapshot
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
- [x] PTY child tree cleanup — Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) on spawn
- [ ] Inline graphics (sixel + kitty) — deferred to v1.x

### Widgets
- [x] Infrastructure (layouts, workspaces, lifecycle) — `orchid-widgets` ships the full framework: `Widget` trait, `WidgetRegistry`, `WidgetManager` (create / move / resize / close, idle sweeper, persistence), `WorkspaceManager` (up to 9 workspaces, dense ordinals, switch-next/previous/by-ordinal), `LayoutEngine` (16×10 grid, auto-placement, collision, pixel snapshots), `GroupManager` (tab stacks persisted in a dedicated redb table), framework-wide events, and `build_command_set` of widget / workspace / group commands. `orchid-ui` exposes the renderer-agnostic `WidgetView` / `WidgetViewDispatcher` bridge and the Slint workspace dashboard (switcher, layout grid, drag/resize with snap ghost + collision feedback, dock show/hide + hover animations, group tab strip with drag-to-stack / switch / reorder / dissolve / Alt-drag detach).
- [x] Widget: Weather
- [x] Widget: Moon (astronomy)
- [x] Widget: System indicators
- [x] Widget: Files (recent) — shared MRU store, dock widget, FM virtual Recent folder
- [x] Widget: Universal search — debouncer + aggregator wired; UI patch-on-update (no per-keystroke rebuild)
- [x] Widget: Media player (audio/video)
- [x] Widget: RSS feed
- [x] Widget: Password manager — unlock UI (passphrase + Hello), search, copy, TOTP, add entry; lock vault button + command
- [x] Widget: Terminal — end-to-end with tab strip, split panes, draggable dividers, shortcuts, live raster, persisted layout

### Viewers
- [~] Images (PNG, JPEG, WebP, AVIF, HEIC, BMP, GIF, SVG, RAW) — `ImageViewer` + zoom/pan/rotate/flip with active flip accent; keyboard shortcuts when focused (+/− zoom, F fit, 1 actual, arrows pan, R/Shift+R rotate, H/V flip); localized Fit/Actual Size with active-mode highlight + status strip + toolbar hover hints; viewport re-fit while in fit mode; SVG via `resvg`; HEIC/RAW route to Image with clear unsupported message (native decode pending)
- [~] PDF (pdfium) — Pdfium-backed viewer with page navigation (toolbar + PageUp/Down/←/→ when focused), go-to-page input, fit width/page with active-mode highlight, zoom, toolbar hover hints, viewport re-fit; action failures (page/zoom/fit/viewport) surface as localized notifications; requires bundled `pdfium.dll`
- [~] Text with syntax highlighting (Tree-sitter) — grammars for rust/python/toml/json/markdown/javascript/typescript/tsx/yaml/go/bash/html/css/c/cpp/java/ruby/sql/php/kotlin; MVP edit mode (toggle, multiline edit, save via toolbar/Ctrl+S with hover hints, dirty ●, localized line count + LF/CRLF); read-only virtualized scroll (Flickable → text_scroll + viewport-sized window)
- [~] Archives (ZIP, 7z, TAR, TAR.GZ, TAR.XZ) — browse + preview + extract selected/all; localized toolbar header + status strip (format/count + extract feedback); navigate/select/extract failures notify; TAR.XZ via `xz2`

### Security
- [~] Password manager (KDBX4 format, custom UX) — unlock/lock UI + Hello; KDBX4 R/W, groups/entries/TOTP; `privacy.vault_auto_lock_seconds` idle lock (default 300s)
- [~] File and folder encryption (age-based) — engine + file-manager encrypt / decrypt / reveal wired; localized passphrase UX + Windows Hello on FM passphrase dialog
- [x] Biometric unlock via Windows Hello — password vault + FM encrypted-folder passphrase via DPAPI

### Storage
- [x] Content-addressed storage (BLAKE3 + FastCDC chunking) — `ChunkStore`, refcount table, orphan GC; managed-folder policy (`exclude_patterns`, quota, retention) in `orchid-fs`
- [~] Deduplication in managed folders — `Deduplicator` + add-to-managed in file manager; ingest failure UX + sidebar stats; policy dialog in FM

### Network Clients
- [~] SFTP / SMB / WebDAV / FTP via rclone — browse + read/write via `RcloneProvider`; credentials via config or rclone.conf remote; network virtual folder in FM sidebar

### Search
- [x] Tantivy indexing — `orchid-search::SearchEngine` with full schema, batched writer, commit/optimize/shutdown
- [x] File watcher for incremental updates — `IndexFsSubscriber` + `FileWatcher` on `[search].included-roots` (default: Documents), bootstrap crawl, text/PDF extract → `IndexScheduler` (wired in `OrchidApp::bootstrap`)
- [x] Universal search (files + commands + settings) — live settings editor for theme/locale/density/bools; complex shortcut/leader fields stay read-only; search debouncer hardening + `SEARCH_LIVE` miss metrics; file hits show Tantivy content snippets in the subtitle when available

### UX
- [x] Theming (light/dark, density modes, hot-reload) — theme, locale, and density hot-reload from config.toml (main window + startup window)
- [x] Built-in themes (Orchid Light/Dark, Solarized, Nord, Catppuccin, High Contrast) — nine bundled themes + JSON loader from `themes_dir`
- [~] Internationalization (11 languages, RTL) — 11 Fluent catalogues bundled (`en-US`…`ar-SA`); widget titles + catalog/dock descriptions (label+desc hover, label-only dock tiles), FM/viewer sizes, System uptime/battery charging·time + severity hover hints, Properties/Details/delete confirm, search empty states + candidate/title hover (BMP source glyphs), startup status strip (localized theme display names + locale endonyms), FM Home/loading/empty/access + encrypted/managed entry hints + OS I/O error mapping (translated) + elided filename/status-bar hover + disabled back/forward + sort-header hints + text-only pane errors + sidebar toggle active accents + BMP sidebar glyphs, settings shortcuts (resolved command names)/coming-soon/disabled/default placeholders + section/field-label hover + localized config-reload/field-reject/save failure notifications, TOTP remaining, PDF page-of/Go + text-only unavailable, archive extract/parent/path/entry/info-banner hover + icon-driven BMP glyphs + text-only binary preview, weather location/status + BMP condition glyphs + forecast range/precip Fluent + search/onboarding/dialog hover chrome, moon BMP phase glyphs + value-row hover, viewer unsupported type + text LF/CRLF/syntax/encoding + image/archive format labels + localized loading/error status (no glyph icons) + viewer/password/search/FM error fallthrough translated (IO/unknown mapped), recent-files path + RSS title/summary hover, media track metadata hover + transport failure notifications, notification title elide hover, command palette `orc` invocation, terminal split-drag + password (list + Copy/Open + detail elide/TOTP accent)/catalog/dock/FM/workspace/group chrome hover hints (incl. Alt-detach elide) + widget title elide/resize tooltip localized; S-size RTL mirrors notification/workspace docking when language starts with `ar`
- [~] Adaptive layouts (profiles for different screens) — Hybrid density nudges UI scale from canvas width: below 1100 px toward Touch (1.2×), above 1600 px toward Mouse (0.8×)
- [x] Gestures (touch, pen, mouse) — recogniser + `default_bindings` wired through `orchid-ui` to workspace panel, notification center, dock, and universal search
- [x] Keyboard shortcuts + leader-key mode — `Shortcut` parsing, reserved-combo detection, user override application, and configurable leader-key chord dispatch (`Ctrl+Shift+Space` + letter)
- [x] Command palette — Ctrl+Shift+P overlay with fuzzy search and command dispatch
- [x] Onboarding tour, hint mode — four-step first-run overlay; `Win+?` hint overlays; persisted `[onboarding]` config

### Additional
- [ ] Jyotish module (optional, not in default widgets)

## v1.x — 9–18 months

- [ ] AI agents (Ollama + OpenAI API)
- [ ] Graphical resource monitor with history
- [~] Extended notification system — in-app list with Clear all, per-item dismiss, a 50-item soft cap, and redb-backed persistence across sessions; startup tip + bridged FM/password/config/viewer action failures (incl. PDF/archive/viewport + FM rename/delete/drop/context); OS toasts deferred
- [ ] Built-in browser (WebView2)
- [ ] Lua scripting (mlua)
- [ ] Theme and widget marketplace

## v2.0 — Year 2

- [ ] Replace Winlogon\Shell as an option
- [ ] TUI mode (ratatui for SSH/low-spec machines)
- [ ] Mobile companion (Android/iOS)
- [ ] Plugin system (WASM, capability-based)
- [ ] Enterprise edition (centralized management)
