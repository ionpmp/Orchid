# orchid-viewers

Content viewers for Orchid. Groups the per-format rendering pipelines: PDFs via `pdfium-render`, raster images via `image`, syntax-highlighted text via `tree-sitter`, and archives (7z, zip) via `sevenz-rust` and `zip`.

Each viewer exposes the same high-level trait so the UI layer can pick a renderer purely from a detected file kind, without format-specific branches outside this crate.
