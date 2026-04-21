# orchid-terminal

Terminal subsystem for Orchid. `portable-pty` hosts the child process (PowerShell, cmd, WSL, SSH) and bridges stdin / stdout / resize events; `alacritty_terminal` handles VT parsing and maintains the grid that the UI layer renders.

Inline graphics protocols (sixel, kitty) will be added in a later stage via `wezterm-term`; this crate owns the API shape so that integration does not ripple through the rest of the workspace.
