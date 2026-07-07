//! Embeds the application icon on Windows builds and stages Pdfium next to the
//! executable when a bundled copy is available.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    stage_pdfium_library();
    embed_windows_icon();
}

/// Copy `third-party/pdfium/win-x64/pdfium.dll` into `target/<profile>/` so
/// runtime binding via `Pdfium::bind_to_library()` finds it beside `orchid.exe`.
fn stage_pdfium_library() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = manifest_dir
        .join("../../third-party/pdfium/win-x64/pdfium.dll");

    println!("cargo:rerun-if-changed={}", source.display());

    if !source.is_file() {
        println!(
            "cargo:warning=Pdfium not found at {}; PDF viewing disabled until pdfium.dll is placed there (see docs/BUILDING.md)",
            source.display()
        );
        return;
    }

    let out = out_dir();
    let Some(target_profile_dir) = out.ancestors().nth(3) else {
        println!("cargo:warning=Could not resolve target profile dir for Pdfium staging");
        return;
    };

    let dest = target_profile_dir.join("pdfium.dll");
    if let Err(e) = fs::copy(&source, &dest) {
        println!("cargo:warning=Failed to copy Pdfium to {}: {e}", dest.display());
        return;
    }

    println!(
        "cargo:warning=Staged Pdfium for runtime loading at {}",
        dest.display()
    );
}

fn out_dir() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"))
}

fn embed_windows_icon() {
    #[cfg(windows)]
    {
        let icon = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/logo/orchid-icon.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        if icon.is_file() {
            let mut res = winres::WindowsResource::new();
            res.set_icon(icon.to_str().expect("icon path utf-8"));
            if let Err(e) = res.compile() {
                println!("cargo:warning=failed to embed app icon: {e}");
            }
        } else {
            println!(
                "cargo:warning=orchid-icon.ico not found at {}; skipping exe icon",
                icon.display()
            );
        }
    }
}
