# Terminal

Catalog **Terminal** (`terminal`). PTY + custom VT emulator (SGR, cursor,
erase, OSC 0/2/7, OSC 52 clipboard).

Settings → Terminal → **Terminal grid** (`[terminal].grid`) chooses the
grid for the next session. **Built-in** is the default. **Alacritty** parses
the same PTY bytes with `alacritty_terminal` and paints cells through the
same view. Both grids draw Sixel and direct Kitty images, copy text with
OSC 52, and store the OSC 7 directory. OSC 52 paste is ignored. zlib Kitty
payloads are skipped. A session keeps the grid it opened with.

**Backends:** PowerShell, cmd, WSL, SSH (`ssh://`), Custom. Custom and SSH
extra args can spawn arbitrary processes. The saved widget state keeps
PowerShell, cmd, the WSL distro, and the SSH host. A Custom command is
not stored: the next open uses the platform default (PowerShell on
Windows). An SSH session restores the host only; user, port, jump hosts,
identity file, and extra args are dropped.

**Layout:** tabs, horizontal/vertical splits, persisted with the widget.
Orchid-profile defaults: `Ctrl+Shift+H`/`J` split, `Ctrl+Shift+T` tab,
`Ctrl+Shift+W` close, `Ctrl+PageUp`/`PageDown` tabs.

Inline images: Sixel (`DCS q`) and Kitty graphics direct pixels (24-bit RGB,
32-bit RGBA, and PNG). zlib Kitty payloads (`o=z`) are skipped. Sixel, and
Kitty when `C=1`, move the cursor past the image. On Windows, PTY children
join a Job Object so the tree dies with Orchid.
