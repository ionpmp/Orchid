# orchid-ui

UI layer for Orchid, built on [Slint](https://slint.dev/) with the Skia renderer. Slint component files live under `ui/` and are compiled into Rust by `build.rs` (via `slint-build`) so they can be imported with `slint::include_modules!()`.

Only the high-level windows and the component tree live in this crate. Business logic stays in the non-UI crates and is surfaced through typed view-models that the components bind to.
