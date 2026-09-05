# orchid-format

Native `.orchid` container (`application/vnd.orchid`). Spec:
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md).

Phase 1 (sealed framing) is under construction in this crate.

## Schema

FlatBuffers TOC: [`schema/orchid_toc.fbs`](schema/orchid_toc.fbs).
Generated Rust lives in `src/toc_generated.rs` (regenerate with `flatc --rust`).
