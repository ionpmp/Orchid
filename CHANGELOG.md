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
  (Windows Search then Tantivy), EXIF / IPTC / XMP (`Canon` or `Make=Canon`),
  GPS radius (`lat,lon,km`), save results as a virtual folder; find
  duplicates by BLAKE3 content hash and find large files.
- Toolbar **visited folders** menu: top 5 most frequent paths, then recent;
  persisted with the widget.
- Address bar switches to an editable path with folder autocomplete; focus
  loss restores breadcrumb buttons.
- File/folder context menu shows name, type, size, modified, and MIME at the
  top instead of a separate Properties item.
- Selection: long-press or marquee enters tap-to-toggle mode; the toolbar
  shows **Deselect** only when multiple items are selected; tap empty or
  Escape clears the set; Shift+click range and Ctrl+click still work;
  invert (`*`), name mask (`+` / `-`), files-only / folders-only, attribute
  filter; status bar shows selected size.
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
- **Media viewer (libmpv):** in-app audio/video playback with play/pause,
  seek scrubber, volume/mute, speed, folder playlist, audio-track cycle,
  chapters, embedded and sidecar `.srt`/`.ass` subtitles, album cover art
  (ID3 APIC / folder `cover.jpg`), A-B loop (Shift+A/B, Shift+L clear),
  resume position, manual subtitle open (Ctrl+S) with scale `{`/`}` and
  Alt+↑/↓ position, playlist shuffle (Y) / random jump (R), Windows SMTC
  publish with cover thumbnail (media keys / lock screen), simple EQ
  presets (E: Flat/Bass/Treble/Vocal), hardware decode via `hwdec=auto-copy`
  (H cycles auto-copy ↔ software; status chip shows active decoder), persisted
  volume/mute (`media_prefs.json`), SW blit up to 1920px wide, OSD flash for
  volume/seek/speed/mute, folder auto-advance on EOF (L toggles loop), stop
  that seeks to start and pauses (Backspace), Home/End playlist ends, frame
  step (`,` / `.`), screenshot PNG (Ctrl+Shift+S), sleep timer (T:
  15/30/60/90/off), aspect cycle (V), rotate (Ctrl+R), audio delay
  (Ctrl+[/]), wheel seek and double-click fullscreen on the surface,
  and keyboard shortcuts. Side playlist panel (Q toggle; click row to jump).
  Drop files from Explorer onto a viewer to open them there. SW blit scales
  to the widget viewport (capped at 1920×1080). Video transport chrome
  auto-hides after ~2.5s idle (mouse move / keys / hover reveal; audio-only
  keeps the bar).   Pitch-preserving speed via mpv `audio-pitch-correction`
  / scaletempo2 (K or click speed chip to toggle; persisted). Paused idle
  skips UI republish / slows mpv polling; catalog picker remembers the last
  media folder. Subtitle style presets (G: outline / yellow / box / cyan;
  Ctrl+0 resets). ReplayGain off/track/album (U). Shift+F kiosk, Esc exits
  immersive, Shift+M next monitor. Catalog **Media Player** launcher opens a
  file picker then places a viewer on the canvas (distinct from **Now Playing**
  SMTC). Bundle `mpv-1.dll` / `libmpv-2.dll` under `third-party/mpv/win-x64/`
  (see `docs/BUILDING.md`); without it, chrome remains and files can still
  open in the system player. Subtitle style and ReplayGain choices persist;
  playlist panel open state and SW blit width account for the side list.
  Chapters popup (click chapter chip), volume scrubber (drag / double-click
  mute), clickable EQ and audio-track chips.
- Image, PDF (pdfium), syntax-highlighted text (Tree-sitter),
  archives (ZIP / 7z / TAR / TAR.GZ / TAR.XZ).
- Image display: fit-to-window / width / height, 1:1, shrink-if-larger
  (no upscale), theme / black / white / gray / custom / checkerboard
  backgrounds (alpha), EXIF orientation auto-rotate, ICC color management
  toward the Windows monitor profile (or sRGB). Fullscreen (F11),
  borderless kiosk, and next-monitor (M). Slint stays 8-bit RGBA — HDR
  framebuffer presentation is still pending.
