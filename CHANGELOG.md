# Changelog

All notable changes to Orchid are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/) once tagged
releases begin. Until then, entries under **Unreleased** describe the pre-alpha
tree on [`main`](https://github.com/ionpmp/Orchid).

## [Unreleased]

Pre-alpha snapshot as of **2026-07-30** (`0.1.0` workspace version). No tagged
release yet.

### Added

#### Workspace & shell
- In-app **window manager**: undock / dock widgets, floating placement, z-order,
  minimize / maximize / restore, in-app taskbar, Ctrl+Tab cycle, edge snap,
  schema v2 persistence.
- Widget **groups** (tab stacks), workspaces, 16×10 layout grid, catalog, dock,
  command palette, leader-key mode, onboarding tour, hint mode (`Win+?`).
- User-remappable shortcuts in Settings, with **Orchid (Commander)**,
  **Windows**, **macOS**, and **Linux** profiles (`[shortcuts].profile` plus
  per-command overrides).
- Nine bundled themes + JSON theme loader; 11 Fluent locales with RTL (ar-SA).

#### File manager & storage
- Dual-pane FM with icons / list / details / gallery, tabs, breadcrumbs,
  drag-and-drop (including OS drop and FM→viewer), tags, colour labels, quick
  filter, virtual folders (Recent, Starred, Tags, Search results, Recycle Bin,
  categories, network). Browse the Recycle Bin, restore items, permanently
  delete selected items, or empty the bin.
- Find files (`Alt+F7` / Tools): name / mask / regex, size, date, attributes,
  content grep (literal or regex), case sensitivity, archives, indexed search
  (Windows Search then Tantivy), save results as a virtual folder; find
  duplicates by BLAKE3 content hash and find large files.
- Toolbar **visited folders** menu: top 5 most frequent paths, then recent;
  persisted with the widget.
- Address bar switches to an editable path with folder autocomplete; focus
  loss restores breadcrumb buttons.
- File/folder context menu shows name, type, size, modified, and MIME at the
  top instead of a separate Properties item.
- Selection: Shift+click / Ctrl+click, invert (`*`), name mask (`+` / `-`),
  files-only / folders-only, attribute filter, rectangular marquee, and
  hover checkboxes; status bar shows selected size.
- Navigation: Ctrl+PgUp goes up, Alt+F1 / toolbar drive menu switches
  volumes, Ctrl+Shift+T opens the selection in a new tab, Ctrl+Shift+Enter
  opens it in the other pane, Ctrl+B flattens nested files (branch view).
- File operations: F5/F6 copy/move to the other pane (clipboard in
  single-pane), F7 new folder, Shift+F4 new file, F8/Del delete (to Recycle
  Bin; restore / empty from the Recycle Bin folder), Shift+Del
  permanent delete, Ctrl+X cut. Ctrl+Z / Ctrl+Y undo and redo copy, move,
  rename, new file/folder, and Recycle Bin delete (session-only; overwrites
  and permanent deletes are not stacked). Overwrite / Skip / Rename / Overwrite older
  / Resume conflict dialog with “apply to all”. Copy queue with pause,
  resume, cancel, verify, new/changed-only, folder structure only, NTFS
  ADS, timestamps and attributes. Batch rename by pattern; symlink /
  hardlink / junction. Touch action bar when single-click open is on.
  Advanced tools: folder compare / sync / merge, byte-level file compare,
  split / join, checksums (MD5 / SHA-1 / SHA-256 / BLAKE3 / CRC32) with
  sidecar verify, Base64 / UUE encode–decode, bulk attributes / timestamps
  / name case, chmod (and Unix chown), Windows ACL via icacls. Properties
  report (`Alt+Enter`) with EXIF, ID3, Office core metadata (editable),
  Windows Authenticode / PE certificate-table inspection, a Sharing
  section (SMB name, UNC, share / unshare, Windows Sharing tab), and
  Previous Versions (Volume Shadow Copy list, restore, copy beside,
  Windows Previous Versions tab), and BitLocker (volume status, lock /
  unlock with password or recovery key, Windows BitLocker panel).
- System clipboard file copy/paste (`CF_HDROP` + Preferred DropEffect) so
  Ctrl+C / Ctrl+X / Ctrl+V exchange files with Explorer and other apps.
- Browse archives as folders (`archive:`), extract all or selected, create /
  add / delete / test; password, multi-volume, and SFX via 7-Zip. Formats:
  ZIP, RAR, 7z, TAR / TAR.GZ / TAR.XZ / TAR.BZ2, CAB, ISO, ACE, ARJ, LZH;
  nested archives open after a temp extract.
- Encrypted folders (age), managed folders with content-addressed ingest
  (BLAKE3 + FastCDC), rclone network mounts (SFTP / SCP / SMB / FTP / FTPS /
  WebDAV / S3 and OAuth clouds via `rclone-remote`), remote-to-remote copy,
  FTP resume retries, `rclone sync` cloud sync, runtime network bookmarks,
  and letterless Windows UNC mappings.
- Tantivy search with incremental FS watcher, PDF/text/DOCX extractors,
  universal search (files + commands + settings).

#### Viewers
- Image, PDF (pdfium), syntax-highlighted text (Tree-sitter),
  archives (ZIP / 7z / TAR / TAR.GZ / TAR.XZ).
- FM **F3** opens the Lister (view), **F4** opens the built-in editor;
  context menu **File associations…** opens the OS default-apps settings.
- Text Lister: Text / HEX / binary, encoding picker, wrap/no-wrap, find
  (F7) with regex + multiline replace, print, undo/redo.
- Media files open a Play handoff to the system player; HTML shows source
  plus Open in browser.
- **Tier-1 DOCX document editor**: OOXML read/write, Preview/Source,
  parley+swash canvas, selection and keyboard editing, tables (cell nav,
  insert/delete row/col, `tblGrid` widths), inline images (body + cells),
  Find/Replace (`Ctrl+F` / F3, `n/m` status), catalog **Document** launcher,
  canvas dock; Word/LibreOffice fixtures and round-trip tests.
- Spec draft for native **`.orchid`** container — see
  [`docs/ORCHID_FORMAT.md`](docs/ORCHID_FORMAT.md) (not implemented yet).

#### Built-in widgets
- Terminal (PTY: PowerShell / cmd / WSL / SSH; tabs + splits).
- Weather, Moon (geometric phase disk), System indicators, Media, RSS,
  Recent files, Universal search, Password manager (KDBX4 + Windows Hello).
- **Calculator** (standard / scientific, history, memory, `=expr` search).
- **Processes** (apps / services / startup / users).
- **World clock** (multi-city, IANA zones, GPS/IP “Local” label).
- **Notes** (tabbed scratchpad, wrap/mono/font, find).
- **Calendar** (month grid, day agenda, upcoming strip, jump-to-date, color
  filter, duplicate, year jump, universal search).
- **Jyotish** (Vedic panchanga Phases A–H): day scores, dashas, gochara,
  birth-time rectification, multi-location + GPS/IP pin, birth profiles,
  notifications / export / search, full i18n chrome — see
  [`docs/jyotish.md`](docs/jyotish.md).

#### Platform
- Event bus, action dispatcher, command registry, gesture recognizer,
  shortcut overrides, `BackgroundJobQueue` for always-on fetch work.
- redb state store + TOML config with hot-reload; history / cache eviction.

### Changed
- Large dependency refresh (Tantivy 0.26, redb 4, keepass 0.13, age/secrecy,
  notify, portable-pty/vte, viewers stack, ICU, FastCDC, windows/sysinfo).
- Idle CPU, UI lag, FM listing/thumbnail cost, and cold-start work cut
  (virtualized lists, Arc listings, live dir watches, mmap thumbs, coalesced
  weather/RSS fetches, System/Processes live refresh).
- **UI/render performance pass**: terminal glyph-cache `Arc` sharing, dirty-line
  retained raster, `Arc<[Cell]>` grid rows + mutation-only generation bumps,
  BytesMut PTY reads; in-place Slint model patches for clock / media / password
  / search / recent / calculator (including floating frames); media thumbnails
  pass `Arc<[u8]>` instead of base64; thumbnail service memory LRU, PNG encode
  without RGBA unwrap-clone, and real in-flight coalescing.

### Fixed
- System CPU sampling via `GetSystemTimes`; process-list refresh spikes.
- FM Type labels for long extensions; delete-to-recycle / show-extensions;
  transfer and rename failure toasts.
- File-manager gallery / large-icon tiles no longer stretch 32×32 shell bitmaps;
  Windows jumbo (256px) association icons are used, and small glyphs stay at
  native size instead of melting across the tile.
- File-manager drag-and-drop starts on the pressed entry (selection lag no
  longer aborts the gesture) and dropping onto a file completes a move/copy
  into the current folder instead of cancelling.
- File-manager listings no longer go blank after leaving a long folder: the
  visible window resets on navigate / tab / filter / sort, and small folders
  always ship a full slice. Scroll virtualization mutates the entry model in
  place instead of remounting the widget (which made selection jump).
- File-manager background right-click works on empty space and empty folders,
  and shows only relevant actions (new folder / file, paste, select all).
- File-manager context-menu icons render as geometric glyphs (the previous
  `action-*` ids were drawn as Text and do not exist in Slint's Windows font).
- File-manager single-pane mode no longer shows the left navigation sidebar;
  the listing uses the full widget width. Dual-pane still includes the sidebar.
- Floating viewer unsaved-close confirm; Clock move-city handlers; Jyotish
  profile date/time steppers and search field sync.
- Fluent message IDs (hyphenated only); locale UTF-8 mojibake from early
  dependency bumps.

### Security
- Prefer `rclone-remote` over plaintext passwords in `config.toml` (documented
  in [`docs/SECURITY.md`](docs/SECURITY.md)).
- Vault idle auto-lock; Windows Hello / DPAPI for vault and encrypted-folder
  passphrase.

---

Older scaffolding history (2026-04 → mid-2026) lives in git; this file tracks
user-visible and contributor-relevant milestones from the pre-alpha push
toward MVP v0.1.
