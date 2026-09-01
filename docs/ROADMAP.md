# Orchid Roadmap

Legend: `[x]` done · `[~]` in progress · `[ ]` not started.

Last synced with `main` as of **2026-07-30**. Narrative release notes:
[`CHANGELOG.md`](../CHANGELOG.md). Spec for the future native container:
[`ORCHID_FORMAT.md`](ORCHID_FORMAT.md).

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
- [x] Drag-and-drop — folder rows, breadcrumbs, cross-pane, OS drop (move/copy with Ctrl), FM→viewer content zone (multi-file: floating viewer per new path / focus if already open, soft cap 8 + one rebuild), cross-widget FM move, transfer progress + failure toast; canvas + FM content-zone hit-test; wheel-scroll during drag; Enter/single-click open uses real `is_dir` from FM snapshot
- [x] System clipboard — copy/cut/paste files via `CF_HDROP` + Preferred DropEffect (Explorer and other apps); remote-only selections stay on the in-app clipboard
- [x] Virtual folders (Recent, Categories, Network, Recycle Bin) — Recent, Starred, Tags, categories, Recycle Bin (list / restore / purge / empty via OS trash); localized breadcrumbs + empty states; network mounts from config.toml with rclone browse/write + `copyto`/`moveto` fast paths
- [x] Inline rename, tags, color labels — inline rename in list/grid; tag / colour / star via `orchid-fs::TagManager`
- [x] File properties / content metadata — Alt+Enter report (path, times, attributes, MIME) plus EXIF, ID3, Office `docProps/core.xml` edit, Authenticode / PE certificate-table view, Sharing (SMB name / UNC, share or unshare folder, open the Windows Sharing tab), Previous Versions (shadow copies: list, restore, copy beside, open the Windows tab), and BitLocker (status, lock / unlock, Windows control panel)
- [x] Undo / redo for file operations — Ctrl+Z / Ctrl+Y (session stack): copy, move, rename, new file/folder, Recycle Bin delete; overwrites and permanent deletes are not stacked
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
- [x] Infrastructure (layouts, workspaces, lifecycle) — `orchid-widgets` ships the full framework: `Widget` trait, `WidgetRegistry`, `WidgetManager` (create / move / resize / close, visibility-driven Active↔Sleeping, Sleeping→Unloaded sweeper, persistence), `WorkspaceManager` (up to 9 workspaces, dense ordinals, switch-next/previous/by-ordinal), `LayoutEngine` (16×10 grid, auto-placement, collision, pixel snapshots), `GroupManager` (tab stacks persisted in a dedicated redb table), framework-wide events, and `build_command_set` of widget / workspace / group commands. `orchid-core::BackgroundJobQueue` runs always-on interval work (RSS/weather fetch; foundation for agents). `orchid-ui` exposes the renderer-agnostic `WidgetView` / `WidgetViewDispatcher` bridge and the Slint workspace dashboard (switcher, layout grid, drag/resize with snap ghost + collision feedback, dock show/hide + hover animations, group tab strip with drag-to-stack / switch / reorder / dissolve / Alt-drag detach).
- [x] In-app window manager — per-widget `WindowPlacement` (grid or floating); undock/dock any widget; z-order; minimize / maximize / restore; in-app taskbar; Ctrl+Tab cycle; edge snap (left/right half, maximize); schema v2 persistence
- [x] Widget: Weather
- [x] Widget: Moon (astronomy)
- [x] Widget: System indicators
- [x] Widget: Processes — Task Manager–style processes / services / startup / users (no Performance graphs)
- [x] Widget: Calculator — standard + scientific modes, history, memory, DEG/RAD/GRAD, keyboard input, `=expr` universal-search
- [x] Widget: World clock — multi-city list, IANA zones, relative offsets, reorder, GPS/IP “Local” label, settings persistence
- [x] Widget: Notes — tabbed local scratchpad with wrap/mono/font settings and find
- [x] Widget: Calendar — local month grid + day agenda CRUD, upcoming strip, jump-to-date, color filter, default duration, duplicate, year jump, settings, universal search
- [x] Widget: Files (recent) — shared MRU store, dock widget, FM virtual Recent folder
- [x] Widget: Universal search — debouncer + aggregator wired; UI patch-on-update (no per-keystroke rebuild)
- [x] Widget: Media player (audio/video)
- [x] Widget: RSS feed
- [x] Widget: Password manager — unlock UI (passphrase + Hello), search, copy, TOTP, add entry; lock vault button + command
- [x] Widget: Terminal — end-to-end with tab strip, split panes, draggable dividers, shortcuts, live raster, persisted layout
- [x] Widget: Jyotish — see Additional / Jyotish module below

