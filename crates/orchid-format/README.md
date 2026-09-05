# orchid-format

Native `.orchid` container (`application/vnd.orchid`). Spec:
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md).

Phase 1 (sealed framing) is implemented in this crate: header / `ORCR`
regions / FlatBuffers TOC / footer, zstd for Clean-Text + Structured,
`memmap2` open, and a small CLI.

## Schema

FlatBuffers TOC: [`schema/orchid_toc.fbs`](schema/orchid_toc.fbs).
Generated Rust lives in `src/toc_generated.rs` (regenerate with `flatc --rust`).

## CLI

```bash
cargo run -p orchid-format -- create \
  --output sample.orchid \
  --clean-text body.txt \
  --structured doc.bin \
  --raw attachment.bin

cargo run -p orchid-format -- read sample.orchid \
  --dump-clean-text out.txt
```

Library entry points: `write_sealed_file` / `SealedFile::open`.
