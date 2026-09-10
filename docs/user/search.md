# Search

1. **Universal Search** — overlay / widget
2. **File Manager Find** (`Alt+F7`) — see [file-manager.md](file-manager.md)

## Universal Search

Top edge swipe, or the Search widget. Sources: **files** (Tantivy BM25 +
snippets), **commands**, **settings**, **calculator** (`=`), **calendar**,
**Jyotish**.

ANN + RRF hybrid search exists in `orchid-search` (stub embeddings, `.orchid`
Clean-Text) but is **not** what the universal-search file list uses yet.

## Index

`[search]` in `config.toml` (not the Settings panel): `included-roots`
(empty → Documents), `excluded-patterns`, `max-file-size-mib`,
`extract-text` / `extract-pdf` (PDF needs pdfium). Index path:
`data\search_index`. Watcher + bootstrap crawl. `.orchid` Clean-Text is
extracted when enabled.

Admin: [configuration](../admin/configuration.md).
