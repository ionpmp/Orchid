# Orchid Roadmap

## MVP (v0.1) — 6–8 months

### Core
- [ ] Workspace structure (Cargo workspace, crates split)
- [ ] State store (redb wrapper) + configuration (TOML)
- [ ] Event bus + command registry
- [ ] Minimal Slint + Skia window + theming + i18n infrastructure

### File Manager
- [ ] Dual-pane mode
- [ ] Views (icons, list, details, gallery)
- [ ] Tabs, breadcrumbs
- [ ] Drag-and-drop
- [ ] Virtual folders (Recent, Categories, Network)
- [ ] Inline rename, tags, color labels
- [ ] Quick filter

### Terminal
- [ ] PTY backend (portable-pty + ConPTY)
- [ ] Terminal emulation (alacritty_terminal)
- [ ] Tabs + splits
- [ ] PowerShell, cmd, WSL backends
- [ ] SSH sessions
- [ ] Inline graphics (sixel + kitty)

### Widgets
- [ ] Infrastructure (layouts, workspaces, lifecycle)
- [ ] Widget: Weather
- [ ] Widget: Moon (astronomy)
- [ ] Widget: System indicators
- [ ] Widget: Files (recent)
- [ ] Widget: Universal search
- [ ] Widget: Media player (audio/video)
- [ ] Widget: RSS feed
- [ ] Widget: Password manager
- [ ] Widget: Terminal

### Viewers
- [ ] Images (PNG, JPEG, WebP, AVIF, HEIC, BMP, GIF, SVG, RAW)
- [ ] PDF (pdfium)
- [ ] Text with syntax highlighting (Tree-sitter)
- [ ] Archives (browse + extract)

### Security
- [ ] Password manager (KDBX4 format, custom UX)
- [ ] File and folder encryption (age-based)
- [ ] Biometric unlock via Windows Hello

### Storage
- [ ] Content-addressed storage (BLAKE3 + FastCDC chunking)
- [ ] Deduplication in managed folders

### Network Clients
- [ ] SFTP / SMB / WebDAV / FTP via rclone
- [ ] Virtual folders in file manager

### Search
- [ ] Tantivy indexing
- [ ] File watcher for incremental updates
- [ ] Universal search (files + commands + settings)

### UX
- [ ] Theming (light/dark, density modes, hot-reload)
- [ ] Built-in themes (Orchid Light/Dark, Solarized, Nord, Catppuccin, High Contrast)
- [ ] Internationalization (11 languages, RTL)
- [ ] Adaptive layouts (profiles for different screens)
- [ ] Gestures (touch, pen, mouse)
- [ ] Keyboard shortcuts + leader-key mode
- [ ] Onboarding tour, hint mode, command palette

### Additional
- [ ] Jyotish module (optional, not in default widgets)

## v1.x — 9–18 months

- [ ] AI agents (Ollama + OpenAI API)
- [ ] Graphical resource monitor with history
- [ ] Extended notification system
- [ ] Built-in browser (WebView2)
- [ ] Lua scripting (mlua)
- [ ] Theme and widget marketplace

## v2.0 — Year 2

- [ ] Replace Winlogon\Shell as an option
- [ ] TUI mode (ratatui for SSH/low-spec machines)
- [ ] Mobile companion (Android/iOS)
- [ ] Plugin system (WASM, capability-based)
- [ ] Enterprise edition (centralized management)
