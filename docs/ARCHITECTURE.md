# Orchid Architecture

This document describes the **current** workspace: 13 crates, a single
desktop process, rclone RC + CLI, PTY children, and WebView2 overlays.
Planned systems (Ollama agents, WASM plugins, Winlogon shell) are not in
the tree — see [ROADMAP.md](ROADMAP.md).

## High-level diagram

```
┌─────────────────────────────────────────────────────────────┐
│  UI Layer (Slint + Skia Ganesh, SLINT_BACKEND=winit-skia)   │
│  Workspace canvas, cinema kit, in-app window manager       │
│  WebView2 overlays (HTML viewer + Browser widget)          │
├─────────────────────────────────────────────────────────────┤
│  orchid-ui — composition root (OrchidApp) + window + themes   │
│  orchid-app — thin binary (tracing, Tokio, mimalloc,        │
│               single-instance named pipe)                  │
├─────────────────────────────────────────────────────────────┤
│  orchid-widgets — managers + builtins (incl. browser)      │
│  orchid-viewers — image / PDF / text / archive / DOCX /      │
│                   .orchid / media (libmpv) / HTML           │
│  orchid-terminal — PTY + vte emulator + session/layout      │
│  orchid-fs — local + rclone (rcd keep-alive + CLI)          │
│  orchid-search — Tantivy + ANN/RRF (UI still BM25)          │
│  orchid-format — native .orchid (Phases 1–5)                │
│  orchid-embed — StubEmbedder (ORT feature reserved)            │
│  orchid-crypto — age, KDBX4, BLAKE3 chunks, Hello / DPAPI   │
│  orchid-storage — redb state + TOML config + OrchidPaths    │
│  orchid-i18n — Fluent catalogues (11 locales)               │
│  orchid-core — EventBus, actions, commands, input, jobs       │
├─────────────────────────────────────────────────────────────┤
│  Subprocesses                                                │
│  ├─ rclone rcd (localhost HTTP) + per-transfer CLI         │
│  └─ PTY children (portable-pty; Windows Job Object cleanup) │
└─────────────────────────────────────────────────────────────┘
```

## Principles

1. **Single binary, subprocesses only where required.** rclone and PTY
   children. LLM inference is planned, not present.
2. **Event → Action → Command.** Touch, mouse, keyboard, and pen become a
   semantic `Action`. Registered commands have an `orc …` form. File-manager
   operations use internal action ids (`fs.copy`, …) and profile bindings;
   they are **not** all `orc fs …` verbs. Inventory:
   [commands.md](commands.md).
3. **State in one place.** redb (`state.redb`, schema **v2**). Vault:
   `passwords.kdbx`. Chunks under `data/chunks`. Config is TOML.
4. **No plugins in this release.** Everything is built in (v2.0 item).

## Crate map

```
orchid/
├── crates/
│   ├── orchid-core/
│   ├── orchid-storage/        # redb schema v2, config.toml
│   ├── orchid-crypto/
│   ├── orchid-fs/
│   ├── orchid-search/         # Tantivy + hybrid helpers
│   ├── orchid-terminal/
│   ├── orchid-viewers/
│   ├── orchid-widgets/
│   ├── orchid-format/        # .orchid container + CLI
│   ├── orchid-embed/
│   ├── orchid-i18n/
│   ├── orchid-ui/
│   └── orchid-app/           # orchid.exe
├── docs/
├── scripts/
└── third-party/               # pdfium, libmpv (not committed as blobs)
```

`orchid-app` boots logging, `OrchidPaths`, and `orchid_ui::OrchidApp`.
Composition lives in `OrchidApp::bootstrap`. A second `orchid.exe` forwards
argv paths over a Windows named pipe and exits.

## Persistence (Windows, typical)

| Store | Path | Contents |
|-------|-------|----------|
| `config.toml` | `%APPDATA%\Orchid\Orchid\config\config.toml` | Settings; hot-reloaded |
| `state.redb` | `…\data\state.redb` | Workspaces, widgets, groups, history, session, cache, tags. Schema **v2**. Extra: `crypto_chunk_refs`, `widget_groups` |
| `passwords.kdbx` | `…\data\passwords.kdbx` | KeePass vault |
| chunks | `…\data\chunks` | BLAKE3 + FastCDC (plaintext by design) |
| search index | `…\data\search_index` | Tantivy |
| logs | `…\data\logs` | Default filter `orchid=info` |
| network bookmarks | `…\data\network-bookmarks.toml` | Runtime mounts |

Codec for redb values: `bincode_reloaded` 3. Path table:
[admin/data-and-operations.md](admin/data-and-operations.md).

## Widget visibility and windows

- **Active** only on the active workspace and active group tab.
- Sleeping → Unloaded after idle (~30 min). Visible widgets are not paused
  by `last_touched`.
- `WindowPlacement`: grid or floating (cap 8). Undock / dock, snap,
  taskbar, Ctrl+Tab.
- RSS / weather fetch uses `BackgroundJobQueue` until the instance is closed.

## Network FS

List/stat prefer a long-lived **`rclone rcd`** HTTP server on localhost
(keep-alive). Transfers and `sync` still spawn the rclone CLI. Prefer
`rclone-remote` over inline passwords — [SECURITY.md](SECURITY.md).

## Related

- [DESIGN.md](DESIGN.md) — UX + cinema kit
- [SECURITY.md](SECURITY.md)
- Crate READMEs under `crates/*/README.md`
