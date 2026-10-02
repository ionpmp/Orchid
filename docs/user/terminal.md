# Terminal

Catalog **Terminal** (`terminal`). PTY + custom VT emulator (SGR, cursor,
erase, OSC 0/2/7, OSC 52 clipboard).

**Backends:** PowerShell, cmd, WSL, SSH (`ssh://`), Custom. Custom and SSH
extra args can spawn arbitrary processes.

**Layout:** tabs, horizontal/vertical splits, persisted with the widget.
Orchid-profile defaults: `Ctrl+Shift+H`/`J` split, `Ctrl+Shift+T` tab,
`Ctrl+Shift+W` close, `Ctrl+PageUp`/`PageDown` tabs.

Inline images: Sixel (`DCS q`) and Kitty graphics direct pixels (24-bit RGB,
32-bit RGBA, and PNG). zlib Kitty payloads (`o=z`) are skipped. Sixel, and
Kitty when `C=1`, move the cursor past the image. On Windows, PTY children
join a Job Object so the tree dies with Orchid.
