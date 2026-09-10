// Build script for orchid-ui.
//! Compiles the Slint entrypoint `ui/main.slint`. The rest of the component
//! tree (`theme_global.slint`, `kit.slint`, workspace shell, widgets, overlays)
//! is pulled in via Slint imports — shared design tokens live in `Theme` /
//! `crates/orchid-ui/src/theme/`.

/// The component tree nests deep enough that the Slint compiler overflows the
/// default 8 MiB build-script stack, so compilation runs on a roomier thread.
const COMPILER_STACK_SIZE: usize = 256 * 1024 * 1024;

fn main() {
    std::thread::Builder::new()
        .stack_size(COMPILER_STACK_SIZE)
        .spawn(|| {
            slint_build::compile("ui/main.slint").expect("Slint build failed for ui/main.slint")
        })
        .expect("spawn Slint compiler thread")
        .join()
        .expect("Slint compiler thread panicked");
}
