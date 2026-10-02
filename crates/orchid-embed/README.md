# orchid-embed

Sentence embedding backends for Orchid Phase 5 hybrid search.

- Default: [`StubEmbedder`](src/stub.rs) — deterministic synonym-aware vectors
  for CI (no model download).
- `ort` feature: ONNX Runtime plus the compiled-in quantized graph
  [`models/hash-q.onnx`](models/hash-q.onnx) (`orchid.onnx.hash.q.v1`,
  32 concept features → 64-d). The desktop app enables this feature.
  [`open_embedder`](src/runtime.rs) loads a replacement file when one is
  passed, and falls back to the stub if the runtime cannot start.

Used by `orchid-search` ANN + RRF fusion. Spec:
[`docs/ORCHID_FORMAT.md`](../../docs/ORCHID_FORMAT.md) §9.
