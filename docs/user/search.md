# Search

1. **Universal Search** — overlay / widget
2. **File Manager Find** (`Alt+F7`) — see [file-manager.md](file-manager.md)

## Universal Search

Top edge swipe, or the Search widget. Sources: **files** (Tantivy BM25 fused
with in-memory ANN via reciprocal rank fusion, plus snippets), **commands**,
**settings**, **calculator** (`=`), **calendar**, **Jyotish**.

File hits use [`StubEmbedder`](../../crates/orchid-embed) today (synonym-aware
vectors, no ONNX model). Semantic recall works for indexed text after the
indexer has extracted it. `.orchid` files reuse the stored Embedding region
when present (same width as the stub). The ANN is snapshotted to
`ann.stub.v1` next to `data\search_index` on commit, so hybrid ranking
survives a restart. A real sentence model remains behind the reserved
`ort` feature.

## Index

`[search]` in `config.toml` (not the Settings panel): `included-roots`
(empty → Documents), `excluded-patterns`, `max-file-size-mib`,
`extract-text` / `extract-pdf` (PDF needs pdfium; the same switch also
indexes DOCX, XLSX / XLSM cell text, PPTX / PPTM / PPSX slide and
notes text, EPUB chapters, OpenDocument `.odt` / `.ods` / `.odp`,
RTF, visible HTML text (scripts and styles are skipped), audio
tags (ID3 and Vorbis comments: title, artist, album, lyrics), and
FictionBook (`.fb2` and `.fb2.zip`, including Windows-1251), and email
(`.eml`: subject, from, to, and text or HTML bodies; attachments are
skipped)).
`extract-text` also indexes source and subtitle files (`.rs`, `.py`, `.js`,
`.ts`, `.ps1`, `.sql`, `.ass`, and similar), plus calendar (`.ics`) and
contact (`.vcf`) fields. Photo and attachment blobs in those files are
skipped. Bibliography files (`.bib`, `.ris`) contribute titles, authors,
and abstracts. OPML lists (`.opml`) contribute outline titles and feed URLs.
Jupyter notebooks (`.ipynb`) contribute markdown and code cells, not outputs.
Unix mailboxes (`.mbox`) are split into messages and indexed like `.eml`.
RSS and Atom feeds (`.rss`, `.atom`, and `.xml` that opens as a feed)
contribute titles, authors, links, and article text. Markup is skipped.
Playlists (`.m3u`, `.m3u8`, `.pls`) contribute track titles and paths.
Durations and stream directives are skipped.
LaTeX sources (`.tex`, `.ltx`) contribute the document with comments
removed. Escaped percent signs and verbatim blocks are kept.
CUE sheets (`.cue`) contribute album and track titles, performers, and
file names. Index timestamps are skipped.
Gettext catalogs (`.po`, `.pot`) contribute message ids, translations,
and translator comments. Source locations and flags are skipped.
Shortcuts (`.url`, `.desktop`, `.webloc`) contribute names and URLs.
Launch commands and icon paths are skipped.
Index path:
`data\search_index`. Watcher + bootstrap crawl. `.orchid` Clean-Text is
extracted when enabled.

Admin: [configuration](../admin/configuration.md).
