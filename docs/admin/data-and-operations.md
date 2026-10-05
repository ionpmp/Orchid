# Data layout and operations

Qualifier `com` / org `Orchid` / app `Orchid` (`directories` crate).

| Role | Typical path |
|------|----------------|
| Config | `%APPDATA%\Orchid\Orchid\config` |
| `config.toml` | `…\config\config.toml` |
| `policy.toml` | `…\config\policy.toml` (read-only settings; optional) |
| `audit.log` | `…\config\audit.log` (local; not uploaded) |
| Themes / locale overlays | `…\config\themes\`, `…\config\locales\` |
| Data | `%APPDATA%\Orchid\Orchid\data` |
| `state.redb` | `…\data\state.redb` (schema **v2**) |
| Vault | `…\data\passwords.kdbx` + `passwords.master.dpapi` |
| Chunks | `…\data\chunks\` |
| Search index | `…\data\search_index\` (Tantivy + `ann.<model-id>`; stub is `ann.stub.v1`) |
| Mail | `…\data\mail\` (`accounts.json`, `secrets.dpapi`, `cache.db`) |
| Agent transcript | `…\data\agent-chat.json` (about 40 messages; not uploaded) |
| Face rectangles | `…\data\photo-faces.json` (no person names) |
| Telemetry | `…\data\telemetry.jsonl` (only when telemetry is on) |
| Network bookmarks | `…\data\network-bookmarks.toml` |
| Logs | `…\data\logs\` (Roaming `data_dir`, not LocalAppData) |
| Cache | `%LOCALAPPDATA%\Orchid\Orchid\cache` |
| Installed exe | `%LOCALAPPDATA%\Programs\Orchid` |

Calendar events, contact cards, notes, and widget layout live in
`state.redb`, not in separate files. CalDAV and CardDAV passwords are
DPAPI-wrapped inside that widget config.

`workspaces` / `widgets` config dirs are reserved (live layout is in redb).

## Operations

- **In-app backup:** command palette / `orc data export backup` writes a zip
  of the config directory (`config.toml`, `policy.toml`, `audit.log`, themes,
  locale overlays), `state.redb`, the vault, the Hello sidecar, chunks, and
  network bookmarks. It omits the search index, logs, cache, `data\mail\`,
  `agent-chat.json`, `photo-faces.json`, and `telemetry.jsonl`. The Hello
  sidecar and any DPAPI blobs are still bound to this Windows user and
  machine — restore on another PC needs the vault master password, and mail
  or agent secrets will not be in the zip.
- **Support bundle:** `orc diagnostics export` — sanitized `config.toml`
  (mount passwords redacted), recent logs, environment note. No vault,
  chunks, or mail cache.
- **Manual backup:** quit Orchid, copy `config\`, `state.redb`, vault files,
  `chunks\` if you use managed folders, `network-bookmarks.toml`,
  `data\mail\` if you use Mail, `agent-chat.json` if you want the transcript,
  `photo-faces.json` if you want stored face boxes, and `search_index` if
  you want to skip recrawl.
- **Rebuild search:** quit, delete `data\search_index`, restart.
- **Reset layout:** delete `state.redb` (config.toml survives). Deleting
  all of `data\` also drops the vault and chunks.
- Logs: `RUST_LOG` overrides (`orchid=info` default).

Chunks are **plaintext**. Encrypt live files or the volume if the disk is
untrusted.

Managed-folder ingest clones a chunk from the source when the volume
supports block clone (Windows) or `copy_file_range` (Linux), and copies
the bytes otherwise. Identical whole files on one volume become a hard
link. The chunk store still keeps the content.
