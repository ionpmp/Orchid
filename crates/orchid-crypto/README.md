# orchid-crypto

Cryptography layer for Orchid. Three independent subsystems sharing common secret-handling primitives:

- **File encryption** (`age_encryption`) — `age`-based symmetric (passphrase) and asymmetric (X25519) encryption of files, in-memory buffers, async streams, and whole directories (tar-in-memory). Includes a `RevealManager` that decrypts into a per-session temp directory, wipes the revealed plaintext after a configurable window, and publishes bus events for UI coordination.
- **Password database** (`kdbx`) — KDBX4 read/write via the `keepass` crate, with Orchid-facing `PasswordEntry`, `PasswordGroup`, `SearchQuery`/`SearchResult` types, TOTP helpers (`parse_otpauth_uri`, `generate_code`), and a `SecureClipboard` trait that `orchid-ui` implements.
- **Content-addressed storage** (`content`) — BLAKE3 hashing (streaming + mmap), FastCDC chunking, and a refcount-aware `ChunkStore` backed by a local `crypto_chunk_refs` table on the existing `orchid-storage` redb database. `Deduplicator` turns files into `FileManifest`s that share chunks across inputs.

## Threat model

- `age`-encrypted blobs protect confidentiality at rest: a stolen `.age` file is inert without the passphrase or X25519 identity. Tamper detection is end-to-end via age's HMAC and an additional BLAKE3 plaintext hash in the `.age.meta` sidecar.
- KDBX4 vaults use Argon2id (KeePassXC "Interactive" defaults); the vault is only in cleartext inside the running Orchid process.
- Reveal sessions narrow the window in which plaintext is on disk but do NOT defend against a concurrent attacker running as the same user.
- Content-addressed chunks are plaintext by design. Encrypted files that need dedup should be encrypted *after* chunking; this is the responsibility of `orchid-fs` later.
- Windows DPAPI helpers (`secret::dpapi`) protect short blobs against an offline attacker without access to the user's Windows profile; they do NOT protect against malware running as the same user.

## Scope

This crate is a library. It does not wire any UI or filesystem operations; it provides primitives for `orchid-fs`, `orchid-widgets`, and `orchid-ui` to compose. `unsafe_code` is forbidden at the crate root except for the DPAPI Win32 bindings module, which is isolated and extensively documented.
