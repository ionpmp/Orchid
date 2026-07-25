# Orchid File Format (`.orchid`)

Specification for Orchid’s native container format. This document is the
design source of truth for implementers; no crate ships the format yet.
Related reading: [ARCHITECTURE.md](ARCHITECTURE.md), [SECURITY.md](SECURITY.md),
[ROADMAP.md](ROADMAP.md).

Status: **draft**. Version targets below refer to the *format* version
carried in the file header, not the Orchid application release.

---

## 1. Overview & Goals

`.orchid` is an AI-native file container intended to complement—and, for
Orchid-authored documents, eventually replace—DOCX and PDF as the primary
on-disk representation. It is not a general interchange format for every
desktop app; it is Orchid’s own package for content that the shell owns,
indexes, encrypts, and hands to agents.

### Goals

1. **Self-describing package.** One file (or a thin linked manifest) carries
   media, clean text, structured edit history, embeddings, provenance, and
   encryption metadata without sidecar sprawl.
2. **AI-first retrieval.** Hierarchical embeddings and token counts travel
   with the document so agents can RAG without re-tokenizing and re-embedding
   on every open.
3. **Reuse Orchid crypto primitives.** Content addressing uses the existing
   FastCDC + BLAKE3 pipeline (`crates/orchid-crypto/src/content/chunker.rs`)
   and `ChunkStore` (`crates/orchid-crypto/src/content/store.rs`). Encryption
   extends the existing `age_encryption` stack rather than inventing a new
   cipher suite.
4. **Resilient layout.** Per-region mini-headers allow linear recovery when
   the table of contents (TOC) is damaged.
5. **Sealed or linked.** Export and share as a self-contained sealed file;
   keep library copies as linked manifests that dedupe through `ChunkStore`.

### Primary scenarios

| Scenario | Behaviour |
| --- | --- |
| **Document editor native save** | Editor writes Structured (CRDT or snapshot), Clean-Text, optional Embeddings and Provenance into a `.orchid`. Default for “Save” inside Orchid is native; DOCX/PDF remain export targets. |
| **File manager “wrap as .orchid”** | User selects arbitrary files/folders; FM packs them into Raw region(s), derives Clean-Text where extractors exist, and optionally computes embeddings for search. |
| **Agent / search ingest** | `orchid-search` opens the file (mmap), reads Clean-Text + Embedding regions, indexes BM25 + ANN without unpacking a full DOCX/PDF pipeline. |
| **Encrypted share** | Private regions encrypted to age recipients; public regions (e.g. title digest, provenance stubs) remain readable for triage. |

### Non-goals (v1 framing)

- Replacing ZIP/OOXML as an Office interchange standard.
- Guaranteeing bit-identical round-trips through Microsoft Word.
- Streaming live collaborative protocol over the wire (CRDT ops are file-borne;
  sync transport is out of scope for this document).

---

## 2. Naming & Positioning

| Item | Value |
| --- | --- |
| File extension | `.orchid` |
| Magic bytes (header + terminator) | `ORCD` (ASCII `0x4F 0x52 0x43 0x44`) |
| MIME type | `application/vnd.orchid` |

### Why `.orchid`

The earlier working name `.omni` suggested a universal “everything” archive.
That framing competed with ZIP and confused users. `.orchid` ties the format
to the product: it is Orchid’s native document/package type, analogous to
how Blender owns `.blend`, Sketch owns `.sketch`, and Figma owns `.fig`.

| Format | Owner | Role |
| --- | --- | --- |
| `.blend` | Blender | Native scene; FBX/glTF for export |
| `.sketch` / `.fig` | Sketch / Figma | Native design doc; PDF/PNG for share |
| `.orchid` | Orchid | Native AI-aware package; DOCX/PDF for import/export |

Magic `ORCD` is short, ASCII-printable, and unlikely to collide with ZIP
(`PK`), PDF (`%PDF`), or OOXML (ZIP). MIME `application/vnd.orchid` follows
vendor MIME practice; register with IANA only if the format ships publicly
beyond the app.

On Windows, associate `.orchid` with Orchid via the installer; sniffers may
also key off the leading four bytes.

