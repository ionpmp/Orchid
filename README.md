# Orchid

> A touch-first computing environment for Windows where gestures, commands, and widgets are three representations of the same action.

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status: Pre-Alpha](https://img.shields.io/badge/status-pre--alpha-red.svg)](docs/ROADMAP.md)
[![CI](https://github.com/ionpmp/Orchid/actions/workflows/ci.yml/badge.svg)](https://github.com/ionpmp/Orchid/actions/workflows/ci.yml)

**Orchid** is an alternative user environment for Windows. It unifies a graphical workspace, a command palette, and a file manager into one shell. It is designed first for touch devices (Surface, 2-in-1 laptops, tablets) and is equally usable with mouse, keyboard, and pen.

This repository is **pre-alpha** (`0.1.0` workspace version). There is no tagged release yet. What ships is the in-tree desktop binary; planned work lives only in the [roadmap](docs/ROADMAP.md).

## Philosophy

Every gesture has a textual command. Every command can spawn a graphical widget. Control, automation, and visualization are three forms of the same action.

## What works today

- **Workspace shell** — up to nine workspaces, 16×10 widget grid, catalog, dock, tab groups, in-app window manager, cinema control kit
- **File manager** — dual-pane, tags, virtual folders, archives, encryption, managed folders, rclone mounts (RC keep-alive for list/stat), Find, **Wrap as .orchid**
- **Viewers** — images, PDF (pdfium), text (Tree-sitter), archives, HTML (WebView2 overlay), media (libmpv), Tier-1 DOCX / native `.orchid` editor
- **Browser** — catalog widget with WebView2 (tabs, bookmarks, find-in-page)
- **Terminal** — PowerShell, cmd, WSL, SSH; tabs and splits
- **Widgets** — weather, moon, Jyotish, clock, system, processes, calculator, notes, calendar, RSS, search, passwords, audio player (synced lyrics panel), video player, now-playing, recent files
- **Search** — Tantivy full-text + universal search; hybrid ANN exists in-crate (stub embedder) and is not the universal-search UI yet
- **`.orchid` container** — sealed and linked (CAS) files, age encryption, C2PA, CRDT structured region, stub embeddings (`orchid-format` / `orchid-embed`)
- **Security** — KDBX4 vault, Windows Hello, age encrypt/reveal
- **Theming & i18n** — nine bundled themes + JSON user themes, 11 Fluent locales including RTL (`ar-SA`)

How to use it: [User guide](docs/user/README.md). How to deploy and configure it: [Admin guide](docs/admin/README.md).

## Technology stack

| Layer | Technology |
|---|---|
| Language | Rust (MSRV 1.98) |
| GUI | Slint + Skia (Ganesh, winit-skia) |
| Storage | redb (state) + KDBX4 (passwords) + files (CAS chunks) |
| Terminal | portable-pty + custom vte emulator |
| Encryption | age (rage) |
| Content addressing | BLAKE3 + FastCDC |
| Search | Tantivy (+ ANN/RRF in `orchid-search`, stub embedder) |
| Documents | OOXML + parley/swash; native `.orchid` |
| PDF | pdfium-render |
| Media | libmpv |
| HTML / browser | WebView2 |
| Network FS | rclone (`rcd` keep-alive + CLI transfers) |
| Configuration | TOML |

## Status

**Pre-alpha.** Active development toward v0.1. Planned work (AI agents, plugins, shell replacement, ORT embeddings, …): [`docs/ROADMAP.md`](docs/ROADMAP.md). Release notes: [`CHANGELOG.md`](CHANGELOG.md).

## System requirements

- Windows 10 (1809+) or Windows 11
- x86_64 (CI and bundled native DLLs target x64; ARM64 is a goal)
- 4 GB RAM minimum, 8 GB recommended
- GPU with DirectX 11+ (Skia)
- 500 MB free disk space

Optional at runtime: `pdfium.dll`, libmpv, `rclone`, 7-Zip, WebView2 Evergreen (usually already installed).

## Building from source

```bash
git clone https://github.com/ionpmp/Orchid.git
cd Orchid
cargo build --release -p orchid-app
```

Binary: `target/release/orchid.exe`. DLLs, profiles, WebView2: [`docs/BUILDING.md`](docs/BUILDING.md). Desktop install: [`docs/admin/install.md`](docs/admin/install.md).

## Documentation

Index: [`docs/README.md`](docs/README.md)

| Document | Audience |
|---|---|
| [User guide](docs/user/README.md) | People using Orchid |
| [Admin guide](docs/admin/README.md) | Install, config, data, operations |
| [Roadmap](docs/ROADMAP.md) | Planned work only |
| [Changelog](CHANGELOG.md) | What changed in the tree |
| [Building](docs/BUILDING.md) | Developers |
| [Architecture](docs/ARCHITECTURE.md) | Crate map |
| [Contributing](docs/CONTRIBUTING.md) | Contributors |
| [`.orchid` format](docs/ORCHID_FORMAT.md) | Format implementers |
| [Security](docs/SECURITY.md) | Security researchers |
| [Code of Conduct](docs/CODE_OF_CONDUCT.md) | Community |

## License

Orchid is distributed under the [GNU Affero General Public License v3.0 or later](LICENSE)
(`AGPL-3.0-or-later` in `Cargo.toml`).

## Community

- **[Issues](https://github.com/ionpmp/Orchid/issues)** — bugs and feature requests
- **[Discussions](https://github.com/ionpmp/Orchid/discussions)** — ideas and questions
- **[Security advisories](https://github.com/ionpmp/Orchid/security/advisories)** — private vulnerability reports

---

*"Every gesture becomes a command. Every command becomes a widget."*
