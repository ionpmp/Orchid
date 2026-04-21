# orchid-crypto

Cryptography primitives for Orchid. Bundles three subsystems:

- file and folder encryption built on the `age` crate,
- KDBX4 password-vault parsing and writing via `keepass`,
- content-addressed chunking using `blake3` hashing and `fastcdc` content-defined chunking.

All APIs are designed to be used from async contexts without leaking low-level crypto types across crate boundaries.
