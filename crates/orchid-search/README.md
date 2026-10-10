# orchid-search

Full-text search for Orchid backed by Tantivy. Exposes a `SearchEngine` facade, a scheduler-driven indexer, a pluggable content-extractor chain, and an `IndexFsSubscriber` that keeps the index live in response to `orchid-fs` bus events (`fs.created`, `fs.modified`, `fs.deleted`, `fs.renamed`, `fs.tags_changed`).

`Extractor::new()` registers the text-family extractors (source, calendar, contacts, bibliography, feeds, notebooks, mailboxes, playlists, subtitles, UI catalogs, lockfiles, and the rest of `extractors/`). Desktop bootstrap always adds `.orchid` Clean-Text (`with_orchid`). When `[search].extract-pdf` is on it also adds PDF (`pdfium-render`), DOCX, XLSX, PPTX, EPUB, OpenDocument, RTF, audio tags, FictionBook, `.eml`, and HTML. Crawl reads file bodies only when `extract-text` or `extract-pdf` is on.

The engine ships with a single fixed schema (see `schema::Schema`). Tokenizers:

- `path` — raw / exact-match (used as the primary key for upserts and deletes)
- `name` / `content` — Tantivy's default + English stemmer
- `extension`, `tags`, `color_label`, `mime`, `kind`, `in_archive` — raw strings

`query::QueryBuilder` sets text, one or more extensions, a tag, a path prefix, a size range, modified-after / modified-before, limit, and offset. MIME, colour, and file-versus-directory filters live on `Query` and in engine filtering; they are not builder methods. Free-text searches attach a content snippet (via Tantivy's `SnippetGenerator`) with highlight ranges when the hit has indexed body text; filter-only queries leave `snippet: None`.

PDF extraction requires pdfium at runtime; see `extractors::pdf`. `.orchid`
Clean-Text extraction lives in `extractors::orchid`.

ANN + RRF hybrid search (`ann`, `hybrid`, `SearchEngine::search_hybrid`)
fuses Tantivy BM25 with `orchid-embed`. The desktop app enables the `ort`
feature, so that build uses the compiled-in quantized ONNX model. Crate
tests without the feature stay on the synonym stub. The engine keeps an ANN
in lockstep with upserts/removes and writes `ann.<model-id>` beside the
Tantivy index on commit (`ann.stub.v1` for the stub). Universal-search
**files** call `search_hybrid`.
