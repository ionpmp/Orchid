// Build script for orchid-ui.
//
// Compiles the Slint UI tree into Rust code so that the library can include it
// via `slint::include_modules!()` in a later stage. Right now only the
// minimal `ui/main.slint` stub exists; add additional compilation entries here
// as the UI grows.
//
// TODO(ui): wire the full component tree and set up a shared style module.

fn main() {
    slint_build::compile("ui/main.slint").expect("Slint build failed for ui/main.slint");
}