---

## 3. Container Layout

Byte order is **little-endian** unless noted. Offsets are absolute from the
start of the file unless described as “from end.”

### 3.1 High-level structure

```
┌──────────────────────────────────────────────┐
│ Header (fixed, 4 KiB-aligned block)          │
├──────────────────────────────────────────────┤
│ Region 0  (aligned)                          │
│ Region 1  (aligned)                          │
│ ...                                          │
├──────────────────────────────────────────────┤
│ TOC (FlatBuffers, aligned)                   │
├──────────────────────────────────────────────┤
│ Footer (fixed size)                          │
└──────────────────────────────────────────────┘
```

All major sections begin on a **4096-byte (4 KiB)** boundary. Padding between
sections is zeros. Alignment enables `memmap2` page-friendly mapping and
future partial decrypt of individual regions.

### 3.2 Header (offset 0)

Fixed layout, total size padded to 4096 bytes.

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| 0 | 4 | `magic` | `ORCD` |
| 4 | 2 | `version_major` | `u16` |
| 6 | 2 | `version_minor` | `u16` |
| 8 | 8 | `capability_flags` | `u64` bitfield (see §12) |
| 16 | 16 | `file_uuid` | RFC 4122 UUID (bytes) |
| 32 | 8 | `created_unix_ms` | `u64` UTC milliseconds since epoch |
| 40 | 8 | `header_flags` | reserved; write `0` |
| 48 | … | reserved / zero pad | through offset 4095 |

Initial format version: **1.0** (`major=1`, `minor=0`).

`file_uuid` identifies this logical document across sealed↔linked repacks and
generation chains. A new UUID is allocated only for a truly new document, not
for every save.

### 3.3 Regions

Each region is a self-describing blob:

```
┌────────────┬──────────┬──────────┬─────────────────┬──────────────┐
│ local_magic│ type_id  │ length   │ enc_meta?       │ payload      │
│ 4 bytes    │ u16      │ u64      │ variable        │ `length`     │
│ "ORCR"     │          │          │ (if encrypted)  │ bytes        │
└────────────┴──────────┴──────────┴─────────────────┴──────────────┘
```

| Field | Size | Description |
| --- | --- | --- |
| `local_magic` | 4 | ASCII `ORCR` (`0x4F 0x52 0x43 0x52`) — region marker for linear scan |
| `type_id` | 2 | Region taxonomy id (see §4) |
| `length` | 8 | `u64` byte length of `enc_meta || payload` that follows (not including the 14-byte mini-header) |
| `enc_meta` | 0 or N | Present when region capability / TOC marks encryption; see §7 |
| `payload` | rest | Possibly compressed and/or age-encrypted ciphertext |

Region start offsets in the TOC are absolute file offsets of `local_magic`.
After the payload, pad with zeros to the next 4 KiB boundary before the next
region (or TOC).

**Mini-header size:** 14 bytes (`4 + 2 + 8`). Recovery scanners search for
`ORCR` on 4 KiB-aligned offsets first, then fall back to a sliding scan
within each page if corruption shifted data.

### 3.4 TOC (FlatBuffers)

The TOC is a FlatBuffers root of type `Toc` (schema in §5). It lists every
region with absolute offset, type, plaintext BLAKE3, compression codec,
encryption info, and generation metadata.

The TOC blob itself begins on a 4 KiB boundary. Its byte length is recorded
implicitly by the footer (`toc_offset_from_end` and file size).

### 3.5 Footer

Fixed-size trailer at EOF (no padding after footer):

| Offset from EOF | Size | Field |
| --- | --- | --- |
| −44 | 8 | `toc_offset_from_end` — `u64` distance from EOF to start of TOC |
| −36 | 32 | `toc_blake3` — BLAKE3-256 of the TOC bytes |
| −4 | 4 | `terminator_magic` — `ORCD` |

Open algorithm:

1. Read last 44 bytes; verify terminator `ORCD`.
2. Seek to `file_len - toc_offset_from_end`; read TOC; verify BLAKE3.
3. Parse FlatBuffers; map regions (optionally via `memmap2`).

