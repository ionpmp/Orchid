# Orchid

> A touch-first computing environment for Windows where gestures, commands, and widgets are three representations of the same action.

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status: Pre-Alpha](https://img.shields.io/badge/status-pre--alpha-red.svg)](docs/ROADMAP.md)
[![CI](https://github.com/ionpmp/Orchid/actions/workflows/ci.yml/badge.svg)](https://github.com/ionpmp/Orchid/actions/workflows/ci.yml)

**Orchid** is an alternative user environment for Windows that unifies the graphical interface and command line into a single workspace. It is designed primarily for touch devices (Surface, 2-in-1 laptops, tablets) but is equally comfortable with mouse, keyboard, and pen input.

## Philosophy

Every gesture performed with a finger has a textual representation as a command. Every command can spawn a graphical widget. Control, automation, and visualization are three forms of the same action.

## Key Features (MVP, pre-alpha)

- **File Manager** — dual-pane, touch-friendly; tags, virtual folders, encryption, managed folders, rclone network mounts
- **Built-in Terminal** — PowerShell, cmd, WSL, SSH; tabs and splits (inline sixel/kitty graphics planned for v1.x)
- **Widget System** — dashboard grid, workspaces, tab groups, in-app window manager (float / dock / snap / taskbar)
- **Document Editor** — Tier-1 DOCX (Preview/Source, tables, images, Find/Replace); native `.orchid` format [specced](docs/ORCHID_FORMAT.md)
- **Viewers** — images, PDF, text (Tree-sitter + MVP edit), archives
- **Password Manager** — KDBX4, biometric unlock via Windows Hello
- **File Encryption** — age-based encrypt / decrypt / reveal for files and folders
- **Deduplication** — content-addressed storage via BLAKE3 + FastCDC
- **Search** — Tantivy full-text + universal search (files, commands, settings)
- **Built-in Widgets** — weather, moon, system, processes, calculator, world clock, notes, calendar, media, RSS, recent files, Jyotish (Vedic panchanga)
- **Theming** — light/dark, density modes, nine bundled themes, hot-reload
- **Internationalization** — 11 languages out of the box, RTL support
- **Gestures** — touch, pen, mouse, and keyboard as first-class input

## Technology Stack

| Layer | Technology |
|---|---|
| Language | Rust (MSRV 1.97) |
| GUI | Slint |
| Rendering | Skia (Ganesh backend via Slint) |
| Storage | redb (state) + KDBX4 (passwords) + files (chunks) |
| Terminal | portable-pty + custom vte emulator |
| Encryption | age (rage) |
| Content Addressing | BLAKE3 + FastCDC |
| Search | Tantivy |
| Documents | OOXML + parley/swash preview |
| PDF | pdfium-render |
| Network FS | rclone CLI subprocesses |
| Configuration | TOML |

## Status

**Pre-Alpha** (`0.1.0` workspace version). Active MVP development toward v0.1.

- Roadmap: [`docs/ROADMAP.md`](docs/ROADMAP.md)
- Recent changes: [`CHANGELOG.md`](CHANGELOG.md)

## System Requirements

- Windows 10 (1809+) or Windows 11
- x86_64 or ARM64
- 4 GB RAM minimum, 8 GB recommended
- GPU with DirectX 11+ support (for Skia)
- 500 MB free disk space

## Building from Source

```bash
# Requires Rust 1.97+ and Visual Studio Build Tools (C++ workload)
git clone https://github.com/ionpmp/Orchid.git
cd Orchid
cargo build --release
```

Binary: `target/release/orchid.exe`. For PDF viewing, place `pdfium.dll` as described in [`docs/BUILDING.md`](docs/BUILDING.md).

## Documentation

Index: [`docs/README.md`](docs/README.md)

| Doc | Audience |
|---|---|
| [Changelog](CHANGELOG.md) | Users & contributors |
| [Roadmap](docs/ROADMAP.md) | Users & contributors |
| [Building](docs/BUILDING.md) | Developers |
| [Architecture](docs/ARCHITECTURE.md) | Developers |
| [Contributing](docs/CONTRIBUTING.md) | Contributors |
| [Design Philosophy](docs/DESIGN.md) | Designers & contributors |
| [`.orchid` format](docs/ORCHID_FORMAT.md) | Format implementers |
| [Jyotish](docs/jyotish.md) | Jyotish widget users |
| [Security](docs/SECURITY.md) | Security researchers |
| [Code of Conduct](docs/CODE_OF_CONDUCT.md) | Community |

## License

Orchid is distributed under the [GNU Affero General Public License v3.0 or later](LICENSE)
(`AGPL-3.0-or-later` in `Cargo.toml`).

## Community

- **[Issues](https://github.com/ionpmp/Orchid/issues)** — bugs and feature requests (use the issue templates when filing)
- **[Discussions](https://github.com/ionpmp/Orchid/discussions)** — ideas and questions
- **[Security advisories](https://github.com/ionpmp/Orchid/security/advisories)** — private vulnerability reports

---

*"Every gesture becomes a command. Every command becomes a widget."*
