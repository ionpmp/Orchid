# orchid-search

Full-text search for Orchid backed by Tantivy. Exposes a `SearchEngine` facade, a scheduler-driven indexer, a pluggable content-extractor dispatch (plaintext + PDF via `pdfium-render`), and an `IndexFsSubscriber` that keeps the index live in response to `orchid-fs` bus events (`fs.created`, `fs.modified`, `fs.deleted`, `fs.renamed`, `fs.tags_changed`).

The engine ships with a single fixed schema (see `schema::Schema`). Tokenizers:

- `path` — raw / exact-match (used as the primary key for upserts and deletes)
- `name` / `content` — Tantivy's default + English stemmer
- `extension`, `tags`, `color_label`, `mime`, `kind`, `in_archive` — raw strings

`query::QueryBuilder` exposes a fluent surface covering text, extension, MIME, tag, colour, path prefix, size and modified-time ranges, file/directory filters, and pagination. Free-text searches attach a content snippet (via Tantivy's `SnippetGenerator`) with highlight ranges when the hit has indexed body text; filter-only queries leave `snippet: None`.

PDF extraction requires pdfium at runtime; see `extractors::pdf`. `.orchid`
Clean-Text extraction lives in `extractors::orchid`.

ANN + RRF hybrid search (`ann`, `hybrid`, `SearchEngine::search_hybrid`)
fuses Tantivy BM25 with stub embeddings from `orchid-embed`. The engine
keeps an in-memory ANN in lockstep with upserts/removes (not persisted;
the indexer crawl refills it). Universal-search **files** call
`search_hybrid`. A production ONNX model remains a
[roadmap](../../docs/ROADMAP.md) item.
