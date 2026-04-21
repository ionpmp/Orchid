//! Error type for [`orchid_crypto`](crate).

/// Unified error type for every operation exposed by `orchid-crypto`.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CryptoError {
    // --- age -----------------------------------------------------------
    /// A passphrase did not decrypt the payload.
    #[error("invalid passphrase")]
    InvalidPassphrase,

    /// Generic failure from the `age` encryption pipeline.
    #[error("age encryption failed: {0}")]
    AgeEncrypt(String),

    /// Generic failure from the `age` decryption pipeline, including plaintext
    /// integrity-check failures after a successful cryptographic decrypt.
    #[error("age decryption failed: {0}")]
    AgeDecrypt(String),

    /// The age header on an encrypted file is not parseable.
    #[error("encrypted file header is malformed")]
    MalformedHeader,

    /// The sidecar metadata file for an encrypted payload could not be read
    /// or parsed.
    #[error("encrypted file metadata missing or unreadable: {0}")]
    MetadataUnreadable(String),

    // --- KDBX ----------------------------------------------------------
    /// Failed to open the KDBX password database.
    #[error("failed to open password database: {0}")]
    KdbxOpen(String),

    /// The master password does not unlock the KDBX database.
    #[error("invalid master password")]
    InvalidMasterPassword,

    /// No entry with the given id exists.
    #[error("entry not found: {0}")]
    EntryNotFound(uuid::Uuid),

    /// No group with the given id exists.
    #[error("group not found: {0}")]
    GroupNotFound(uuid::Uuid),

    /// An entry with the same title already exists in the target group.
    #[error("duplicate entry title in same group: {0}")]
    DuplicateEntryTitle(String),

    /// Failed to set up or parse a TOTP configuration.
    #[error("TOTP setup failed: {0}")]
    TotpSetup(String),

    /// TOTP code generation failed.
    #[error("TOTP generation failed: {0}")]
    TotpGeneration(String),

    // --- Content addressing -------------------------------------------
    /// A chunk hash was not found in the chunk store.
    #[error("chunk not found: {0}")]
    ChunkNotFound(String),

    /// A chunk's on-disk contents no longer match its registered hash.
    #[error("chunk integrity check failed (expected {expected}, got {actual})")]
    ChunkIntegrity {
        /// Hex-encoded hash the caller asked for.
        expected: String,
        /// Hex-encoded hash computed from the on-disk bytes.
        actual: String,
    },

    /// Releasing a chunk whose refcount is already zero.
    #[error("refcount underflow on chunk {0}")]
    RefcountUnderflow(String),

    // --- Reveal --------------------------------------------------------
    /// A reveal session's lifetime has run out.
    #[error("reveal session expired")]
    RevealExpired,

    /// The [`crate::RevealManager`] has no live session with this id.
    #[error("reveal session not found: {0}")]
    RevealNotFound(uuid::Uuid),

    // --- Platform ------------------------------------------------------
    /// Windows DPAPI returned an error.
    #[error("DPAPI operation failed: {0}")]
    Dpapi(String),

    /// DPAPI was invoked on a non-Windows build.
    #[error("DPAPI is unavailable on this platform")]
    DpapiUnavailable,

    // --- Generic -------------------------------------------------------
    /// A filesystem I/O operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// An operation on [`orchid_storage`] bubbled up.
    #[error(transparent)]
    Storage(#[from] orchid_storage::StorageError),

    /// An operation on [`orchid_core`] bubbled up.
    #[error(transparent)]
    Core(#[from] orchid_core::CoreError),

    /// A bincode / hex / base32 / ... encoding operation failed.
    #[error("encoding error: {0}")]
    Encoding(String),
}

/// Crate-wide `Result` alias defaulting to [`CryptoError`].
pub type Result<T, E = CryptoError> = std::result::Result<T, E>;
