# Orchid Architecture

## High-Level Diagram

```
┌─────────────────────────────────────────────────────┐
│  UI Layer (Slint + Skia Ganesh)                     │
│  Workspace dashboard, floating viewers, window mgr  │
│  Widgets as native Slint components                 │
├─────────────────────────────────────────────────────┤
│  Orchid (Rust workspace)                            │
│  ├─ orchid-core — event bus, actions, commands, BackgroundJobQueue │
│  ├─ orchid-storage — redb state + TOML config       │
│  ├─ orchid-fs — local + rclone network providers    │
│  ├─ orchid-crypto — age, KDBX4, BLAKE3 chunks       │
│  ├─ orchid-search — Tantivy + FS indexer            │
│  ├─ orchid-viewers — image/PDF/text/archive/DOCX    │
│  ├─ orchid-terminal — PTY + vte emulator            │
│  ├─ orchid-widgets — framework + builtins           │
│  ├─ orchid-i18n — Fluent catalogues (11 locales)    │
│  └─ orchid-ui / orchid-app — composition + window   │
├─────────────────────────────────────────────────────┤
│  Subprocesses (no Cap'n Proto yet)                  │
│  ├─ rclone CLI (network FS operations)              │
│  └─ PTY children (via portable-pty)                 │
└─────────────────────────────────────────────────────┘
```

## Principles

1. **Single binary, multi-process where it matters.** The core is a single Rust process. Subprocesses are used only where necessary: rclone (network code isolation), PTY (terminal nature), and in the future Ollama for LLM inference.

2. **Event → Action → Command.** Every input (touch, mouse, keyboard, pen, voice) is converted into a semantic Action. Each Action has a textual command representation and is reversible where possible.

3. **State in one place.** redb is the single store for runtime state. The password vault is a KDBX4 file (`passwords.kdbx`) via the `keepass` crate — not SQLite. Files are used for chunks of the deduplicated storage.

4. **Configuration is transparent.** TOML files, editable by humans. Power users should be able to share configurations easily.

5. **No plugins in MVP.** Everything is built in. The plugin system is planned for v2.0, designed based on real experience.

## Crate Structure

```
orchid/
├── Cargo.toml                   # workspace root
├── README.md                    # project overview (repo root)
├── CHANGELOG.md                 # notable changes
├── docs/                        # roadmap, architecture, format specs
├── crates/
│   ├── orchid-core/             # event bus, command registry, types
│   ├── orchid-storage/          # redb wrapper, config, state
│   ├── orchid-crypto/           # age, KDBX, content addressing
│   ├── orchid-fs/               # local FS, network providers, chunking
│   ├── orchid-search/           # Tantivy
│   ├── orchid-terminal/         # PTY + custom vte emulation
│   ├── orchid-viewers/          # PDF, images, text, archives, DOCX editor
│   ├── orchid-widgets/          # widget infrastructure + built-in widgets
│   ├── orchid-i18n/             # localization (Fluent, 11 locales)
│   ├── orchid-ui/               # Slint UI layer + window manager
│   └── orchid-app/              # main binary, wires everything together
├── assets/                      # icons, fonts, branding
└── tests/                       # reserved for future cross-crate integration tests
```

See also: [CHANGELOG.md](../CHANGELOG.md), [ROADMAP.md](ROADMAP.md),
[ORCHID_FORMAT.md](ORCHID_FORMAT.md) (native container, not yet implemented).

## Network FS note

Network mounts are implemented by spawning the **rclone CLI** (`lsjson`, `cat`, `rcat`, …) per operation. A long-lived `rclone serve` process and Cap'n Proto IPC are **not** in the tree yet; treat older diagrams that mention them as aspirational.

Prefer `rclone-remote` in `config.toml` (credentials in `rclone.conf`) over inline `password` fields — see [SECURITY.md](SECURITY.md).

## Widget visibility, windows, and background jobs

- **Visibility owns Active ↔ Sleeping.** A widget is active only when it is on the active workspace and is the active tab of its group (or not in a multi-member group). `visible_instance_ids` + `WidgetManager::apply_visibility` drive this; the UI calls sync after layout-changing actions (and once at bootstrap).
- **In-app window manager.** Each instance has a `WindowPlacement` (grid cell or floating overlay). Users undock / dock, minimize / maximize / restore, edge-snap, and cycle with Ctrl+Tab; state persists in redb (schema v2). Floating viewers (images, PDF, text, archives, DOCX) share the same chrome; catalog **Document** can open docked on the canvas.
- **Sleeping → Unloaded** remains an idle memory reclaim (~30 min). Idle Active → Sleeping was removed so visible widgets are not paused by `last_touched`.
- **UI-only timers** (`PeriodicRefresh`: media, system, password, moon) stop in `on_sleep`.
- **Always-on work** (RSS/weather network refresh; future AI agents) uses `orchid_core::BackgroundJobQueue` — interval jobs keyed by string, independent of widget visibility until the instance is closed.

## Detailed Architecture

- [SECURITY.md](SECURITY.md) — security model and reporting

Additional deep-dive documents (state storage, event bus, UI layer) are planned as the implementation stabilizes; until then, see the sections above and [DESIGN.md](DESIGN.md).
