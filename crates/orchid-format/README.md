# orchid-format

Native `.orchid` container (`application/vnd.orchid`). Spec:
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md).

Phase 1–2: sealed + linked framing, FlatBuffers TOC, zstd Clean-Text /
Structured, optional per-region age encryption, `memmap2` open, and CLI.

## Schema

FlatBuffers TOC: [`schema/orchid_toc.fbs`](schema/orchid_toc.fbs).
Generated Rust lives in `src/toc_generated.rs` (regenerate with `flatc --rust`).

## CLI

```bash
cargo run -p orchid-format -- create \
  --output sample.orchid \
  --clean-text body.txt \
  --structured doc.bin \
  --raw attachment.bin \
  --passphrase 'secret'

cargo run -p orchid-format -- read sample.orchid \
  --passphrase 'secret' \
  --dump-clean-text out.txt
```

Library: `write_sealed_file` / `write_linked_file` / `SealedFile::open` /
`linked_region_plaintext` / `sealed_to_linked` / `linked_to_sealed`.
