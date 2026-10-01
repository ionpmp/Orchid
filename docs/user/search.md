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
`extract-text` also indexes source files (`.rs`, `.py`, `.js`,
`.ts`, `.ps1`, `.sql`, and similar), plus calendar (`.ics`) and
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
Subtitles (`.srt`, `.vtt`, `.ass`, `.ssa`, `.lrc`, `.smi`, `.ttml`)
contribute cue text.
Timestamps, cue numbers, and style overrides are skipped.
Media notes (`.nfo`) contribute titles, plots, and names when they are
Kodi XML. Poster URLs and stream details are skipped. A plain-text
scene note is indexed as text.
SVG drawings (`.svg`) contribute visible labels. Scripts, styles, and
metadata are skipped.
Registry exports (`.reg`) contribute key paths and string values,
including UTF-16 files. `dword` and `hex` values are skipped.
Java property files (`.properties`) contribute keys, values, and
comments. `\\uXXXX` escapes are decoded.
.NET resource files (`.resx`) and XLIFF (`.xlf`, `.xliff`) contribute
names, source text, and translations. Embedded binary values are skipped.
Apple string tables (`.strings`, `.stringsdict`) contribute keys,
translations, and block comments. Line comments are skipped.
Qt Linguist catalogs (a `.ts` file that opens as a translation file)
contribute context names, source strings, and translations. TypeScript
`.ts` files stay source text. Location filenames are skipped.
Android resource XML (`<resources>` with `<string>`, `<plurals>`, or
`<string-array>`) contributes resource names and visible text. Other
`.xml` files stay plain text, unless they open as a feed.
Windows resource scripts (`.rc`) contribute quoted UI strings from
string tables, dialogs, and menus. Comments and `#include` lines are
skipped.
Qt Designer and GTK Glade files (`.ui`) contribute window titles, labels,
and tooltips. Geometry and object ids are skipped.
XAML files (`.xaml`) contribute titles, text, content, and headers.
Element names and `x:Name` ids are skipped.
Manual pages (`.1`, `.3pm`, `.man`, and similar) contribute section titles
and body text. Comments and `.ig` blocks are skipped.
iOS storyboards and XIB files (`.storyboard`, `.xib`) contribute titles,
label text, placeholders, and user labels. Class names and object ids
are skipped.
Perl POD (`.pod`) contributes headings, items, and paragraphs. Code
after `=cut` and `=begin comment` blocks are skipped.
Texinfo manuals (`.texi`, `.texinfo`) contribute titles and body text.
`@ignore` blocks and `@c` comments are skipped.
Compiled gettext catalogs (`.mo`, `.gmo`) contribute message ids and
translations. The catalog header is skipped.
GNU Info manuals (`.info`, `.info-1`) contribute node titles and body
text. Tag tables are skipped.
systemd units (`.service`, `.socket`, `.mount`, and similar) contribute
descriptions, documentation, and start commands. Environment variables
and credentials are skipped.
XSPF playlists (`.xspf`) contribute titles, creators, albums, and
locations. Durations and vendor extensions are skipped.
WiX sources (`.wxs`, `.wxl`) contribute product names, feature titles,
dialog text, and localization strings. Component ids and property values
are skipped.
Torrent files (`.torrent`) contribute display names, comments, announce
URLs, and file paths. Piece hashes are skipped.
RPM spec files (`.spec`) contribute package names, summaries,
descriptions, changelogs, and file lists. Build scripts are skipped.
Debian source control (`.dsc`, `.changes`) contributes package names,
descriptions, and relationships. Checksums and file hashes are skipped.
RDoc files (`.rdoc`) contribute headings, lists, and paragraphs. Rules
and `:stopdoc:` blocks are skipped.
MSBuild projects (`.csproj`, `.fsproj`, `.vbproj`, `.vcxproj`) contribute
assembly names, descriptions, SDK ids, and package or project references.
Versions and source-file lists are skipped.
Diffs (`.diff`, `.patch`) contribute changed paths and line text. Git
blob hashes and binary patch bodies are skipped.
NuGet manifests (`.nuspec`) contribute package ids, titles, descriptions,
authors, and dependency ids. Versions, commits, and packed files are
skipped.
Visual Studio solutions (`.sln`) contribute project names, project paths,
and solution items. GUIDs and configuration tables are skipped.
Maven POM files (`pom.xml`) contribute group ids, artifact ids, names,
and descriptions. Versions and `properties` blocks are skipped.
XML solutions (`.slnx`) contribute project paths, folder names, and
solution-item paths. Build configurations are skipped.
Android manifests (`AndroidManifest.xml`) contribute package names,
labels, component names, and permissions. Versions and resource
references are skipped.
Property lists (`.plist`) contribute display names, identifiers, and
usage descriptions. Version and SDK values are skipped, and binary
plists are ignored.
Dockerfiles (`Dockerfile`, `.dockerfile`) contribute image names, labels,
and copy paths. `ENV` and `ARG` values are skipped.
`package.json` and `composer.json` contribute names, descriptions,
keywords, and dependency names. Scripts and versions are skipped.
`go.mod` contributes the module path and required module paths. Toolchain
and dependency versions are skipped.
`go.sum` contributes module paths. Versions and checksums are skipped.
`package-lock.json` contributes package names. Versions and integrity
hashes are skipped.
`yarn.lock` contributes package names. Versions and integrity hashes are
skipped.
`composer.lock` contributes package names and descriptions. Versions and
dist checksums are skipped.
`pnpm-lock.yaml` contributes package names. Versions and integrity hashes
are skipped.
`poetry.lock` contributes package names and descriptions. Versions and
file hashes are skipped.
`Gemfile.lock` contributes gem names. Versions, revisions, and checksums
are skipped.
`Pipfile.lock` contributes package names. Versions and hashes are skipped.
`pubspec.lock` contributes package names. Versions and sha256 checksums
are skipped.
`uv.lock` contributes package and dependency names. Versions and hashes
are skipped.
`Cargo.lock` contributes package names. Versions, sources, and checksums
are skipped.
Index path:
`data\search_index`. Watcher + bootstrap crawl. `.orchid` Clean-Text is
extracted when enabled.

Admin: [configuration](../admin/configuration.md).