### Viewers
- [x] Floating open — new documents open in a viewport-relative floating overlay above the canvas; reopening the same path focuses the existing viewer (group tab + scroll when docked); header-drag onto a free grid slot docks to the canvas
- [~] Images (PNG, JPEG, WebP, AVIF, HEIC, BMP, GIF, TIFF, TGA, SVG/SVGZ, AI, EPS/PS, WMF/EMF, CDR, ICO/CUR, PNM, DDS, HDR, EXR, JXL, PSD, XCF, PCX, JPEG 2000, CR2/CR3, NEF, ARW, RAF, ORF, PEF, RW2, SRW, X3F, RWL, DNG, DCR) — `ImageViewer` + zoom/pan/rotate/flip with active flip accent; keyboard shortcuts when focused (+/− zoom, F fit, 1 actual, W/E/0 fit width/height/shrink, B background, F11 fullscreen, K kiosk, M next monitor, Esc exit, arrows pan when zoomed / next-prev when fitted, PgUp/PgDn / Space folder nav, R/Shift+R rotate, U 180°, [/] free angle, X reset orientation, H/V flip, Z magnifier, S selection zoom, N overview); folder playlist (next/prev/first/last/goto/random, loop, skip unreadable, recently viewed, jump to folder in FM; wheel + swipe); zoom percent, zoom-to-selection, cursor-anchored Ctrl+wheel, pinch + two-finger pan, magnifier, thumbnail overview, restore last zoom on switch; lossless JPEG/PNG rotate/flip/crop + EXIF auto-rotate + folder batch (writes the file); thumbnail strip (top/bottom) + folder grid + S/M/L size + name/size/date/rating + preload next N + disk cache (mtime refresh) + EXIF preview thumbs + contact sheet (`T`/`G`/`D`/`I`/`P`); slideshow (F5 auto-advance, Space pause, `,`/`.` speed, Y random, J/Shift+J fade/slide/dissolve/wipe + duration, Shift+O name/date/EXIF overlay, Shift+M music, loop, HTML/video/EXE/SCR export); metadata inspector (Shift+I EXIF/IPTC/XMP/GPS/ICC/hashes, Ctrl+Shift+O EXIF overlay, Ctrl+H histogram, cursor RGB/HSL/HEX/CMYK, OSM map); metadata editing (IPTC/XMP, GPS set/clear, shoot date + delta, strip all / strip GPS, copy, CSV/XML, folder templates; FM Tools + viewer Save); destructive save-as crop (aspect + keep side), resize (%/px/cm, Nearest/Bilinear/Bicubic/Lanczos), canvas expand, perspective, straighten / auto-straighten; tone/color save-as (brightness/contrast, exposure/highlights/shadows, auto-levels/contrast/color, WB temp/tint, sat/vibrance/hue/gamma, curves, levels, selective color, channel mixer, gray/sepia/invert, posterize/solarize/threshold; Shift+L); filters/effects save-as (sharpen/unsharp, Gaussian/motion blur, median/despeckle, emboss, edges, oil/watercolor/cartoon/sketch, grain, vignette, lens + CA, red-eye, skin smooth, stacks + looks; Shift+F); annotate/draw save-as (line/arrow, rect/ellipse/poly, pen, text + font/size/color, callout, privacy blur/pixelate, highlight, text/image watermark + batch + 9-slot + opacity, date stamp; Shift+D); multi-image batch (convert, lossless rotate, thumbs, rename templates, recipe preview/cancel/save, 2–4 compare + pick-best, pixel diff, composite, panorama, HDR merge); print (size/margins, n-up, index sheet, preview, header/footer, ICC, batch; Shift+P / Ctrl+P); share/export (save-as quality, ICO/favicon, clipboard, wallpaper, email, social, screenshot region/window/delay; Shift+S); localized fit modes + status strip + toolbar hover hints; viewport re-fit while in fit mode; checkerboard / black / white / gray / custom canvas; EXIF orientation applied on load; ICC transform to the Windows monitor profile or sRGB (8-bit RGBA — HDR framebuffer still pending); SVG via `resvg`; HEIC/HEIF via Windows WIC when the OS HEIF codec is installed (clear unsupported message otherwise); RAW demosaics via `rawler` (camera WB + EV / temp / tint on Shift+L `develop`) and falls back to the embedded JPEG preview; animated GIF / APNG / WebP playback (Space play/pause, Shift+←/→ frame step, frame strip, export sibling `stem-f001.png`); multi-page TIFF + multi-size ICO/CUR (page strip, extract sibling `stem-p002.png` / `stem-32x32.png`); multi-page PDF in the PDF widget with extract-page PNG; EXIF report in FM Properties / Tools; folder browse Timeline / Map (GPS scatter) / Calendar (Ctrl+Shift+T/M/C, Esc); auto-hide chrome (tap / idle); mouse gestures (swipe next/prev, right-drag, vertical swipe chrome, double-click fit); touch pinch / two-finger pan / swipe / tap / double-tap; People view by faces planned for v1.x
- [~] PDF (pdfium) — Pdfium-backed viewer with page navigation (toolbar + PageUp/Down/←/→ when focused), go-to-page input, fit width/page with active-mode highlight, zoom, toolbar hover hints, viewport re-fit; action failures (page/zoom/fit/viewport) surface as localized notifications; requires bundled `pdfium.dll`
- [~] Text with syntax highlighting (Tree-sitter) — grammars for rust/python/toml/json/markdown/javascript/typescript/tsx/yaml/go/bash/html/css/c/cpp/java/ruby/sql/php/kotlin; F3 view / F4 edit from FM; Text/HEX/binary modes; encoding autodetect + on-the-fly switch; wrap/no-wrap; find/replace (F7 / Ctrl+F, regex + multiline); print; undo/redo in the editor; save via toolbar/Ctrl+S, dirty ●, localized line count + LF/CRLF
- [~] Archives — FM folder browse (`archive:`), extract all/selected, create / add / delete / test, password + multi-volume + SFX via 7-Zip; ZIP / RAR / 7z / TAR(+GZ/XZ/BZ2) / CAB / ISO / ACE / ARJ / LZH; nested archives; viewer still previews ZIP / 7z / TAR / TAR.GZ / TAR.XZ
- [~] Media (audio/video) — libmpv in-app playback (RGBA blit ≤1920×1080, viewport-adaptive) with play/pause/seek/stop/volume/speed/mute/fullscreen, folder playlist side panel (Q) + shuffle + loop + EOF auto-next + Home/End, frame step + screenshot PNG, sleep timer (T), aspect/rotate/audio-delay, wheel seek + double-click fullscreen, Explorer drop-open, audio tracks + chapters, album cover (APIC / folder cover), A-B loop + resume, manual sub-add + scale/pos, simple EQ presets, `hwdec=auto-copy` (H toggles SW, persisted), persisted volume/mute, OSD flash, Windows SMTC publish, video chrome auto-hide, pitch-preserving speed (K), pause idle CPU trim, remember last picker folder, sub style presets (G), ReplayGain (U), media kiosk/Esc/next-monitor; catalog **Media Player** launcher (file picker → canvas viewer; distinct from **Now Playing** SMTC); embedded + sidecar `.srt`/`.ass` subs; falls back to system player (`opener`) when DLL missing; ID3 tag report in FM Tools
- [~] HTML — source preview + Open in the system browser (no embedded WebView yet)
- [~] Document editor (DOCX) — Tier-1 rich text model + OOXML read/write (fonts/bold/italic/underline/strikethrough/highlight/superscript/subscript/color, external `w:hyperlink` + rels round-trip (preview blue+underline; Ctrl+click opens http/https/mailto; hover pointer; insert/edit/remove via Link toolbar / Ctrl+K; internal #bookmark via w:anchor + w:bookmarkStart, Ctrl+click jumps), alignment, lists via `numbering.xml`/`numId`, simple tables with cell-paragraph cursor/typing (Enter split / Backspace join, align/list/indent, Tab/Shift+Tab cell nav, insert table / insert/delete row/col via toolbar), merge/unmerge cells (toolbar M+/M−; horizontal prefer, else vertical), `gridSpan`/`vMerge` OOXML round-trip on cells (preview spans columns/rows; continue slots covered by restart), column grid preview from `tblGrid`/`tcW` (equal-width when absent) + hairline borders + column-aware hit-test, `w:drawing` inside cells → preview blit + click/delete/arrow cell-image cursor, page setup (preview honors `w:pgMar`; Mar+/- margins; Pg shows/toggles Ltr/A4), body inline `w:drawing` → `Block::Image` with OOXML write + media/rels on save; insert via toolbar **Img** / Ctrl+V clipboard image); `DocumentViewer` + dispatch before ZIP-archive; Slint document surface (`kind=7`) with toolbar + Preview/Source toggle; parley+swash RGBA preview canvas with click/drag/double-click-word/triple-click-paragraph selection, caret paint, keyboard typing (insert/delete/Enter paragraph / Shift+Enter soft `w:br` / arrows/word-jump/Home/End/Ctrl+A), clipboard copy/cut/paste (text + image), and inline-image blit; selection-scoped B/I/U/S/H/x²/x₂ + clear formatting (Tx / Ctrl+Space) + font size/family/color + align/list (Tab/Shift+Tab indent) on selected paragraphs; viewport-driven preview width; plain `TextInput` source mode; undo/redo; print (toolbar / Ctrl+P, plain-text shell Print like text viewer); page break (Ctrl+Enter / w:pageBreakBefore, preview rule); paragraph spacing before/after (w:spacing + Sb±/Sp± toolbar); line spacing (w:line auto/exact/atLeast; Ln± presets for auto); paragraph indent (w:ind left/right/firstLine/hanging; Ind± left, Ir± right, Fl± first/hanging); IME composition via TextInput catcher (preedit strip); status strip word/char counts; catalog **Document** launcher writes a sample `.docx` under `data/documents/` and opens a viewer docked on the canvas (floating chrome has dock ▣; FM/`open_in_viewer` stays floating); in-document Find/Replace (Ctrl+F / F3; Aa match-case) with next/prev over plain text, `n/m` match status, replace current/all, and Source mode caret sync, Preview scroll-to-match; Find/Link hover tips; format/align/list + image/table + font/Preview + Save/Print + spacing/indent/page toolbar tips; Preview zoom (Z± / Ctrl+wheel / Ctrl+0, 50–300%); Tantivy DOCX extractor; Word/LibreOffice fixtures under `tests/fixtures/docx/` + parse/round-trip tests

### Security
- [~] Password manager (KDBX4 format, custom UX) — unlock/lock UI + Hello; KDBX4 R/W, groups/entries/TOTP; `privacy.vault_auto_lock_seconds` idle lock (default 300s)
- [~] File and folder encryption (age-based) — engine + file-manager encrypt / decrypt / reveal wired; localized passphrase UX + Windows Hello on FM passphrase dialog
- [x] Biometric unlock via Windows Hello — password vault + FM encrypted-folder passphrase via DPAPI

### Storage
- [x] Content-addressed storage (BLAKE3 + FastCDC chunking) — `ChunkStore`, refcount table, orphan GC; managed-folder policy (`exclude_patterns`, quota, retention) in `orchid-fs`
- [~] Deduplication in managed folders — `Deduplicator` + add-to-managed in file manager; ingest failure UX + sidebar stats; policy dialog in FM

### Network Clients
- [x] SFTP / SCP / SMB / FTP / FTPS / WebDAV / S3 via rclone — browse + read/write; `rclone-remote` or inline creds; remote↔remote copy; FTP resume retries; `rclone sync` cloud sync; runtime bookmarks (`network-bookmarks.toml`); letterless Windows UNC in Network
- [~] OAuth clouds (Drive / OneDrive / Dropbox) — named `rclone-remote` in rclone.conf (no in-app OAuth wizard yet)

### Search
- [x] Tantivy indexing — `orchid-search::SearchEngine` with full schema, batched writer, commit/optimize/shutdown
- [x] File watcher for incremental updates — `IndexFsSubscriber` + `FileWatcher` on `[search].included-roots` (default: Documents), bootstrap crawl, text/PDF extract → `IndexScheduler` (wired in `OrchidApp::bootstrap`)
- [x] Universal search (files + commands + settings) — live settings editor for theme/locale/density/bools/shortcut profile + remaps; search debouncer hardening + `SEARCH_LIVE` miss metrics; file hits show Tantivy content snippets in the subtitle when available
- [x] File-manager Find — name / mask / regex, size, date, attributes, content grep, case toggle, archives, Windows Search + Tantivy indexed fallback, EXIF / IPTC / XMP and GPS radius, save as `virtual:search`, duplicates by BLAKE3, large files (`Alt+F7`)

### UX
- [x] Theming (light/dark, density modes, hot-reload) — theme, locale, and density hot-reload from config.toml (main window + startup window)
- [x] Built-in themes (Orchid Light/Dark, Solarized, Nord, Catppuccin, High Contrast) — nine bundled themes + JSON loader from `themes_dir`
- [~] Internationalization (11 languages, RTL) — 11 Fluent catalogues bundled (`en-US`…`ar-SA`); widget titles + catalog/dock descriptions (label+desc hover, label-only dock tiles), FM/viewer sizes, System uptime/battery charging·time + severity hover hints, Properties/Details/delete confirm, search empty states + candidate/title hover (BMP source glyphs), startup status strip (localized theme display names + locale endonyms), FM Home/loading/empty/access + encrypted/managed entry hints + OS I/O error mapping (translated) + elided filename/status-bar hover + disabled back/forward + sort-header hints + text-only pane errors + sidebar toggle active accents + BMP sidebar glyphs, settings shortcuts (editable remaps + OS profiles)/disabled/default placeholders + section/field-label hover + localized config-reload/field-reject/save failure notifications, TOTP remaining, PDF page-of/Go + text-only unavailable, archive extract/parent/path/entry/info-banner hover + icon-driven BMP glyphs + text-only binary preview, weather location/status + BMP condition glyphs + forecast range/precip Fluent + search/onboarding/dialog hover chrome, moon BMP phase glyphs + value-row hover, viewer unsupported type + text LF/CRLF/syntax/encoding + image/archive format labels + localized loading/error status (no glyph icons) + viewer/password/search/FM error fallthrough translated (IO/unknown mapped), recent-files path + RSS title/summary hover, media track metadata hover + transport failure notifications, notification title elide hover, command palette `orc` invocation, terminal split-drag + password (list + Copy/Open + detail elide/TOTP accent)/catalog/dock/FM/workspace/group chrome hover hints (incl. Alt-detach elide) + widget title elide/resize tooltip localized; S-size RTL mirrors notification/workspace docking when language starts with `ar`
- [~] Adaptive layouts (profiles for different screens) — Hybrid density nudges UI scale from canvas width: below 1100 px toward Touch (1.2×), above 1600 px toward Mouse (0.8×)
- [x] Gestures (touch, pen, mouse) — recogniser + `default_bindings` wired through `orchid-ui` to workspace panel, notification center, dock, and universal search
- [x] Keyboard shortcuts + leader-key mode — `Shortcut` parsing, reserved-combo detection, Orchid/Windows/macOS/Linux profiles, Settings remapping, user override application, and configurable leader-key chord dispatch (`Ctrl+Shift+Space` + letter)
- [x] Command palette — Ctrl+Shift+P overlay with fuzzy search and command dispatch
- [x] Onboarding tour, hint mode — four-step first-run overlay; `Win+?` hint overlays; persisted `[onboarding]` config

### Additional
- [x] Jyotish module — Vedic panchanga widget (tithi/nakshatra/yoga/karana/vara + grahas, Lahiri/KP/Raman ayanamsa; opt-in via catalog) + personal day scores, monthly/yearly forecast, life retrospective, birth-time rectification; Phase A trust layer (limb end-times, now/day scores, Rahu/Yama/Gulika, golden fixtures, disclaimer); Phase B score transparency (factor deltas/strength, narrative modes, anti-repeat advice); Phase C dashas/life (pratyantar + now strip, year→antar expand, gochara soft tint, rectify maha/antar/pratyantar scoring); Phase D usable rectify wizard (progress/back/draft, event validation, top-N score breakdown, refine pass, place resync); Phase E UI polish (a11y dots/tooltips, month selection+legend, empty location, year aggregate cache); Phase F i18n/tone (real UI translations for 9 locales, Sanskrit glossary keepers, Fluent placeholder audit, CONTRIBUTING note); Phase G product surfaces (day-color/Rahu Kalam notifications, day/week clipboard export, universal-search keyword source, [`docs/jyotish.md`](jyotish.md)); Phase H engineering (norm360/tara/dasha property tests, month color-cache reuse guard, slim closed-rectify UI model, `show_rahukalam` / `enable_personal` settings); post-H product polish (multi-location city search, pin Current location via GPS/IP, birth profiles with separate birth place, birth-date calendar + place UTC fill, richer localized Day summaries/advice)

## v1.x — 9–18 months

- [ ] AI agents (Ollama + OpenAI API) — will enqueue on `BackgroundJobQueue` (already used for RSS/weather)
- [ ] Photo library intelligence (after AI agents) — builds on FM `TagManager` + the image viewer
  - [ ] Hierarchical tags (`places/italy/rome`, nested sidebar, inherit / filter by ancestor)
  - [ ] Auto-tagging via AI — local Ollama or OpenAI: scene, objects, caption → tags (opt-in, review before apply)
  - [ ] Face recognition and grouping by people — detect / cluster / name, **People view** in the image viewer + People virtual folder, privacy: local embeddings only
  - [ ] Group photos by event / date — burst + EXIF/XMP shoot-date clusters, named events
  - [ ] Smart albums by criteria — saved queries (tag / person / date / GPS / rating / type) as virtual folders
- [ ] Graphical resource monitor with history
- [~] Extended notification system — in-app list with Clear all, per-item dismiss, a 50-item soft cap, and redb-backed persistence across sessions; startup tip + bridged FM/password/config/viewer action failures (incl. PDF/archive/viewport + FM rename/delete/drop/context); OS toasts deferred
- [ ] Built-in browser (WebView2)
- [ ] Lua scripting (mlua)
- [ ] Theme and widget marketplace

## Native format (`.orchid`) — see [`docs/ORCHID_FORMAT.md`](ORCHID_FORMAT.md)

AI-native container (magic `ORCD`) for documents and media wrappers. Spec first; implementation phased:

- [ ] Phase 1 — Framing: self-describing regions + FlatBuffers TOC + Raw/Clean-Text/Structured (snapshot) + mmap + zstd; sealed mode only
- [ ] Phase 2 — Per-region `age` encryption + linked mode (`ChunkStore`) + CAS generation history
- [ ] Phase 3 — C2PA provenance region (`c2pa` crate)
- [ ] Phase 4 — CRDT structured region (multi-writer human + AI agent)
- [ ] Phase 5 — Local embeddings (`ort`) + hierarchical vectors + ANN hybrid search with Tantivy

## v2.0 — Year 2

- [ ] Replace Winlogon\Shell as an option
- [ ] TUI mode (ratatui for SSH/low-spec machines)
- [ ] Mobile companion (Android/iOS)
- [ ] Plugin system (WASM, capability-based)
- [ ] Enterprise edition (centralized management)
