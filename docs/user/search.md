# Search

1. **Universal Search** — overlay / widget
2. **File Manager Find** (`Alt+F7`) — see [file-manager.md](file-manager.md)

## Universal Search

Top edge swipe, or the Search widget. Sources: **files** (Tantivy BM25 fused
with in-memory ANN via reciprocal rank fusion, plus snippets), **commands**,
**settings**, **calculator** (`=`), **agent** (`?`), **calendar**, **Jyotish**.

The desktop app embeds those file hits with a compiled-in
quantized ONNX model (`orchid.onnx.hash.q.v1`, 64 dimensions): concept
tokens are hashed into 32 bins and an int8 matrix projects them. Builds
without the `ort` feature stay on the synonym stub (`orchid.stub.synonym.v1`).
Semantic recall works for indexed text after the indexer has extracted it.
A `.orchid` Embedding region is reused only when its model id matches the
embedder that opened the index; otherwise the extracted text is embedded
again. The ANN snapshot sits next to `data\search_index` and is named
`ann.stub.v1` for the stub, or `ann.<model-id>` for any other model, so a
model change does not mix vectors. Set `[search].sentence-model` to a
replacement graph that accepts `features` (`float32[1, 32]`) and returns
`embedding`. The path is read when the index opens.

A question that starts with `?` asks the agent in Settings → Agent. The
agent is off until you enable it. The desktop app sends the message to
Ollama (`http://127.0.0.1:11434` when the endpoint is empty) or to an
OpenAI-compatible `/chat/completions` URL, on the background job queue.
The reply is a notification, and the same turn is appended to the shared
transcript in `data/agent-chat.json` (about 40 messages). The Agent widget
shows that transcript. The model may read a local text file, list one
folder, and search the open file index. A proposed file write is stored
until you confirm it in the Agent widget; nothing is written before that.
There is no shell. Set a model name first. An API key is optional and is
stored with Windows DPAPI after you save it; leaving the key field blank
keeps the saved key.

## Index

Settings → Search, and `[search]` in `config.toml`: `included-roots`
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
skipped. Protocol Buffers (`.proto`), GraphQL (`.graphql`, `.gql`),
Prisma (`.prisma`), Nix (`.nix`), Terraform (`.tf`), HCL (`.hcl`), and
Zig (`.zig`) are indexed as source text. `.terraform.lock.hcl` still
contributes only provider addresses. Bibliography files (`.bib`, `.ris`) contribute titles, authors,
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
`packages.lock.json` contributes package ids. Versions and content hashes
are skipped.
`Podfile.lock` contributes pod names. Versions, commits, and checksums
are skipped.
`pdm.lock` contributes package names and summaries. Versions and file
hashes are skipped.
`mix.lock` contributes package names. Versions and checksums are skipped.
`flake.lock` contributes node names, owners, repos, and refs. narHash
values and revisions are skipped.
`bun.lock` contributes package names. Versions and integrity hashes are
skipped.
`deno.lock` contributes package names and remote module URLs. Integrity
hashes and revisions are skipped.
`Package.resolved` contributes package identities and repository URLs.
Revisions and versions are skipped.
`Cartfile.resolved` contributes repository names and URLs. Versions and
commits are skipped.
`.terraform.lock.hcl` contributes provider addresses. Versions and hashes
are skipped.
`Chart.lock` contributes dependency names and repositories. Versions and
the digest are skipped.
`MODULE.bazel.lock` contributes module and repository names. Integrity
hashes are skipped.
`requirements.txt` contributes package names. Versions and `--hash` values
are skipped.
`.env` contributes variable names. Values are skipped.
`.npmrc` contributes registry URLs. Auth tokens and passwords are skipped.
`.netrc` contributes machine names. Logins, passwords, and accounts are
skipped.
`.pypirc` contributes server names and repository URLs. Usernames and
passwords are skipped.
`.git-credentials` contributes host names. Usernames and passwords are
skipped.
`.aws/credentials` contributes profile names and regions. Access keys and
secrets are skipped.
`Cargo.lock` contributes package names. Versions, sources, and checksums
are skipped.
Index path:
`data\search_index`. Watcher + bootstrap crawl. `.orchid` Clean-Text is
extracted when enabled.

Admin: [configuration](../admin/configuration.md).
