# Building Orchid

## Requirements

- **OS:** Windows 10 (1809+) or Windows 11. Building on Linux/macOS is possible, but the target is Windows.
- **Rust:** 1.82.0 or newer (pinned via `rust-toolchain.toml`)
- **Visual Studio Build Tools 2022** (or Visual Studio with the C++ workload) — required to build native dependencies (Skia, pdfium)
- **Git** for cloning

## Installing Dependencies

### Windows

1. Install [Rustup](https://rustup.rs/)
2. Install [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/downloads/) with the "Desktop development with C++" workload
3. Install [Git for Windows](https://git-scm.com/download/win)

### Additional System Libraries

These will be listed as dependencies are added.

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

## Linting

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

## Troubleshooting

### Skia compile errors

Skia is built via `slint` with the `renderer-skia` feature. The first build can take 15–30 minutes. Use `sccache` to speed up subsequent builds:

```bash
cargo install sccache
$env:RUSTC_WRAPPER = "sccache"  # PowerShell
```

### `link.exe not found`

Install Visual Studio Build Tools 2022.
