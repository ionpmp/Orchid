# Orchid Architecture

## High-Level Diagram

```
┌─────────────────────────────────────────────────────┐
│  UI Layer (Slint + Skia Ganesh)                     │
│  Widgets as native Slint components                 │
├─────────────────────────────────────────────────────┤
│  Orchid Core (Rust)                                 │
│  ├─ Event Bus (in-process channels)                 │
│  ├─ Command Registry (semantic actions)             │
│  ├─ Widget Manager                                  │
│  ├─ State Store (redb wrapper)                      │
│  ├─ FS Layer (local + network providers)            │
│  ├─ Crypto Layer (age for files, KDBX for passwords)│
│  └─ Search (Tantivy)                                │
├─────────────────────────────────────────────────────┤
│  Backend Processes (via Cap'n Proto)                │
│  ├─ rclone serve (network FS)                       │
│  └─ PTY subprocesses (via portable-pty)             │
└─────────────────────────────────────────────────────┘
```

## Principles

1. **Single binary, multi-process where it matters.** The core is a single Rust process. Subprocesses are used only where necessary: rclone (network code isolation), PTY (terminal nature), and in the future Ollama for LLM inference.

2. **Event → Action → Command.** Every input (touch, mouse, keyboard, pen, voice) is converted into a semantic Action. Each Action has a textual command representation and is reversible where possible.

3. **State in one place.** redb is the single store for runtime state. SQLite is used only for the password database (KDBX). Files are used for chunks of the deduplicated storage.

4. **Configuration is transparent.** TOML files, editable by humans. Power users should be able to share configurations easily.

5. **No plugins in MVP.** Everything is built in. The plugin system is planned for v2.0, designed based on real experience.

## Crate Structure

```
orchid/
├── Cargo.toml                   # workspace root
├── README.md                    # project overview (repo root)
├── docs/                        # documentation
├── crates/
│   ├── orchid-core/             # event bus, command registry, types
│   ├── orchid-storage/          # redb wrapper, config, state
│   ├── orchid-crypto/           # age, KDBX, content addressing
│   ├── orchid-fs/               # local FS, network providers, chunking
│   ├── orchid-search/           # Tantivy
│   ├── orchid-terminal/         # PTY + emulation
│   ├── orchid-viewers/          # PDF, images, text, archives
│   ├── orchid-widgets/          # widget infrastructure + built-in widgets
│   ├── orchid-i18n/             # localization
│   ├── orchid-ui/               # Slint UI layer
│   └── orchid-app/              # main binary, wires everything together
├── assets/                      # icons, fonts, default themes
├── locales/                     # translation files
└── tests/                       # integration tests across crates
```

## Detailed Architecture

- [SECURITY.md](SECURITY.md) — security model and reporting

Additional deep-dive documents (state storage, event bus, UI layer) are planned as the implementation stabilizes; until then, see the sections above and [DESIGN.md](DESIGN.md).
