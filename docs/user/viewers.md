# Viewers

Opening a file (F3 / double-click / drop) routes by magic bytes and
extension. `.orchid` wraps unwrap Raw (except DOCX envelopes, which stay
in the document editor). Chrome keeps the `.orchid` path; temps are deleted
on close. OOXML packages (ZIP) are classified via `[Content_Types].xml`
when present in the file head: Word → document editor; Excel (`.xlsx`,
`.xlsm`) and PowerPoint (`.pptx`, `.pptm`, `.ppsx`) → a read-only HTML
preview (sheets as tables, slides as cards, including speaker notes).
`.xlsb` stays in the archive browser. Misnamed `.docx` sheets or slides
follow the sniff, not the extension.

Catalog **Document Editor** creates `Untitled.orchid` (linked when a
`ChunkStore` is available). **Media Player** is a file picker → media Viewer.

## Images

Wide format set (JPEG/PNG/WebP/RAW/SVG/HEIC via WIC, …). Zoom/pan, folder
playlist, thumbs, slideshow, Timeline/Map/Calendar, EXIF, sibling-file
edits. **Files → Photos** can ask Windows for face rectangles and groups
`people/` tags. The image viewer draws those stored rectangles on the
decoded picture. A rotated or flipped view hides them. The boxes do not
name the person. The view stays 8-bit RGBA.
Radiance HDR and OpenEXR are tone-mapped into that buffer (Reinhard, then
sRGB) so pixels brighter than 1.0 are not clipped to white. The status
line marks those files `tone-mapped`.

## PDF

Needs `pdfium.dll`. Page nav, fit width / page, zoom, outline sidebar,
in-document find (`Ctrl+F`, match case), print (`Ctrl+P`).

Drag to select text (double-click a word, `Ctrl+A` the page). `Ctrl+C`
copies the selection, or the whole page when nothing is selected.
**Highlight** writes highlight annotations into the open file (reloads so
you can stack marks). If that path is not writable, it falls back to a
sibling `*-hl.pdf`. **Comment** pins a sticky note whose text is the
current selection (`*-note.pdf` if the open file is not writable).

**Form** lists existing AcroForm fields (`Name=value`) and fills a text
box, checkbox (`true` / `yes` / `on` / `1`), or radio button (the value
must match that button's export value). Type `Name=value` and press
Fill. The write stays in the open file and the page reloads; if that
path is not writable, it falls back to a sibling `*-form.pdf`. Combo
boxes, list boxes, and signatures are shown but not filled.

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