- Image folder navigation: next/prev/first/last, go-to-N, random, loop
  at the ends, skip unreadable files, recently viewed list, and jump to
  the folder in the file manager. PgUp/PgDn / Space / arrows (when
  fitted), mouse wheel, and horizontal swipe.
- Image zoom / pan: percent field, zoom-to-selection (Shift+drag),
  cursor-anchored Ctrl+wheel, pinch + two-finger pan, magnifier (Z),
  thumbnail overview for large images, and restore last zoom when
  switching files.
- Image view-only transforms: 90° CW/CCW, 180°, free angle (field or
  `[` / `]`), horizontal / vertical flip, and reset orientation (no
  file write).
- Lossless JPEG/PNG file transforms: rotate 90/180/270, flip, MCU-aligned
  JPEG crop, EXIF auto-rotate without recompress, and batch rotate of
  the folder playlist.
- Image thumbnail strip (top/bottom), folder grid mode, S/M/L size,
  name/size/date/rating under each cell, preload of the next N images,
  shared on-disk thumbnail cache (mtime-keyed; refreshes when the file
  changes), fast EXIF/embedded-JPEG thumbs, and a contact-sheet PNG
  (`T`/`G`/`D`/`I`/`P`).
- Image folder browse: Timeline (EXIF/mtime), Map (GPS pins), and Calendar
  month grid (`Ctrl+Shift+T` / `M` / `C`, Esc). People view by faces is
  planned for v1.x.
- Image chrome auto-hides after idle time (tap or vertical swipe to
  peek); mouse swipe / right-drag next-prev, double-click fit/actual;
  touch pinch, two-finger pan, swipe, tap, and double-tap.
- Image slideshow: auto-advance every N seconds (F5), pause/resume (Space),
  speed (`,`/`.`), random order (Y), fade/slide/dissolve/wipe with adjustable
  duration (J / Shift+J), loop, name/date/EXIF overlay (Shift+O), folder
  background music (Shift+M / ffplay), and export to HTML player, ffmpeg
  MP4, optional self-running EXE, and `.scr` screensaver.
- Image metadata inspector: EXIF (camera / lens / exposure / date), IPTC,
  XMP, GPS with OpenStreetMap, file size / dimensions / bit depth / ICC,
  MD5 and SHA-256, brightness+RGB histogram, and RGB/HSL/HEX/CMYK under
  the cursor. Shift+I opens the panel; Ctrl+Shift+O overlays EXIF;
  Ctrl+H cycles the histogram (`Shift+I` / `Ctrl+Shift+O` / `Ctrl+H`).
- Image metadata editing: IPTC / XMP fields, set or clear GPS, set or shift
  the shoot date, strip all tags or GPS only (privacy), copy tags between
  files, CSV/XML export and CSV import, and folder templates
  (`orchid-meta-templates.json`). File Manager **Tools → Metadata**; the
  viewer panel can save, strip, and export the open file.
- Destructive image edits write a sibling file (never the original):
  rectangle crop with optional aspect lock (1:1 / 4:3 / 3:2 / 16:9) and
  keep-width / keep-height, resize to `%` / `px` / `cm` with
  Nearest / Bilinear / Bicubic / Lanczos, canvas expand, perspective
  (four corners), straighten along a line, and auto-straighten.
  Viewer toolbar (`Shift+C` crop copy); FM **Tools → Image edit** for
  batch resize / canvas / auto-straighten.
- Image tone / color corrections write a sibling file: brightness /
  contrast, exposure / highlights / shadows, auto-levels / auto-contrast /
  auto-color (gray-world), white-balance temperature / tint, saturation /
  vibrance / hue / gamma, packed curves and levels, selective color,
  channel mixer, grayscale / sepia / invert, posterize / solarize /
  threshold. Viewer panel (`Shift+L` / ☼); FM **Tools → Image edit**.