If the footer or TOC fails verification, enter recovery mode (§10).

---

## 4. Region Taxonomy

`type_id` values are stable. Unknown types with the high bit clear must be
skipped by readers that do not understand them (forward compatible). Types
with the high bit set (`0x8000`) are private experiments and must not ship
in released files.

| type_id | Name | Required? | Typical size | Purpose / payload |
| --- | ---: | --- | --- | --- |
| `0x0001` | **Raw** | Optional\* | KiB–GiB | Opaque media or original bytes: images, PDF, audio, nested files. Payload = optionally compressed bytes + TOC `content_type` hint (MIME). For FM wrap, one Raw region per top-level entry or a tar-like bundle (implementation choice; v1 prefers one region per file with TOC `name`). |
| `0x0002` | **Clean-Text** | Recommended | KiB–few MiB | UTF-8 plain text (LF newlines) suitable for search, TTS, and agent context. Derived from Structured or extracted from Raw. Compression: zstd. |
| `0x0003` | **Structured** | Optional\*\* | KiB–tens of MiB | Editor document model. v1: FlatBuffers/bincode snapshot of layout tree. Later: CRDT op log + compaction snapshot (§8). |
| `0x0004` | **Embedding** | Optional | MiB-scale | Hierarchical vectors + token counts (§9). Codec and model id in TOC. |
| `0x0005` | **Provenance** | Optional | KiB | C2PA / content credentials or Orchid-native attestation stubs (§ Phase 3). |
| `0x0006` | **Version-history** | Optional | KiB–MiB | Manifest of prior generations: UUIDs, CAS chunk hashes, timestamps, parent links. Used heavily in linked mode. |

\* For FM wrap of binary-only content, Raw is required; Clean-Text may be empty or absent if no extractor applies.  
\*\* For the document editor, Structured is required; Raw may hold exported media assets.

### Region roles

- **Raw** — fidelity / original bytes. Do not rely on Raw for search.
- **Clean-Text** — canonical searchable text. Indexers prefer this over
  re-extracting Raw.
- **Structured** — fidelity of edits, layout, comments, AI suggestions.
- **Embedding** — precomputed vectors for semantic search / RAG.
- **Provenance** — authenticity and edit lineage for third-party validators.
- **Version-history** — generation graph; pairs with `ChunkStore` in linked mode.

Multiple regions of the same type are allowed (e.g. several Raw files, or
Embedding shards). The TOC `name` / `ordinal` fields disambiguate.

---

## 5. TOC / FlatBuffers Schema

Draft schema (`orchid_toc.fbs`). Field ids are part of the wire contract;
append-only evolution rules of FlatBuffers apply.

```fbs
// orchid_toc.fbs — draft TOC for application/vnd.orchid

namespace orchid.format;

enum RegionType : uint16 {
  Raw = 1,
  CleanText = 2,
  Structured = 3,
  Embedding = 4,
  Provenance = 5,
  VersionHistory = 6,
}

enum CompressionCodec : uint8 {
  None = 0,
  Zstd = 1,
}

enum StorageMode : uint8 {
  Inline = 0,   // payload bytes live in this file (sealed)
  Linked = 1,   // payload is a chunk-hash list into ChunkStore
}

table EncryptionInfo {
  /// age recipient stanzas or opaque age header prefix length.
  age_header: [ubyte];
  /// BLAKE3-256 of the plaintext (after decompress).
  plaintext_blake3: [ubyte]; // length 32
  /// Identity kind hint: 0 = passphrase, 1 = x25519 (mirrors IdentityKind).
  identity_kind: uint8;
  /// True if this region is classified public (may still be integrity-hashed).
  is_public: bool;
}

table ChunkRef {
  blake3: [ubyte]; // 32
  size: uint64;
}

table RegionEntry {
  type: RegionType;
  /// Absolute file offset of local_magic ORCR.
  offset: uint64;
  /// Length field from mini-header (enc_meta || payload).
  length: uint64;
  /// Human name (e.g. relative path for FM wrap).
  name: string;
  /// Stable order among siblings of the same type.
  ordinal: uint32;
  compression: CompressionCodec;
  storage: StorageMode;
  /// Inline: empty. Linked: ordered FastCDC chunk hashes.
  chunks: [ChunkRef];
  encryption: EncryptionInfo; // optional
  /// MIME or model id / schema id depending on type.
  content_type: string;
  /// BLAKE3 of compressed plaintext before encryption (integrity).
  payload_blake3: [ubyte]; // 32
}

table Toc {
  /// Monotonic generation for this file_uuid lineage.
  generation: uint64;
  /// Parent generation (0 if genesis).
  parent_generation: uint64;
  /// Echo of header file UUID.
  file_uuid: [ubyte]; // 16
  /// sealed vs linked default for regions that omit storage.
  default_storage: StorageMode;
  regions: [RegionEntry];
  /// Optional zstd dictionary id (hash) used by text regions in this library.
  zstd_dict_blake3: [ubyte]; // 32, optional
}

root_type Toc;
```

