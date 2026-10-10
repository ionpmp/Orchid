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
- **File manager** — dual-pane, tags, virtual folders (including Managed), Photos (people / events / albums), archives (ZIP, 7z, TAR including gz / xz / bz2), encryption, managed folders (block-clone ingest), rclone mounts (RC keep-alive for list/stat), cloud sign-in for Drive / personal OneDrive / Dropbox, Find, compare and sync, hashes, links, image and metadata tools, Windows attributes / ACL / shares / previous versions / BitLocker status, **Wrap as .orchid**
- **Viewers** — images (including tone-mapped HDR / OpenEXR and stored face boxes), PDF (pdfium, AcroForm fill, highlight, sticky notes), text (Tree-sitter), archives, HTML (WebView2 overlay), media (libmpv), spreadsheets (edit simple formulas), slide card preview, Tier-1 DOCX / native `.orchid` editor
- **Browser** — catalog widget with WebView2 (tabs, bookmarks, find-in-page)
- **Mail** — IMAP/SMTP widget: account wizard, DPAPI secrets, SQLite cache, HTML reading pane, IDLE, attachments
- **Terminal** — PowerShell, cmd, WSL, SSH; tabs and splits; built-in or Alacritty grid (Sixel, Kitty, OSC 52, OSC 7)
- **Widgets** — weather, moon, Jyotish, clock, system (60-sample graphs), processes (per-process graphs), Optimize, Protection, calculator, notes, calendar (one CalDAV collection), contacts (one CardDAV collection), RSS, search, agent, passwords, audio player (synced lyrics panel), video player, now-playing, recent files
- **Search** — Universal Search fuses Tantivy BM25 with ANN (compiled-in quantized ONNX, 64-d; synonym stub without the `ort` feature). `?` asks the agent. File Manager Find is a separate walk
- **Agent** — Ollama or an OpenAI-compatible chat API. Shared transcript, local file tools, writes only after confirmation. Off until enabled
- **`.orchid` container** — sealed and linked (CAS) files, age encryption, C2PA, CRDT structured region, embeddings (`orchid-format` / `orchid-embed`)
- **Security** — KDBX4 vault, Windows Hello, age encrypt/reveal, DPAPI for mail / agent / CalDAV / CardDAV / inline mount passwords
- **Theming & i18n** — nine bundled themes + JSON user themes and a four-palette marketplace, 11 Fluent locales including RTL (`ar-SA`)
- **Text mode** — `orchid --tui` lists a local folder and previews small text files. It does not open the desktop window
- **Policy and shell** — `policy.toml` beside the config file makes listed settings read-only. An optional https address refreshes that file at startup. `audit.log` stays on this computer. Settings can replace Explorer for this Windows user; `orchid --restore-shell` puts it back

How to use it: [User guide](docs/user/README.md). How to deploy and configure it: [Admin guide](docs/admin/README.md).

## Technology stack

| Layer | Technology |
|---|---|
| Language | Rust (MSRV 1.98) |
| GUI | Slint + Skia (Ganesh, winit-skia); `orchid --tui` uses ratatui |
| Storage | redb (state) + KDBX4 (passwords) + files (CAS chunks) |
| Terminal | portable-pty + vte grid, or Alacritty grid (`alacritty-grid`) |
| Encryption | age (rage) |
| Content addressing | BLAKE3 + FastCDC |
| Search | Tantivy + ANN/RRF (`orchid-search`); ONNX or stub (`orchid-embed`) |
| Mail | In-process IMAP/SMTP (`orchid-mail`), SQLite cache |
| Documents | OOXML + parley/swash; native `.orchid` |
| PDF | pdfium-render |
| Media | libmpv |
| HTML / browser / mail | WebView2 |
| Network FS | rclone (`rcd` keep-alive + CLI transfers) |
| Configuration | TOML |

## Status

**Pre-alpha `0.1.0`.** There is no tagged release yet. The guides describe this tree (14 crates, reviewed 2026-10-08). Planned work (mobile companion, plugins): [`docs/ROADMAP.md`](docs/ROADMAP.md). Release notes: [`CHANGELOG.md`](CHANGELOG.md).

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
