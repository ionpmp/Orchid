# orchid-terminal

Terminal subsystem for Orchid.

## Architecture

- `backend` — shell / WSL / SSH launch specs (`BackendSpec`, `SshTarget`).
- `pty` — thin async-friendly wrapper around `portable-pty` with a resizable PTY, a background reader task that streams 8 KiB byte chunks, and a writer task that takes user keystrokes.
- `emulator` — VT / ANSI state machine built directly on `vte::Parser`. We deliberately do **not** pull in `alacritty_terminal` for its full grid model — its API surface has churned across versions and Orchid only needs the subset covered here (SGR, cursor movement, erase, scroll region, OSC 0/2/7). More advanced features (vi mode, regex scrollback search, full xterm coverage) can drop in later without breaking the public API. A TODO tracks the migration path.
- `input` — keyboard, paste, and mouse encoders. Bracketed-paste guard rejects injection attempts, normalises CRLF.
- `session` — end-to-end lifecycle: spawn a backend, run emulator + reader task, persist / restore through `orchid-storage`.
- `layout` — pure data model for tabs + split trees (UI-agnostic).

## Cleanup model

When the Orchid process exits, spawned child processes are terminated via `portable-pty`'s `Child::kill` as part of session close. On Windows, each PTY child is also assigned to a Job Object created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; the job handle is owned by `PtyHandle`, so the whole process tree is killed when Orchid exits or the handle is dropped. Other platforms rely on explicit shutdown alone.

## OSC coverage

- OSC 0, 1, 2 — window title (emits `TerminalTitleChanged`).
- OSC 7 — working directory (emits `TerminalCwdChanged`).
- OSC 52 — clipboard write. Emits `TerminalClipboardWrite`; `orchid-ui` subscribes and copies the payload to the system clipboard via `arboard`.
