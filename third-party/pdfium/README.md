# Pdfium (Windows x64)

Orchid loads `pdfium.dll` at runtime for PDF viewing and search text extraction.

Download a prebuilt binary from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries/releases) and place:

```
third-party/pdfium/win-x64/pdfium.dll
```

The `orchid-app` build script copies this DLL next to `orchid.exe` under `target/<profile>/`.

See [docs/BUILDING.md](../../docs/BUILDING.md) for full setup instructions.
