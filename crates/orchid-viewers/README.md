# orchid-viewers

Content viewers and editors. Dispatch uses magic bytes + extension. DOCX
and `.orchid` document envelopes are classified **before** generic ZIP.

| Kind | Stack | Notes |
|------|--------|--------|
| Images | `image`, `resvg`, WIC, `rawler`, … | View + sibling-file edit/export. HDR and OpenEXR are tone-mapped to 8-bit RGBA. Face rectangles are drawn by the UI from `photo-faces.json` |
| PDF | `pdfium-render` | Needs `pdfium.dll`. Text, highlight, sticky notes, AcroForm fill (text, check, radio, combo, list) |
| Text | Tree-sitter + rope | View / edit / hex |
| Archives | zip, sevenz, tar(+gz/xz/bz2) | Browse / preview |
| Documents | OOXML + parley/swash | Tier-1 DOCX + native `.orchid` save/open |
| Spreadsheets | OOXML | `.xlsx` / `.xlsm` cell table. Simple formulas recalculate on that sheet. `.xlsb` stays an archive |
| Slides | HTML cards | `.pptx` / `.pptm` / `.ppsx` read-only preview, including notes |
| Media | libmpv | Optional DLL |
| HTML | source + local path | WebView2 overlay in `orchid-ui` |

Native `.orchid` I/O: `document/orchid_io.rs`. Spec:
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md).
