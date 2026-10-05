# orchid-terminal

Terminal subsystem for Orchid.

## Architecture

- `backend` — shell / WSL / SSH launch specs (`BackendSpec`, `SshTarget`).
- `pty` — thin async-friendly wrapper around `portable-pty` with a resizable PTY, a background reader task that streams 8 KiB byte chunks, and a writer task that takes user keystrokes.
- `emulator` — default VT grid on `vte::Parser` (SGR, cursor, erase, scroll region, OSC 0/2/7, Sixel, Kitty). The `alacritty-grid` feature adds a second grid that feeds bytes into `alacritty_terminal` and copies cells into the same snapshot. The desktop app enables that feature. Both grids draw Sixel and direct Kitty images, copy text from OSC 52, and store OSC 7. OSC 52 paste and zlib Kitty payloads are ignored. `[terminal].grid` selects which one new sessions use. An open session keeps its grid.
- `input` — keyboard, paste, and mouse encoders. Bracketed-paste guard rejects injection attempts, normalises CRLF.
- `session` — end-to-end lifecycle: spawn a backend, run emulator + reader task, persist / restore through `orchid-storage`.
- `layout` — pure data model for tabs + split trees (UI-agnostic).

## Cleanup model

When the Orchid process exits, spawned child processes are terminated via `portable-pty`'s `Child::kill` as part of session close. On Windows, each PTY child is also assigned to a Job Object created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; the job handle is owned by `PtyHandle`, so the whole process tree is killed when Orchid exits or the handle is dropped. Other platforms rely on explicit shutdown alone.

## OSC coverage

- OSC 0, 1, 2 — window title (emits `TerminalTitleChanged`).
- OSC 7 — working directory (emits `TerminalCwdChanged`).
- OSC 52 — clipboard copy. Emits `TerminalClipboardWrite`; `orchid-ui` subscribes and copies the payload to the system clipboard via `arboard`. Paste requests are ignored. The Alacritty grid handles the same copy path.
