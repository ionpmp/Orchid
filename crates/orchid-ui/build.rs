// Build script for orchid-ui.
//! Compiles the Slint entrypoint `ui/main.slint`. The rest of the component
//! tree (`theme_global.slint`, workspace shell, widgets, overlays) is pulled
//! in via Slint imports — shared design tokens live in `Theme` /
//! `crates/orchid-ui/src/theme/`.

fn main() {
    slint_build::compile("ui/main.slint").expect("Slint build failed for ui/main.slint");
}
