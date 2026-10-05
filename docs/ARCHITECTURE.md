# Orchid Architecture

This document describes the **current** workspace: **14 crates**, one desktop
process, in-process IMAP/SMTP, rclone RC + CLI, PTY children, and WebView2
overlays. Planned systems (WASM plugins, mobile companion) are not in the
tree — see [ROADMAP.md](ROADMAP.md). A local `policy.toml` can lock settings;
it is not a management server.

Reviewed against the tree on **2026-10-05** (`0.1.0` pre-alpha, no tagged
release).

## High-level diagram

```
┌─────────────────────────────────────────────────────────────┐
│  UI Layer (Slint + Skia Ganesh, SLINT_BACKEND=winit-skia)   │
│  Workspace canvas, cinema kit, in-app window manager        │
│  WebView2 overlays (HTML viewer, Browser, Mail HTML bodies) │
├─────────────────────────────────────────────────────────────┤
│  orchid-ui — composition root (OrchidApp) + window + themes │
│  orchid-app — thin binary (tracing, Tokio, mimalloc,        │
│               single-instance named pipe, --tui,            │
│               --restore-shell)                              │
├─────────────────────────────────────────────────────────────┤
│  orchid-widgets — managers + builtins                       │
│  orchid-mail — IMAP/SMTP, autodiscover, SQLite cache        │
│  orchid-viewers — image / PDF / text / archive / OOXML /    │
│                   spreadsheet / .orchid / media / HTML      │
│  orchid-terminal — PTY + vte or Alacritty grid              │
│  orchid-fs — local + rclone (rcd keep-alive + CLI)          │
│  orchid-search — Tantivy + ANN/RRF hybrid                   │
│  orchid-embed — synonym stub, or bundled ONNX (`ort`)       │
│  orchid-format — native .orchid (Phases 1–5)                │
│  orchid-crypto — age, KDBX4, BLAKE3 chunks, Hello / DPAPI   │
│  orchid-storage — redb state + TOML config + OrchidPaths    │
│  orchid-i18n — Fluent catalogues (11 locales)               │
│  orchid-core — EventBus, actions, commands, input, jobs     │
├─────────────────────────────────────────────────────────────┤
│  Out of process                                              │
│  ├─ rclone rcd (localhost HTTP) + per-transfer CLI          │
│  ├─ PTY children (portable-pty; Windows Job Object cleanup) │
│  ├─ WebView2 (Evergreen runtime, in-process COM host)       │
│  └─ Optional HTTPS: agent endpoint, CalDAV, CardDAV,        │
│     mail servers, update check, telemetry, policy fetch     │
└─────────────────────────────────────────────────────────────┘
```

## Principles

1. **Single binary, subprocesses only where required.** rclone and PTY
   children leave the process. Mail, search, and the agent run in-process.
   The agent calls Ollama or an OpenAI-compatible HTTPS API. It does not
   load a local language model.
2. **Event → Action → Command.** Touch, mouse, keyboard, and pen become a
   semantic `Action`. Registered commands have an `orc …` form. File-manager
   operations use internal action ids (`fs.copy`, …) and profile bindings;
   they are **not** all `orc fs …` verbs. Inventory:
   [commands.md](commands.md).
3. **State in one place.** redb (`state.redb`, schema **v2**). Vault:
   `passwords.kdbx`. Chunks under `data/chunks`. Mail has its own SQLite
   cache and DPAPI secret file. Config is TOML.
4. **No plugins in this release.** Everything is built in (v2.0 item).

## Crate map

```
orchid/
├── crates/
│   ├── orchid-core/
│   ├── orchid-storage/        # redb schema v2, config.toml, policy.toml
│   ├── orchid-crypto/
│   ├── orchid-fs/
│   ├── orchid-search/         # Tantivy + hybrid helpers
│   ├── orchid-embed/          # stub or ONNX (`ort`)
│   ├── orchid-terminal/
│   ├── orchid-viewers/
│   ├── orchid-widgets/        # builtins, including mail UI logic
│   ├── orchid-mail/           # IMAP/SMTP engine used by the mail widget
│   ├── orchid-format/         # .orchid container + CLI
│   ├── orchid-i18n/
│   ├── orchid-ui/
│   └── orchid-app/            # orchid.exe
├── docs/
├── scripts/
└── third-party/               # pdfium, libmpv (not committed as blobs)
```

`orchid-app` boots logging, `OrchidPaths`, and `orchid_ui::OrchidApp`.
Composition lives in `OrchidApp::bootstrap`. A second `orchid.exe` forwards
argv paths over a Windows named pipe and exits. `orchid --tui` never opens
the desktop window. `orchid --restore-shell` restores the per-user Winlogon
shell and exits before the single-instance check.

