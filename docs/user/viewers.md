# Viewers

Opening a file (F3 / double-click / drop) routes by magic bytes and
extension. `.orchid` wraps unwrap Raw (except DOCX envelopes, which stay
in the document editor). Chrome keeps the `.orchid` path; temps are deleted
on close.

Catalog **Document Editor** creates `Untitled.orchid` (linked when a
`ChunkStore` is available). **Media Player** is a file picker → media Viewer.

## Images

Wide format set (JPEG/PNG/WebP/RAW/SVG/HEIC via WIC, …). Zoom/pan, folder
playlist, thumbs, slideshow, Timeline/Map/Calendar, EXIF, sibling-file
edits. **People view** is not implemented. No HDR framebuffer (8-bit RGBA).

## PDF

Needs `pdfium.dll`. Page nav, fit width / page, zoom, outline sidebar,
in-document find (`Ctrl+F`, match case), print (`Ctrl+P`).

Drag to select text (double-click a word, `Ctrl+A` the page). `Ctrl+C`
copies the selection, or the whole page when nothing is selected.
**Highlight** writes a sibling `*-hl.pdf` with highlight annotations over
the selection — it does not edit the open file.

No AcroForm fill-in, no in-place annotation save, no comments.

## Text

Tree-sitter highlighting, F3 view / F4 edit, HEX/binary, encodings,
find/replace, save.

## Media (libmpv)

In-app playback when the DLL is bundled; otherwise system-player handoff.
Playlist, subs, SMTC, EQ, ReplayGain. Viewer volume is **not** the Audio
Player widget volume.

## HTML

WebView2 overlay when the Evergreen runtime is installed; otherwise source
preview (size-capped) + Open in system browser.

## Document editor (DOCX / `.orchid`)

Tier-1 OOXML editor: Preview/Source, tables, images, styles, comments,
headers/footers (including first-page and even pages), Find/Replace, print.

**Save** to `.docx` or native `.orchid` (toolbar / Ctrl+Shift+S). Linked
`.orchid` autosaves after a ~2s debounce. Encrypted documents prompt for a
passphrase and keep the identity for re-encrypt on save. Info strip shows
generation, sealed vs linked, and C2PA status when present.

This is not Microsoft Word. Native format details:
[ORCHID_FORMAT.md](../ORCHID_FORMAT.md).