- Image filters write a sibling file: sharpen / unsharp mask, Gaussian and
  motion blur, median / despeckle, emboss, edge detect, oil / watercolor /
  cartoon / pencil sketch, grain, vignette, barrel-pincushion and chromatic
  aberration correction, red-eye, skin smoothing, stacked recipes, and
  one-click looks (`vivid` / `soft` / `drama` / `clean` / `fade`) plus
  folder presets (`orchid-filter-presets.json`). Viewer panel (`Shift+F` /
  ✻); FM **Tools → Image edit**.
- Image annotations write a sibling file: line / arrow, rectangle / ellipse /
  polygon, freehand pen, text (font / size / color), callouts, privacy
  blur or pixelate, highlight, text and image watermarks with nine-slot
  placement and opacity, batch watermark, and a shoot-date stamp.
  Viewer panel (`Shift+D` / ✎); FM **Tools → Image edit**.
- Multi-image tools in FM **Tools → Image edit**: batch convert (JPEG / PNG /
  WebP / BMP), lossless rotate, thumbnail export, image rename templates
  (`{date}` / `{w}` / `{h}`), recipe preview / cancel / save
  (`orchid-batch-recipes.json`), 2–4 image compare and pick-best, pixel
  diff, composite / merge, panorama stitch, and HDR merge from brackets.
  Existing batch resize / adjust / watermark / metadata stay in the same menu.
- Image print: single photo or batch, paper size and margins, 2/4/6/9-up,
  index / contact sheet, on-screen preview, header/footer metadata tokens,
  and ICC destination (`srgb` / monitor / `.icc` file). Viewer panel
  (`Shift+P` / ⌨ Ctrl+P / ⎙); FM **Tools → Image edit**.
- Image formats: JPEG, PNG, GIF, BMP, TIFF, WebP, TGA, ICO/CUR, PNM
  (PBM/PGM/PPM), DDS, Radiance HDR, OpenEXR, plus JPEG-XL, Photoshop PSD,
  GIMP XCF, and PCX. Vector: SVG/SVGZ (`resvg`), Illustrator AI (Pdfium
  first page or EPS preview), EPS/PS (embedded TIFF/WMF/EPSI or Ghostscript),
  WMF/EMF (GDI), and CorelDRAW CDR (embedded preview). HEIC/AVIF and JPEG
  2000 use Windows Imaging codecs when installed. Camera RAW (CR2/CR3, NEF,
  ARW, RAF, ORF, PEF, RW2, SRW, X3F, RWL, DNG, DCR) demosaics via `rawler`
  when the camera is known, otherwise shows the embedded JPEG preview.
  Shift+L `develop` / `exposure=` / `temp=` / `tint=` re-develops from the
  sensor (camera WB + EV). Animated GIF, APNG, and WebP play in the viewer
  (play/pause, frame step, frame strip); export writes sibling
  `stem-f001.png` files and never overwrites the original. Multi-page
  TIFF and multi-size ICO/CUR step like still pages (Shift+←/→, strip);
  Extract page writes `stem-p002.png` or `stem-32x32.png`. Multi-page
  PDF stays in the PDF widget; Extract page writes `stem-p007.png`.
- Image share / export: save-as with JPEG quality and PNG compression,
  optional max-edge resize, ICO and favicon, pixel copy / paste, set as
  wallpaper, email attachment (auto-resized JPEG + `.eml`), social share
  (clipboard + compose URL), and screenshot (screen / window / region,
  optional delay). Viewer panel (`Shift+S` / ↗, `Ctrl+C` / `Ctrl+V`);
  FM **Tools → Image edit**.
- FM **F3** opens the Lister (view), **F4** opens the built-in editor;
  context menu **File associations…** opens the OS default-apps settings.
- Text Lister: Text / HEX / binary, encoding picker, wrap/no-wrap, find
  (F7) with regex + multiline replace, print, undo/redo.
- Media viewer plays in-app via libmpv when bundled; otherwise opens a Play
  handoff to the system player. HTML shows source
  plus Open in browser.
