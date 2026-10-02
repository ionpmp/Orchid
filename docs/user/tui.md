# Text mode

`orchid --tui` opens a file list in the terminal. `orchid --tui C:\Users\me`
starts in that folder. A file path starts in its parent folder.

The list is local folders only. Enter opens a folder or, for a text file
up to 256 KiB, a scrollable preview. Other files stay in the list; open
those in the desktop app. Network mounts, the vault, and the viewers are
not in this mode. The desktop window does not start, and a running Orchid
window is left alone.

| Key | Action |
|-----|--------|
| Up / Down, j / k | Move |
| Enter | Open the folder or preview text |
| Backspace, Left | Parent folder |
| / | Type a name filter. Esc clears it |
| h | Hide or show names that start with `.` |
| r | Read the folder again |
| q | Quit |

Labels follow `[locale].language` in `config.toml`. From a console, the
release build attaches to that console. From a shortcut it opens its own
console window.
