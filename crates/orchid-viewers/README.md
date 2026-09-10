# orchid-viewers

Content viewers and editors. Dispatch uses magic bytes + extension. DOCX
and `.orchid` document envelopes are classified **before** generic ZIP.

| Kind | Stack | Notes |
|------|--------|--------|
| Images | `image`, `resvg`, WIC, `rawler`, … | View + sibling-file edit/export |
| PDF | `pdfium-render` | Needs `pdfium.dll` |
| Text | Tree-sitter + rope | View / edit / hex |
| Archives | zip, sevenz, tar(+gz/xz) | Browse / preview |
| Documents | OOXML + parley/swash | Tier-1 DOCX + native `.orchid` save/open |
| Media | libmpv | Optional DLL |
| HTML | source + local path | WebView2 overlay in `orchid-ui` |

Native `.orchid` I/O: `document/orchid_io.rs`. Spec:
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md).
