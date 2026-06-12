//! Embeds the application icon on Windows builds.

fn main() {
    #[cfg(windows)]
    {
        let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/logo/orchid-icon.ico");
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
