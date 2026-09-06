# orchid-embed

Sentence embedding backends for Orchid Phase 5 hybrid search.

- Default: [`StubEmbedder`](src/stub.rs) — deterministic synonym-aware vectors
  for CI (no model download).
- Optional `ort` feature: reserved for a bundled quantized ONNX sentence model
  (asset parallel to `pdfium.dll`).

Used by `orchid-search` ANN + RRF fusion. Spec:
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md) §9.