`register_core` / `register_all` in `orchid-widgets` cover widgets that do
not need the file-system stack. The desktop bootstrap registers those plus
terminal, mail (`MailEngine`), passwords, viewer, file manager, and recent
files. Mail is not in `register_all` because it needs a `MailEngine`.

## Persistence (Windows, typical)

| Store | Path | Contents |
|-------|------|----------|
| `config.toml` | `%APPDATA%\Orchid\Orchid\config\config.toml` | Settings; hot-reloaded |
| `policy.toml` | `…\config\policy.toml` | Read-only locks for Settings. Optional https refresh into this file |
| `audit.log` | `…\config\audit.log` | Policy apply, update checks, shell changes. Not uploaded |
| themes / locales | `…\config\themes\`, `…\config\locales\` | JSON themes and Fluent overlays |
| `state.redb` | `…\data\state.redb` | Workspaces, widgets, groups, history, session, cache, tags, widget config (calendar, contacts, notes, …). Schema **v2**. Extra tables: `crypto_chunk_refs`, `widget_groups` |
| `passwords.kdbx` | `…\data\passwords.kdbx` | KeePass vault |
| Hello sidecar | `…\data\passwords.master.dpapi` | Machine/user bound |
| chunks | `…\data\chunks` | BLAKE3 + FastCDC (plaintext by design) |
| search index | `…\data\search_index` | Tantivy + `ann.<model-id>` (`ann.stub.v1` for the stub) |
| mail | `…\data\mail\` | `accounts.json`, `secrets.dpapi`, `cache.db` |
| agent transcript | `…\data\agent-chat.json` | About 40 messages. Not uploaded |
| face boxes | `…\data\photo-faces.json` | Rectangles from Windows. No person names |
| telemetry | `…\data\telemetry.jsonl` | Opt-in `app-start` lines |
| network bookmarks | `…\data\network-bookmarks.toml` | Runtime mounts |
| logs | `…\data\logs` | Default filter `orchid=info` |
| cache | `%LOCALAPPDATA%\Orchid\Orchid\cache` | Rebuildable |

Codec for redb values: `bincode_reloaded` 3. CalDAV and CardDAV passwords
are DPAPI-wrapped inside the widget config in `state.redb`. Mail passwords
and OAuth refresh tokens are the DPAPI blob `secrets.dpapi`, not widget
state. Path table and backup omissions:
[admin/data-and-operations.md](admin/data-and-operations.md).

## Widget visibility and windows

- **Active** only on the active workspace and active group tab.
- Sleeping → Unloaded after idle (~30 min). Visible widgets are not paused
  by `last_touched`.
- `WindowPlacement`: grid or floating (cap 8). Undock / dock, snap,
  taskbar, Ctrl+Tab.
- RSS and weather fetch uses `BackgroundJobQueue` until the instance is closed.
  Mail IDLE runs on the widget task (about 90 seconds); a change schedules
  another sync on that queue. Calendar and contacts sync spawn Tokio tasks.
- Universal Search `?` and the Agent widget share `agent-chat.json`. Tools
  are `read_file`, `list_dir`, `search`, and `propose_write`. A proposed
  write stays pending until the user confirms. There is no shell tool.

## Search and embeddings

Universal Search **files** call `SearchEngine::search_hybrid`: Tantivy BM25
fused with an in-memory ANN index by reciprocal rank fusion. The desktop
app enables `orchid-embed`'s `ort` feature and embeds with the compiled-in
quantized graph `orchid.onnx.hash.q.v1` (32 concept-hash bins → 64-d).
Builds without `ort` stay on the synonym stub. A stored `.orchid` Embedding
region is reused only when its model id matches. `[search].sentence-model`
replaces the graph when the file accepts `features` and returns `embedding`.
The ANN snapshot is named from that model id.

Other universal-search sources are commands, settings, calculator (`=`),
agent (`?`), calendar, and Jyotish. File Manager Find is a separate walk
(Windows Search, then Tantivy) and is not the hybrid overlay.

## Network FS and mail

List/stat prefer a long-lived **`rclone rcd`** HTTP server on localhost
(keep-alive). Transfers and `sync` still spawn the rclone CLI. Prefer
`rclone-remote` over inline passwords — [SECURITY.md](SECURITY.md). Inline
mount passwords are stored as `dpapi:<hex>` and are still placed on the
rclone argv at use time.

Mail speaks IMAP and SMTP itself (rustls). Implicit TLS and STARTTLS are
accepted. Cleartext login is refused. HTML bodies render in a WebView2
overlay with scripts disabled.

## Related

- [DESIGN.md](DESIGN.md) — UX + cinema kit
- [SECURITY.md](SECURITY.md)
- [user/widgets.md](user/widgets.md) — catalog behavior
- Crate READMEs under `crates/*/README.md`
