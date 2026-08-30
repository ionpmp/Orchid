//! Embeds the application icon on Windows builds and stages Pdfium / libmpv
//! next to the executable when bundled copies are available.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    stage_pdfium_library();
    stage_mpv_library();
    embed_windows_icon();
}

/// Copy `third-party/pdfium/win-x64/pdfium.dll` into `target/<profile>/` so
/// runtime binding via `Pdfium::bind_to_library()` finds it beside `orchid.exe`.
fn stage_pdfium_library() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = manifest_dir.join("../../third-party/pdfium/win-x64/pdfium.dll");

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
        println!(
            "cargo:warning=Failed to copy Pdfium to {}: {e}",
            dest.display()
        );
        return;
    }

    println!(
        "cargo:warning=Staged Pdfium for runtime loading at {}",
        dest.display()
    );
}

/// Copy `third-party/mpv/win-x64/*.dll` into `target/<profile>/` so runtime
/// binding can load libmpv beside `orchid.exe`.
fn stage_mpv_library() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_dir = manifest_dir.join("../../third-party/mpv/win-x64");

    println!("cargo:rerun-if-changed={}", source_dir.display());

    if !source_dir.is_dir() {
        println!(
            "cargo:warning=libmpv not found at {}; media playback disabled until DLLs are placed there (see docs/BUILDING.md)",
            source_dir.display()
        );
        return;
    }

    let out = out_dir();
    let Some(target_profile_dir) = out.ancestors().nth(3) else {
        println!("cargo:warning=Could not resolve target profile dir for libmpv staging");
        return;
    };

    let Ok(entries) = fs::read_dir(&source_dir) else {
        println!(
            "cargo:warning=Could not read libmpv directory {}",
            source_dir.display()
        );
        return;
    };

    let mut copied = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.eq_ignore_ascii_case("dll")
            && !name
                .rsplit_once('.')
                .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("dll"))
        {
            continue;
        }
        let dest = target_profile_dir.join(name);
        if let Err(e) = fs::copy(&path, &dest) {
            println!(
                "cargo:warning=Failed to copy {} to {}: {e}",
                path.display(),
                dest.display()
            );
            continue;
        }
        copied += 1;
    }

    if copied == 0 {
        println!(
            "cargo:warning=No DLL files in {}; media playback disabled (see docs/BUILDING.md)",
            source_dir.display()
        );
    } else {
        println!(
            "cargo:warning=Staged {copied} libmpv DLL(s) into {}",
            target_profile_dir.display()
        );
    }
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
