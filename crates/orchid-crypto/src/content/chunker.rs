//! Content-defined chunking over bytes and files using FastCDC.

use std::future::Future;
use std::path::Path;

use fastcdc::v2020::{FastCDC, StreamCDC};
use tokio::fs::File;

use crate::content::hash::hash_bytes;
use crate::error::Result;
use crate::secret::zeroizing::ZeroizingBytes;

/// Tunables for [`Chunker`]. `avg_size` is a target; actual chunks vary
/// between `min_size` and `max_size` depending on content.
#[derive(
    Debug,
    Clone,
    Copy,
    serde::Serialize,
    serde::Deserialize,
    bincode::Encode,
    bincode::Decode,
)]
pub struct ChunkerConfig {
    /// Lower bound on a single chunk's length in bytes.
    pub min_size: u32,
    /// Target average chunk length in bytes.
    pub avg_size: u32,
    /// Upper bound on a single chunk's length in bytes.
    pub max_size: u32,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            min_size: 512 * 1024,
            avg_size: 1024 * 1024,
            max_size: 4 * 1024 * 1024,
        }
    }
}

/// Metadata for a single content-defined chunk.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Byte offset of the chunk in the source stream.
    pub offset: u64,
    /// Length of the chunk in bytes.
    pub length: u32,
    /// BLAKE3 hash of the chunk contents.
    pub hash: [u8; 32],
}

/// Stateless chunker driven by [`ChunkerConfig`].
#[derive(Debug, Clone, Copy)]
pub struct Chunker {
    config: ChunkerConfig,
}

impl Chunker {
    /// Construct a chunker with the given configuration.
    #[must_use]
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Chunk in-memory bytes. Returns `(metadata, slice)` pairs for each
    /// chunk in order. The slices borrow from `data`.
    #[must_use]
    pub fn chunk_bytes<'a>(&self, data: &'a [u8]) -> Vec<(Chunk, &'a [u8])> {
        let cdc = FastCDC::new(
            data,
            self.config.min_size,
            self.config.avg_size,
            self.config.max_size,
        );
        cdc.into_iter()
            .map(|c| {
                let slice = &data[c.offset..c.offset + c.length];
                let hash = hash_bytes(slice);
                (
                    Chunk {
                        offset: c.offset as u64,
                        length: c.length as u32,
                        hash,
                    },
                    slice,
                )
            })
            .collect()
    }

    /// Chunk a file, streaming its contents. Each chunk is handed to `sink`
    /// wrapped in a [`ZeroizingBytes`] so that accidental leaks on the hot
    /// path are caught by the type system.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors and whatever the sink returns.
    pub async fn chunk_file<F, Fut>(
        &self,
        path: &Path,
        mut sink: F,
    ) -> Result<Vec<Chunk>>
    where
        F: FnMut(Chunk, ZeroizingBytes) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        // Hand the chunker a synchronous `Read`: we open the file via
        // `tokio::fs::File`, then convert to `std::fs::File` via
        // `into_std()`. StreamCDC operates synchronously, so we drive it on
        // the current task with small blocking reads — acceptable for
        // desktop-scale files.
        let file = File::open(path).await?.into_std().await;
        let cdc = StreamCDC::new(
            file,
            self.config.min_size,
            self.config.avg_size,
            self.config.max_size,
        );

        let mut out = Vec::new();
        for result in cdc {
            let chunk = result.map_err(|e| crate::error::CryptoError::Io(std::io::Error::other(e)))?;
            let data = ZeroizingBytes::new(chunk.data);
            let meta = Chunk {
                offset: chunk.offset,
                length: chunk.length as u32,
                hash: hash_bytes(data.as_slice()),
            };
            let meta_for_sink = meta.clone();
            sink(meta_for_sink, data).await?;
            out.push(meta);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valid FastCDC parameters small enough to produce multiple chunks
    /// from our synthetic 2 MiB test data. FastCDC requires
    /// `max_size >= MAXIMUM_MIN (256 KiB)`.
    fn test_cfg() -> ChunkerConfig {
        ChunkerConfig {
            min_size: 64 * 1024,
            avg_size: 128 * 1024,
            max_size: 512 * 1024,
        }
    }

    fn test_data() -> Vec<u8> {
        // 2 MiB of pseudo-random-looking but deterministic data.
        (0..(2 * 1024 * 1024 / 4) as u32)
            .flat_map(|i| i.wrapping_mul(2654435761).to_le_bytes())
            .collect()
    }

    #[test]
    fn chunks_cover_the_whole_input_contiguously() {
        let chunker = Chunker::new(test_cfg());
        let data = test_data();
        let chunks = chunker.chunk_bytes(&data);
        assert!(!chunks.is_empty());

        let mut expected_offset = 0u64;
        let mut total = 0u64;
        for (chunk, slice) in &chunks {
            assert_eq!(chunk.offset, expected_offset);
            assert_eq!(chunk.length as usize, slice.len());
            expected_offset += chunk.length as u64;
            total += chunk.length as u64;
        }
        assert_eq!(total, data.len() as u64);
    }

    #[test]
    fn identical_input_produces_identical_chunks() {
        let chunker = Chunker::new(test_cfg());
        let data = test_data();

        let a: Vec<_> = chunker.chunk_bytes(&data).into_iter().map(|(c, _)| c.hash).collect();
        let b: Vec<_> = chunker.chunk_bytes(&data).into_iter().map(|(c, _)| c.hash).collect();
        assert_eq!(a, b);
    }
}
