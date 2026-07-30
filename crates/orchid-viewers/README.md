# orchid-viewers

Content viewers and editors for Orchid. Groups the per-format pipelines:

| Kind | Stack | Notes |
|------|--------|--------|
| Images | `image`, `resvg`, Windows WIC (HEIC) | Zoom / pan / rotate / flip |
| PDF | `pdfium-render` | Needs bundled `pdfium.dll` |
| Text | Tree-sitter grammars | Read-only virtualized scroll + MVP edit/save |
| Archives | `zip`, `sevenz-rust`, TAR (+ gz/xz) | Browse, preview, extract |
| **Documents (DOCX)** | OOXML model + parley/swash preview | Tier-1 editor: tables, images, Find/Replace, Preview/Source |

Each viewer exposes the same high-level trait so the UI layer can pick a
renderer from a detected file kind, without format-specific branches outside
this crate. DOCX dispatches before ZIP (OOXML is a zip package).

Native `.orchid` save is **not** implemented yet — see
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md). Round-trip fixtures
live under `tests/fixtures/docx/`.
