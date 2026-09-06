# orchid-format

Native `.orchid` container (`application/vnd.orchid`). Spec:
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md).

Phase 1–3: sealed + linked framing, FlatBuffers TOC, zstd Clean-Text /
Structured, optional per-region age encryption, C2PA Provenance (signed PNG
carrier), `memmap2` open, and CLI.

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
  --passphrase 'secret' \
  --sign-c2pa

cargo run -p orchid-format -- read sample.orchid \
  --passphrase 'secret' \
  --dump-clean-text out.txt \
  --dump-provenance provenance.png \
  --verify-c2pa
```

Library: `write_sealed_file` / `write_linked_file` / `SealedFile::open` /
`linked_region_plaintext` / `sealed_to_linked` / `linked_to_sealed` /
`sign_clean_text_provenance` / `verify_provenance_carrier`.
