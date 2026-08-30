# Building Orchid

## Requirements

- **OS:** Windows 10 (1809+) or Windows 11. Building on Linux/macOS is possible, but the target is Windows.
- **Rust:** 1.98.0 or newer (pinned via `rust-toolchain.toml`)
- **Visual Studio Build Tools 2022** (or Visual Studio with the C++ workload) — required to build native dependencies (Skia, pdfium)
- **Git** for cloning

## Installing Dependencies

### Windows

1. Install [Rustup](https://rustup.rs/)
2. Install [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/downloads/) with the "Desktop development with C++" workload
3. Install [Git for Windows](https://git-scm.com/download/win)

### Additional System Libraries

**Pdfium (PDF viewing and search extraction)**

Orchid loads `pdfium.dll` at runtime via `pdfium-render`. For local development, download a prebuilt Windows x64 binary from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries/releases) and place it at:

```
third-party/pdfium/win-x64/pdfium.dll
```

The `orchid-app` build script copies this DLL next to `orchid.exe` under `target/<profile>/`. Without it, the PDF viewer shows an explanatory error and PDF text extraction in search is skipped.

**libmpv (in-app audio / video playback)**

Orchid loads libmpv at runtime for the Media viewer. Download a prebuilt Windows
x64 **libmpv** package from
[mpv-player-windows / libmpv](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/)
and place the primary DLL (and any companion DLLs from the package) at:

```
third-party/mpv/win-x64/mpv-1.dll
```

or `libmpv-2.dll` (depending on the package). The `orchid-app` build script
copies every `*.dll` from that folder next to `orchid.exe`. Without libmpv, the
Media viewer shows chrome and can still open the file in the system player.

Helper (requires 7-Zip):

```bash
python scripts/fetch_mpv.py
```

## Cloning

```bash
git clone https://github.com/ionpmp/Orchid.git
cd Orchid
```

## Building

```bash
# Debug (fast compilation, slow runtime)
cargo build

# Release (optimized)
cargo build --release
```

Binary: `target/release/orchid.exe`

## Running

```bash
cargo run --release
```

## Tests

```bash
cargo test --workspace
```

Targeted crates while iterating:

```bash
cargo test -p orchid-viewers
cargo test -p orchid-widgets
cargo test -p orchid-search
cargo test -p orchid-i18n
```

After changing Fluent keys, keep locale parity:

```bash
python scripts/i18n_sync_keys.py
```

## Linting

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

CI runs the same checks on Windows — see [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

## Troubleshooting

### Skia compile errors

Skia is built via `slint` with the `renderer-skia` feature. The first build can take 15–30 minutes. Use `sccache` to speed up subsequent builds:

```bash
cargo install sccache
$env:RUSTC_WRAPPER = "sccache"  # PowerShell
```

### `link.exe not found`

Install Visual Studio Build Tools 2022.