**Notes**

- `generation` increments on each logical save that participates in the
  version chain. GC of obsolete generations is tied to
  `ChunkStore::garbage_collect` (§12).
- Linked regions store only `chunks` in the file; blob bytes live under the
  chunk store directory layout already used by Orchid
  (`<chunks_dir>/<aa>/<rest>.bin`).
- FlatBuffers is chosen so the TOC can grow without breaking old readers and
  so mmap can parse without a full deserialize into owned heap structures.

---

## 6. Sealed vs Linked Modes

### Sealed

- All region payloads are **inline** (`StorageMode::Inline`).
- File is self-contained: copy to a USB drive or attach to email and it opens
  without the local `ChunkStore`.
- Default for **export / share**.

### Linked

- Region payloads are **references** (`StorageMode::Linked`): ordered list of
  BLAKE3 chunk hashes produced by FastCDC (`Chunker` /
  `ChunkerConfig` defaults: min 512 KiB, avg 1 MiB, max 4 MiB).
- The `.orchid` file is a thin header + TOC + Version-history; bytes live in
  `ChunkStore`.
- Default for **library / managed folders** so two similar documents dedupe.

### Repack

| Direction | Operation |
| --- | --- |
| Sealed → Linked | Chunk each inline payload with `Chunker::chunk_bytes`, `ChunkStore::put` each slice, replace payload with `chunks`, set `storage = Linked`, shrink file. |
| Linked → Sealed | `ChunkStore::get` each hash in order, concatenate, write inline, `release` chunk refs when appropriate, set `storage = Inline`. |

Repack preserves `file_uuid` and increments `generation`. Capability flag
`CAP_LINKED` must be set if any region is linked.

### Defaults

| Context | Mode |
| --- | --- |
| Document editor “Export…” / share sheet | Sealed |
| Document editor “Save” into managed library | Linked |
| FM “Wrap as .orchid” into library | Linked |
| FM “Wrap as .orchid” to arbitrary path outside library | Sealed |

---

## 7. Encryption Model

### Current state (gap)

Today `orchid-crypto` age encryption is **whole-file** oriented:

- `Encryptor` / `Decryptor` / `Identity` in
  `crates/orchid-crypto/src/age_encryption/`
- Sidecar `EncryptedFileMeta` (`.age.meta` /
  `.orchid-encrypted.meta`) describes one plaintext blob

Phase 2 extends this to **per-region** recipients inside `.orchid` without
abandoning age.

### Per-region encryption

1. Compress plaintext if applicable (zstd).
2. Age-encrypt the compressed bytes to one or more recipients (passphrase
   and/or X25519), same algorithms as today’s `Encryptor`.
3. Store age header + ciphertext as the region payload (or as linked chunks
   of that ciphertext).
4. Record `EncryptionInfo` in the TOC: age header bytes (or length prefix
   convention matching age stream), `plaintext_blake3`, `identity_kind`,
   `is_public`.

Readers decrypt only regions they need; public regions skip the age step.

### Public vs private regions

| Class | Typical regions | Encrypted? |
| --- | --- | --- |
| **Public** | Provenance summary, short Clean-Text digest, Embedding model id + dimensions (not necessarily vectors), Version-history stubs | Optionally integrity-only; may be plaintext |
| **Private** | Full Clean-Text, Structured, Raw media, full Embedding vectors | Encrypted to recipient set |

