# Orchid

> A touch-first computing environment for Windows where gestures, commands, and widgets are three representations of the same action.

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status: Pre-Alpha](https://img.shields.io/badge/status-pre--alpha-red.svg)](https://github.com/PLACEHOLDER_ORG/orchid/blob/main/docs/ROADMAP.md)

**Orchid** is an alternative user environment for Windows that unifies the graphical interface and command line into a single workspace. It is designed primarily for touch devices (Surface, 2-in-1 laptops, tablets) but is equally comfortable with mouse, keyboard, and pen input.

## Philosophy

Every gesture performed with a finger has a textual representation as a command. Every command can spawn a graphical widget. Control, automation, and visualization are three forms of the same action.

## Key Features (MVP)

- 🗂️ **File Manager** — dual-pane, touch-friendly, with tags and virtual folders
- 💻 **Built-in Terminal** — PowerShell, cmd, WSL, SSH (inline sixel/kitty graphics planned for v1.x)
- 🧩 **Widget System** — desktop as a dashboard with configurable grid and workspaces
- 🔐 **Password Manager** — built-in, KDBX4 format, biometric unlock via Windows Hello
- 🛡️ **File Encryption** — mark a file or folder as encrypted, age-based cryptography
- 📦 **Deduplication** — content-addressed storage via BLAKE3
- 🌐 **Network Clients** — SFTP, SMB, WebDAV, FTP via rclone
- 👁️ **Viewers** — images, PDF, text with syntax highlighting for 100+ languages
- 🌙 **Built-in Widgets** — weather, moon, system indicators, media player, RSS, search
- 🎨 **Theming** — light/dark modes, density modes, hot-reload
- 🌍 **Internationalization** — 11 languages out of the box, RTL support
- ✋ **Gestures** — thoughtful system for touch, pen, mouse, and keyboard

## Technology Stack

| Layer | Technology |
|---|---|
| Language | Rust |
| GUI | Slint |
| Rendering | Skia (Ganesh backend via Slint) |
| Storage | redb (state) + KDBX4 file (passwords) + files (chunks) |
| Terminal | portable-pty + custom vte emulator |
| Encryption | age (rage) |
| Content Addressing | BLAKE3 + FastCDC |
| Search | Tantivy |
| PDF | pdfium-render |
| Network FS | rclone CLI subprocesses |
| Configuration | TOML |

## Status

**Pre-Alpha.** Active MVP development. See [`docs/ROADMAP.md`](docs/ROADMAP.md) for roadmap.

## System Requirements

- Windows 10 (1809+) or Windows 11
- x86_64 or ARM64
- 4 GB RAM minimum, 8 GB recommended
- GPU with DirectX 11+ support (for Skia)
- 500 MB free disk space

## Building from Source

```bash
# Requires Rust 1.97+
git clone https://github.com/PLACEHOLDER_ORG/orchid.git
cd orchid
cargo build --release
```

Detailed instructions: [`docs/BUILDING.md`](docs/BUILDING.md).

## Documentation

Index: [`docs/README.md`](docs/README.md)

- [Roadmap](docs/ROADMAP.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Design Philosophy](docs/DESIGN.md)
- [Contributor Guide](docs/CONTRIBUTING.md)
- [Code of Conduct](docs/CODE_OF_CONDUCT.md)
- [Security Policy](docs/SECURITY.md)

## License

Orchid is distributed under the [GNU Affero General Public License v3.0 or later](LICENSE)
(`AGPL-3.0-or-later` in `Cargo.toml`).

## Community

- **[Issues](https://github.com/PLACEHOLDER_ORG/orchid/issues)** — bugs and feature requests (use the issue templates when filing)
- **[Discussions](https://github.com/PLACEHOLDER_ORG/orchid/discussions)** — ideas and questions

---

*"Every gesture becomes a command. Every command becomes a widget."*
