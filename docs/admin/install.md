# Install

## Platform

Windows 10 1809+ / Windows 11, **x64** (CI and native DLLs). ARM64 is a
goal, not the current ship path.

## From source

See [BUILDING.md](../BUILDING.md). Typical:

```powershell
cargo build --release -p orchid-app
.\scripts\install-desktop.ps1
```

Installs to `%LOCALAPPDATA%\Programs\Orchid` and creates a Desktop
shortcut. Companion `*.dll` files are copied next to `orchid.exe`. If the
exe is running, the script may stage `orchid.exe.new`.

The install script associates `.orchid` / `application/vnd.orchid` with
`orchid.exe` (per-user HKCU). A second process forwards paths to the
running instance over a named pipe.

Portable zip: `.\scripts\build-installer.ps1` → `dist\Orchid-<ver>-win64\`.

## Runtime companions

Place under `third-party/` **before** cargo build, or copy beside the exe:

| Component | If missing |
|----------|------------|
| `third-party/pdfium/win-x64/pdfium.dll` | PDF viewer error; no PDF extract |
| `third-party/mpv/win-x64/mpv-1.dll` or `libmpv-2.dll` | Media chrome; system-player handoff |
| rclone on `PATH` or `RCLONE_BIN` | Network mounts fail |
| 7-Zip | Some archive create/SFX paths unavailable |
| WebView2 Evergreen | HTML source fallback; Browser widget unavailable |

Do not commit DLL blobs. Uninstall the exe folder; user data stays under
`%APPDATA%\Orchid\Orchid\` until deleted.

Start on login: Settings → Open on startup / `[general].open-on-startup`.
Windows Action Center toasts: Settings → Windows notifications /
`[general].os-notifications` (opt-in).