Policy is chosen at write time. A sealed encrypted document may still expose
a public Provenance region so a validator can check credentials without the
passphrase.

### Identity reuse

`Identity` (passphrase / X25519) remains the user-facing key material.
`RevealManager` can wrap per-region decrypt sessions the same way it wraps
whole-file reveal today, with region-scoped TTLs.

---

## 8. CRDT Structured Region

Phase 1 may store Structured as an immutable snapshot. Phase 4 upgrades the
payload to:

```
┌─────────────────────────────┬────────────────────────────┐
│ Compaction snapshot         │ Append-only op log         │
│ (document state at gen G)   │ (RGA / Peritext-like ops)  │
└─────────────────────────────┴────────────────────────────┘
```

### Design points

- **Op log:** append-only; each op carries `actor_id`, `Lamport`/`HLC`
  timestamp, and causal parents. Text ops follow a Peritext-style model
  (positions as IDs, not string indices) so concurrent inserts commute.
- **Snapshot:** periodic compaction folds the prefix of the log into a
  baseline tree; the log retains only ops after the snapshot watermark.
- **Multi-writer:** human editor and AI agent are distinct `actor_id`s.
  Suggestions, comments, and accept/reject are first-class ops—replacing
  DOCX track-changes and comment XML parts.
- **Merge:** loading two forks with the same `file_uuid` unions op logs by
  op id, then materializes. DONE criterion for Phase 4 is a deterministic
  concurrent two-client merge test.
- **Wire encoding:** FlatBuffers or postcard/bincode for ops; exact schema
  lands with Phase 4. The region `content_type` string identifies the
  schema (e.g. `orchid.structured.crdt.v1`).

Until Phase 4, Structured `content_type` is
`orchid.structured.snapshot.v1` and the payload is a single snapshot blob.

---

## 9. Embeddings & AI-native Retrieval

### Hierarchical vectors

The Embedding region stores vectors at multiple granularities:

| Level | Unit | Use |
| --- | --- | --- |
| Document | whole Clean-Text | coarse routing |
| Section | heading-bounded spans | chapter retrieval |
| Paragraph | paragraph / block | precise RAG chunks |

Each record includes:

- level + span offsets into Clean-Text (UTF-8 byte offsets)
- vector (`f16` or `i8` quantized, per model)
- **token counts** per tokenizer id (e.g. cl100k-ish / bundled sentence
  tokenizer) so agents budget context without re-tokenizing

### Digest region for agent RAG

Writers may include a short public Clean-Text *digest* (or a dedicated
name=`digest` Clean-Text region) summarizing title, outline, and keywords
for agents that cannot decrypt the full private text.

### Bundled inference asset

Orchid ships a quantized sentence embedding model loaded via **ORT**
(ONNX Runtime), analogous to how `pdfium.dll` is a bundled native asset for
PDF (see [BUILDING.md](BUILDING.md)). Model id and dimension are recorded in
TOC `content_type` / Embedding metadata so indexes invalidate on model bump.

### Hybrid search

`orchid-search` continues to use Tantivy BM25 over Clean-Text. Phase 5 adds
an ANN index (candidates: `instant-distance` or `hnsw_rs`) over Embedding
vectors. Query path:

1. Embed the query with the same bundled model.
2. ANN top-k over `.orchid` embedding postings.
3. Fuse with Tantivy BM25 (reciprocal rank / weighted sum).
4. DONE: a semantic query retrieves a `.orchid` hit that BM25 alone misses.

---

## 10. Resilience & Recovery

Failure modes: truncated file, bitrot in TOC, partial write of footer.

### Recovery algorithm

1. If footer terminator ≠ `ORCD` or TOC BLAKE3 fails → recovery mode.
2. Linear scan from offset 4096, stepping 4 KiB:
   - If bytes equal `ORCR`, parse `type_id` + `length`.
   - Validate that `offset + 14 + length` stays within the file and that
     the next aligned boundary looks plausible (zero pad or another `ORCR`
     / TOC).
