# libmpv (Windows x64)

Orchid loads libmpv at runtime for in-app audio/video playback in the Media
viewer.

Download a prebuilt Windows x64 **dev** package (includes `mpv-1.dll` or
`libmpv-2.dll`) from [mpv player Windows builds](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/)
and place the DLL(s) at:

```
third-party/mpv/win-x64/mpv-1.dll
```

or:

```
third-party/mpv/win-x64/libmpv-2.dll
```

Copy any companion DLLs from the package into the same folder. The `orchid-app`
build script stages every `*.dll` next to `orchid.exe` under `target/<profile>/`.

Without libmpv, the Media viewer falls back to metadata chrome plus “Open in
the system player”.

See [docs/BUILDING.md](../../docs/BUILDING.md) for full setup instructions.
