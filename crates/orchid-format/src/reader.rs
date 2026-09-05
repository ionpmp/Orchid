//! mmap-backed sealed `.orchid` reader (Phase 1).

#![allow(unsafe_code)]

use std::fs::File;
use std::path::Path;

use memmap2::Mmap;
use orchid_crypto::content::hash_bytes;

use crate::compress::decompress_zstd;
use crate::framing::{Footer, Header, RegionHeader};
use crate::toc::{root_as_toc, CompressionCodec, RegionEntry, RegionType, Toc};
use crate::{FormatError, Result, FOOTER_SIZE, HEADER_SIZE, REGION_MINI_HEADER_SIZE};

/// Opened sealed `.orchid` file backed by an immutable mmap.
pub struct SealedFile {
    mmap: Mmap,
    header: Header,
    toc_range: std::ops::Range<usize>,
}

impl SealedFile {
    /// Memory-map `path` and verify footer + TOC BLAKE3.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // SAFETY: mapping is read-only for the lifetime of `Self`; writers must
        // not mutate the file concurrently.
        let mmap = unsafe { Mmap::map(&file)? };
        Self::from_mmap(mmap)
    }

    fn from_mmap(mmap: Mmap) -> Result<Self> {
        if mmap.len() < HEADER_SIZE as usize + FOOTER_SIZE {
            return Err(FormatError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "file too small for header+footer",
            )));
        }
        let header = Header::decode(&mmap)?;
        let footer = Footer::decode(&mmap)?;
        if footer.toc_offset_from_end as usize > mmap.len() {
            return Err(FormatError::InvalidToc(
                "toc_offset_from_end exceeds file size".into(),
            ));
        }
        let toc_start = mmap.len() - footer.toc_offset_from_end as usize;
        let toc_end = mmap.len() - FOOTER_SIZE;
        if toc_start >= toc_end {
            return Err(FormatError::InvalidToc("empty TOC range".into()));
        }
        if !toc_start.is_multiple_of(crate::ALIGNMENT as usize) {
            return Err(FormatError::InvalidToc("TOC not 4 KiB aligned".into()));
        }
        let toc_bytes = &mmap[toc_start..toc_end];
        if hash_bytes(toc_bytes) != footer.toc_blake3 {
            return Err(FormatError::TocHashMismatch);
        }
        root_as_toc(toc_bytes).map_err(|e| FormatError::InvalidToc(e.to_string()))?;
        Ok(Self {
            mmap,
            header,
            toc_range: toc_start..toc_end,
        })
    }

    /// File header fields.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Parsed FlatBuffers TOC (zero-copy over the mmap).
    pub fn toc(&self) -> Result<Toc<'_>> {
        root_as_toc(&self.mmap[self.toc_range.clone()])
            .map_err(|e| FormatError::InvalidToc(e.to_string()))
    }

    /// Find the first TOC region of `type_`.
    pub fn find_region(&self, type_: RegionType) -> Result<RegionEntry<'_>> {
        let toc = self.toc()?;
        let regions = toc
            .regions()
            .ok_or_else(|| FormatError::InvalidToc("missing regions".into()))?;
        for i in 0..regions.len() {
            let r = regions.get(i);
            if r.type_() == type_ {
                return Ok(r);
            }
        }
        Err(FormatError::RegionNotFound(type_.0))
    }

    /// Read raw stored payload bytes for a TOC region (still compressed if zstd).
    pub fn region_payload(&self, entry: &RegionEntry<'_>) -> Result<&[u8]> {
        let offset = entry.offset() as usize;
        let length = entry.length() as usize;
        let end = offset
            .checked_add(REGION_MINI_HEADER_SIZE)
            .and_then(|p| p.checked_add(length))
            .ok_or_else(|| FormatError::RegionDecode("region extent overflow".into()))?;
        if end > self.mmap.len() {
            return Err(FormatError::RegionDecode("region extends past EOF".into()));
        }
        let _ = RegionHeader::decode(&self.mmap[offset..], entry.offset())?;
        let payload = &self.mmap[offset + REGION_MINI_HEADER_SIZE..end];
        if let Some(expected) = entry.payload_blake3() {
            if expected.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(expected.bytes());
                if hash_bytes(payload) != hash {
                    return Err(FormatError::RegionDecode(
                        "payload BLAKE3 mismatch".into(),
                    ));
                }
            }
        }
        Ok(payload)
    }

    /// Decode region payload applying TOC compression.
    pub fn region_plaintext(&self, entry: &RegionEntry<'_>) -> Result<Vec<u8>> {
        let payload = self.region_payload(entry)?;
        match entry.compression() {
            CompressionCodec::None => Ok(payload.to_vec()),
            CompressionCodec::Zstd => decompress_zstd(payload),
            other => Err(FormatError::UnsupportedCompression(other.0)),
        }
    }

    /// Convenience: Clean-Text plaintext after decompress.
    pub fn clean_text(&self) -> Result<Vec<u8>> {
        let entry = self.find_region(RegionType::CleanText)?;
        self.region_plaintext(&entry)
    }

    /// Convenience: Structured plaintext after decompress.
    pub fn structured(&self) -> Result<Vec<u8>> {
        let entry = self.find_region(RegionType::Structured)?;
        self.region_plaintext(&entry)
    }

    /// Convenience: Raw plaintext (uncompressed in Phase 1).
    pub fn raw(&self) -> Result<Vec<u8>> {
        let entry = self.find_region(RegionType::Raw)?;
        self.region_plaintext(&entry)
    }
}