3. Rebuild a provisional TOC from discovered regions (generation unknown →
   `0`; mark `recovered = true` in memory only).
4. For each region with `payload_blake3` known from a salvageable TOC
   fragment, verify; otherwise recompute after decrypt/decompress when keys
   are available.
5. Offer “Save recovered copy” which writes a fresh sealed file with new
   footer and TOC.

Linked mode recovery also reconciles chunk hashes against `ChunkStore::get`
integrity checks (store already verifies BLAKE3 on read).

---

## 11. Compression

| Payload class | Codec |
| --- | --- |
| Clean-Text, Structured, Version-history, most Provenance | **zstd** (default level 3; writer-selectable) |
| Embedding vectors | typically none (already quantized); optional zstd if it wins |
| Raw media already compressed (JPEG, PNG, MP4, ZIP, PDF, …) | **none** — skip recompression |

TOC `CompressionCodec` records the choice. Decompress before age decrypt
verification of `plaintext_blake3` (plaintext = after decompress).

### Optional per-library zstd dictionary

Managed libraries may train a zstd dictionary over Clean-Text corpora. The
dictionary blob lives in `ChunkStore` (content-addressed); TOC field
`zstd_dict_blake3` references it. Sealed exports that rely on a dictionary
must either inline the dictionary as a Raw region named
`zstd-dictionary` or recompress without a dict before export.

---

## 12. Versioning & Compatibility

### Semver in header

- **major:** breaking layout or semantic change (readers must not guess).
- **minor:** additive (new region types, new capability flags); old readers
  skip unknown regions.

### Capability flags (`u64`)

| Bit | Name | Meaning |
| --- | --- | --- |
| 0 | `CAP_LINKED` | At least one linked region |
| 1 | `CAP_ENCRYPTED` | At least one private encrypted region |
| 2 | `CAP_CRDT` | Structured uses CRDT op log |
| 3 | `CAP_EMBEDDINGS` | Embedding region present |
| 4 | `CAP_C2PA` | Provenance carries C2PA |
| 5 | `CAP_ZSTD_DICT` | Text regions need library dictionary |
| 6–63 | reserved | write zero; ignore unknown bits on read if major matches |

Readers must refuse files with `version_major` greater than implemented.
Unknown capability bits with a known major are warnings, not hard failures,
unless a required region type cannot be interpreted.

### Generation chain & GC

- Each save that retains history bumps `Toc.generation` and may add a
  Version-history entry pointing at parent chunk sets.
- Dropping old generations calls `ChunkStore::release` on unreferenced
  hashes, then `ChunkStore::garbage_collect` to delete orphan `.bin` files
  (same orphan walk used after crashed writes today).
- Sealed files may omit Version-history; linked library files should retain
  a bounded chain (policy in managed-folder config).

---

## 13. Integration with Existing Orchid Crates

| Existing piece | Path | Role in `.orchid` |
| --- | --- | --- |
| `Chunker` / FastCDC | `crates/orchid-crypto/src/content/chunker.rs` | Split region payloads for linked mode; BLAKE3 per chunk |
| `ChunkStore` | `crates/orchid-crypto/src/content/store.rs` | CAS put/get/release; `garbage_collect` for generation GC |
| `hash_bytes` / BLAKE3 | `crates/orchid-crypto/src/content/hash` | TOC digests, footer `toc_blake3`, chunk ids |
| `Encryptor` / `Decryptor` / `Identity` | `crates/orchid-crypto/src/age_encryption/` | Whole-file today; extend to per-region (Phase 2 gap) |
| `EncryptedFileMeta` | `age_encryption/metadata.rs` | Pattern for plaintext size/hash; fold into `EncryptionInfo` |
| `memmap2` | already used (e.g. PDF extract in `orchid-search`) | Map sealed files; page-aligned regions |
| Tantivy BM25 | `crates/orchid-search` | Index Clean-Text |
| Document OOXML path | `crates/orchid-viewers/src/document/` | Import/export bridge until native save dominates |
| Managed folders | `orchid-fs` | Default linked saves + retention policy |

### Future crate