- **Tier-1 DOCX document editor**: OOXML read/write, Preview/Source,
  parley+swash canvas, selection and keyboard editing, tables (cell nav,
  insert/delete row/col, merge/unmerge cells, `tblGrid` widths, `gridSpan`/`vMerge` preview),
  inline images (body + cells),
  Find/Replace (`Ctrl+F` / F3, `n/m` status, Preview scroll-to-match, Find/Link hover tips, format/align/list + image/table + font/Preview + Save/Print + spacing/indent/page toolbar tips), Preview zoom (Z± / Ctrl+wheel / Ctrl+0, 50–300%), insert/edit/remove hyperlinks
  (Link toolbar / Ctrl+K; internal #bookmark), print (toolbar / Ctrl+P), page breaks (Ctrl+Enter), paragraph spacing (Sb±/Sp±), line spacing (auto Ln± + exact/atLeast round-trip), paragraph indent (w:ind left/right; Ind± / Ir± / Fl±), IME composition (preview), page margins from `pgMar` in preview (Mar+/-), Letter/A4 page size (Pg shows Ltr/A4), status word/char counts, find match-case (Aa), catalog **Document** launcher,
  canvas dock; Word/LibreOffice fixtures and round-trip tests.
- Spec draft for native **`.orchid`** container — see
  [`docs/ORCHID_FORMAT.md`](docs/ORCHID_FORMAT.md) (not implemented yet).

#### Built-in widgets
- **Audio Player**: local music library (Songs / Artists / Albums / Folders),
  playlists and favorites, shuffle / repeat, sleep timer, EQ presets, playback
  speed presets, soft volume boost (to 150%), library search, gapless prefetch,
  sidecar `.lrc` lyrics, background library scan, Windows SMTC (lock screen /
  media keys), shared audio-only libmpv session (separate from SMTC Now Playing
  and Viewer media chrome).
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
- MSRV / pinned toolchain **1.97 → 1.98.0**; Cargo.lock refreshed to latest
  compatible crate versions, plus intentional bumps: `icu` 2.3, `swash` 0.2.10,
  `crc32fast` 1.5, `fontdb` 0.24, `resvg`/`usvg` 0.48, `parley` 0.11,
  `bzip2` 0.6, `base64` 0.23, digest stack (`sha2`/`sha1`/`md-5`/`digest` 0.11),
  and `totp-rs` 6 (Builder API). Binary codec **`bincode` → `bincode_reloaded`
  3** (crates.io `bincode` 3.0 is an unmaintained stub that only emits
  `compile_error!`).
- File-manager selection follows Explorer instead of per-row checkboxes: click
  selects, Ctrl+click toggles, Shift+click extends, and a drag rubber-bands and
  highlights entries live as it crosses them. The toolbar shows **Deselect**
  only when multiple items are selected (empty click / Escape still clears).
- Large dependency refresh (Tantivy 0.26, redb 4, keepass 0.13, age/secrecy,
  notify, portable-pty/vte, viewers stack, ICU, FastCDC, windows/sysinfo).
- Idle CPU, UI lag, FM listing/thumbnail cost, and cold-start work cut
  (virtualized lists, Arc listings, live dir watches, mmap thumbs, coalesced
  weather/RSS fetches, System/Processes live refresh).
- File-manager listing stays interactive while scrolling: rebase the
  virtual window only near the edge, coalesce snapshot patches, skip
  unchanged rows, and hit-test the visible viewport instead of the full
  virtual height (hover no longer waits on a disk-enum / model rebuild).
- File-manager empty-space click no longer sticks in tap-to-toggle: a
  click (including trackpad jitter) clears selection, and marquee starts
  only after a 10px drag measured in one coordinate space.
- File-manager first paint: list names without waiting on managed/encrypted
  catalogs or per-folder marker probes; extract shell icons and image
  thumbs for the visible window first (list mode skips image thumbs);
  hidden tabs skip formatted rows until shown; local folders list in one
  blocking FindFirstFile/readdir pass instead of a per-file async stat;
  folders larger than the virtual window publish the first 80 names before
  the rest of the directory finishes; the listing stays visible while
  loading (status-bar hint instead of a full-pane overlay).
- **UI/render performance pass**: terminal glyph-cache `Arc` sharing, dirty-line
  retained raster, `Arc<[Cell]>` grid rows + mutation-only generation bumps,
  BytesMut PTY reads; in-place Slint model patches for clock / media / password
  / search / recent / calculator (including floating frames); media thumbnails
  pass `Arc<[u8]>` instead of base64; thumbnail service memory LRU, PNG encode
  without RGBA unwrap-clone, and real in-flight coalescing.

### Fixed
- Image viewer toolbar: compact geometric icons (not font symbols),
  overflow scroll, a hint strip with the action name on hover, extra
  tools behind **⋯**, and slideshow extras only while a slideshow is
  playing. Hover labels no longer fly off-button.
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
- File-manager selection: click empty space clears; marquee only hits tiles
  that intersect the rectangle (empty drag no longer selects the last file);
  checkbox is a dedicated hit target; click-drag of an already-selected set
  keeps the multi-selection; selected rows use a translucent fill so names
  stay readable; Ctrl+click in single-click-open mode no longer opens.
- File-manager selection mode no longer sticks: it turns itself off once the
  selection empties, a press whose release is swallowed by a listing update can
  no longer arm it after 500 ms, and clicks commit against the entry that was
  pressed rather than whatever the recycled row shows at release time.
- File-manager marquee commit keeps the live index range instead of
  re-hit-testing on mouse-up, which often shrank the selection the moment the
  button came up (restored only after a focus-driven full repaint). The model
  is patched before the live highlight is cleared, and selection cache syncs
  no longer mark the frame dirty (that raced the rubber band on the next tick).
- File-manager rubber-band rows highlight immediately from the live index
  range in Slint, and marquee selection updates the model on the UI stack
  instead of an async spawn — the previous round-trip left some entries
  unhighlighted until a focus-driven full repaint.
- File-manager shell icons use `IShellItemImageFactory` (Explorer's path)
  instead of the jumbo image list, which often stores a 32×32 glyph on a
  256×256 canvas and looked melted when stretched. Rows pull 48px sources;
  tiles fill their box from a true 256px bitmap.
- File-manager rubber band tracks the drag again. The band was driven by an
  overlay spawned on press, which never received the drag because the pressed
  area keeps the pointer grab; it only saw the bare cursor after release, so
  dragging selected nothing and merely hovering afterwards selected entries.
  Both views now track the band in the area that holds the grab.
- File-manager dropped the invisible select mode that a rubber band or a 500 ms
  hold used to latch, which silently turned every later plain click into a
  toggle. Toggling is Ctrl+click only, as in Explorer.
- File-manager rubber band can start on an entry, not just on empty space:
  dragging off an unselected row or tile bands, dragging off a selected one
  still transfers the selection. Touchpads had no reachable way to begin a
  selection, since the only entry points were a 500 ms hold and empty space.
- File-manager gestures now handle `PointerEventKind.cancel`. Touchpads abort
  pointer sequences far more often than mice, and an aborted press never
  reached the `up` handlers: the hold timer kept running and armed select mode
  by itself a moment after a light tap, and the latched `press-empty` flag
  froze list scrolling.
- File-manager clicks: Ctrl+click no longer counts as half of a double click,
  a third rapid click no longer opens the file twice, and selection patches in
  single-pane mode stop falling back to a full frame rebuild.
- File-manager background right-click works on empty space and empty folders,
  and shows only relevant actions (new folder / file, paste, select all).
- File-manager context-menu icons render as geometric glyphs (the previous
  `action-*` ids were drawn as Text and do not exist in Slint's Windows font).
- File-manager single-pane mode no longer shows the left navigation sidebar;
  the listing uses the full widget width. Dual-pane still includes the sidebar.
- Floating viewer unsaved-close confirm; Clock move-city handlers; Jyotish
  birth-profile date calendar (month/year sheets) and hour/minute wheels;
  search field sync.
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
