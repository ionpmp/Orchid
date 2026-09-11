# Search

1. **Universal Search** — overlay / widget
2. **File Manager Find** (`Alt+F7`) — see [file-manager.md](file-manager.md)

## Universal Search

Top edge swipe, or the Search widget. Sources: **files** (Tantivy BM25 fused
with in-memory ANN via reciprocal rank fusion, plus snippets), **commands**,
**settings**, **calculator** (`=`), **calendar**, **Jyotish**.

File hits use [`StubEmbedder`](../../crates/orchid-embed) today (synonym-aware
vectors, no ONNX model). Semantic recall works for indexed text — including
`.orchid` Clean-Text — after the indexer has extracted it. The ANN is
in-memory and refills from the crawl / watcher after a restart. A real
sentence model remains behind the reserved `ort` feature.

## Index

`[search]` in `config.toml` (not the Settings panel): `included-roots`
(empty → Documents), `excluded-patterns`, `max-file-size-mib`,
`extract-text` / `extract-pdf` (PDF needs pdfium). Index path:
`data\search_index`. Watcher + bootstrap crawl. `.orchid` Clean-Text is
extracted when enabled.

Admin: [configuration](../admin/configuration.md).