Suggested home for parsers, writers, CLI, and FlatBuffers generated code:

```text
crates/orchid-format/
```

Dependencies: `orchid-crypto` (chunk + age), `flatbuffers`, `zstd`,
`memmap2`, `blake3`, `uuid`. UI and search depend on `orchid-format`, not
the reverse.

---

## 14. Adoption Path

1. **DOCX remains import/export.** The viewers/document pipeline continues
   to read/write OOXML for interoperability. No user is forced off Word.
2. **`.orchid` becomes native save** for the Orchid document editor (and
   for FM wrap). Autosave and library storage prefer linked `.orchid`.
3. **File manager action: “Wrap as .orchid”** packs selection → Raw +
   derived Clean-Text (+ optional embeddings when Phase 5 lands).
4. **Search** indexes `.orchid` via Clean-Text immediately in Phase 1;
   semantic path in Phase 5.
5. **MIME / extension** registered in the Windows installer when the CLI
   and editor open path are stable.

Migration tip: keep the original DOCX as a Raw region named
`original.docx` on first import so users can re-export bit-identical
bytes if needed.

---

## 15. Phased Roadmap

Each phase lists scope and a measurable **DONE** criterion.

### Phase 1 — Framing

**Scope**

- Sealed files only
- Regions: Raw, Clean-Text, Structured **snapshot** (not CRDT)
- FlatBuffers TOC + header/footer as specified
- `memmap2` open path
- zstd for text/structured; skip compressed Raw

**DONE:** CLI can `create` and `read` a sealed `.orchid` containing exactly
three regions (Raw + Clean-Text + Structured snapshot) and round-trip
Clean-Text bytes bit- identically after decompress.

### Phase 2 — Encryption & versions

**Scope**

- Per-region age encryption (extend `Encryptor`/`Decryptor` beyond
  whole-file; this is the known gap)
- Linked mode via FastCDC + `ChunkStore`
- CAS generation chain + Version-history region
- Sealed↔linked repack

**DONE:** Two linked `.orchid` files that share substantial payload bytes
dedupe in `ChunkStore` (refcount ≥ 2 on shared chunk hashes); decrypt of a
private region succeeds only with the correct `Identity`.

### Phase 3 — C2PA provenance

**Scope**

- Provenance region carrying C2PA content credentials
- Sign on export; verify on open

**DONE:** A third-party C2PA validator accepts the Provenance payload
extracted from a sample `.orchid`.

### Phase 4 — CRDT structured

**Scope**

- RGA/Peritext-like append-only op log + compaction snapshot
- Multi-writer (human + AI agent actor ids)
- Replaces DOCX track-changes/comments for native docs

**DONE:** Concurrent two-client merge test is deterministic (same op sets →
identical materialized document hash).

### Phase 5 — AI embeddings & hybrid search

**Scope**

- Hierarchical Embedding region + token counts
- Bundled quantized sentence model via ORT (asset parallel to `pdfium.dll`)
- ANN (`instant-distance` or `hnsw_rs`) fused with Tantivy BM25 in
  `orchid-search`

**DONE:** A semantic query finds a relevant `.orchid` document that a pure
BM25 query misses on the same corpus.

---

## Appendix A — Implementation checklist (Phase 1)

- [ ] `crates/orchid-format` crate skeleton + `orchid_toc.fbs`
- [ ] Header / region / footer writers with 4 KiB padding
- [ ] Sealed create/read CLI (`orchid-format` or `orchid-app` subcommand)
- [ ] Round-trip tests with three regions
- [ ] MIME + extension constants (`ORCD`, `application/vnd.orchid`)
- [ ] Document this file’s status → “implemented” when Phase 1 DONE lands

## Appendix B — Constants summary

```text
Extension:          .orchid
MIME:               application/vnd.orchid
Header magic:       ORCD
Region magic:       ORCR
Footer terminator:  ORCD
Alignment:          4096 bytes
Hash:               BLAKE3-256
Chunking:           FastCDC (ChunkerConfig defaults)
Encryption:         age (per-region in Phase 2; whole-file only today)
TOC:                FlatBuffers (Toc)
```

---

*End of draft specification.*
