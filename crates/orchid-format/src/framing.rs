//! Fixed-size header, region mini-header, footer, and 4 KiB padding.

use crate::{
    FormatError, Result, ALIGNMENT, FILE_MAGIC, FOOTER_SIZE, FORMAT_VERSION_MAJOR,
    FORMAT_VERSION_MINOR, HEADER_SIZE, REGION_MAGIC, REGION_MINI_HEADER_SIZE,
};

/// Parsed file header (logical fields; on-disk size is [`HEADER_SIZE`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Format major version.
    pub version_major: u16,
    /// Format minor version.
    pub version_minor: u16,
    /// Capability bitfield ([`crate::capability`]).
    pub capability_flags: u64,
    /// Document identity (RFC 4122 bytes).
    pub file_uuid: [u8; 16],
    /// UTC milliseconds since Unix epoch.
    pub created_unix_ms: u64,
    /// Reserved; writers must store `0`.
    pub header_flags: u64,
}

impl Header {
    /// Build a Phase 1 header for a new sealed document.
    #[must_use]
    pub fn new_sealed(file_uuid: [u8; 16], created_unix_ms: u64) -> Self {
        Self {
            version_major: FORMAT_VERSION_MAJOR,
            version_minor: FORMAT_VERSION_MINOR,
            capability_flags: 0,
            file_uuid,
            created_unix_ms,
            header_flags: 0,
        }
    }

    /// Serialize to a 4096-byte aligned block.
    #[must_use]
    pub fn encode(&self) -> [u8; HEADER_SIZE as usize] {
        let mut buf = [0u8; HEADER_SIZE as usize];
        buf[0..4].copy_from_slice(FILE_MAGIC);
        buf[4..6].copy_from_slice(&self.version_major.to_le_bytes());
        buf[6..8].copy_from_slice(&self.version_minor.to_le_bytes());
        buf[8..16].copy_from_slice(&self.capability_flags.to_le_bytes());
        buf[16..32].copy_from_slice(&self.file_uuid);
        buf[32..40].copy_from_slice(&self.created_unix_ms.to_le_bytes());
        buf[40..48].copy_from_slice(&self.header_flags.to_le_bytes());
        buf
    }

    /// Parse a header block starting at `data[0..]`.
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < 48 {
            return Err(FormatError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "header truncated",
            )));
        }
        if &data[0..4] != FILE_MAGIC.as_slice() {
            return Err(FormatError::InvalidMagic);
        }
        Ok(Self {
            version_major: u16::from_le_bytes(data[4..6].try_into().unwrap()),
            version_minor: u16::from_le_bytes(data[6..8].try_into().unwrap()),
            capability_flags: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            file_uuid: data[16..32].try_into().unwrap(),
            created_unix_ms: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            header_flags: u64::from_le_bytes(data[40..48].try_into().unwrap()),
        })
    }
}

/// Region mini-header (`ORCR` + type + length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionHeader {
    /// Taxonomy id (§4).
    pub type_id: u16,
    /// Byte length of `enc_meta || payload` that follows.
    pub length: u64,
}

impl RegionHeader {
    /// Encode the 14-byte mini-header.
    #[must_use]
    pub fn encode(self) -> [u8; REGION_MINI_HEADER_SIZE] {
        let mut buf = [0u8; REGION_MINI_HEADER_SIZE];
        buf[0..4].copy_from_slice(REGION_MAGIC);
        buf[4..6].copy_from_slice(&self.type_id.to_le_bytes());
        buf[6..14].copy_from_slice(&self.length.to_le_bytes());
        buf
    }

    /// Parse a mini-header at `data[0..]`.
    pub fn decode(data: &[u8], absolute_offset: u64) -> Result<Self> {
        if data.len() < REGION_MINI_HEADER_SIZE {
            return Err(FormatError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "region header truncated",
            )));
        }
        if &data[0..4] != REGION_MAGIC.as_slice() {
            return Err(FormatError::InvalidRegionMagic(absolute_offset));
        }
        Ok(Self {
            type_id: u16::from_le_bytes(data[4..6].try_into().unwrap()),
            length: u64::from_le_bytes(data[6..14].try_into().unwrap()),
        })
    }
}

/// Fixed trailer at EOF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footer {
    /// Distance from EOF to the start of the TOC blob.
    pub toc_offset_from_end: u64,
    /// BLAKE3-256 of the TOC bytes.
    pub toc_blake3: [u8; 32],
}

impl Footer {
    /// Serialize the 44-byte footer.
    #[must_use]
    pub fn encode(self) -> [u8; FOOTER_SIZE] {
        let mut buf = [0u8; FOOTER_SIZE];
        buf[0..8].copy_from_slice(&self.toc_offset_from_end.to_le_bytes());
        buf[8..40].copy_from_slice(&self.toc_blake3);
        buf[40..44].copy_from_slice(FILE_MAGIC);
        buf
    }

    /// Parse the last [`FOOTER_SIZE`] bytes of a file.
    pub fn decode(tail: &[u8]) -> Result<Self> {
        if tail.len() < FOOTER_SIZE {
            return Err(FormatError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "footer truncated",
            )));
        }
        let start = tail.len() - FOOTER_SIZE;
        let data = &tail[start..];
        if &data[40..44] != FILE_MAGIC.as_slice() {
            return Err(FormatError::InvalidMagic);
        }
        Ok(Self {
            toc_offset_from_end: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            toc_blake3: data[8..40].try_into().unwrap(),
        })
    }
}

/// Round `offset` up to the next multiple of [`ALIGNMENT`].
#[must_use]
pub fn align_up(offset: u64) -> u64 {
    offset.div_ceil(ALIGNMENT) * ALIGNMENT
}

/// Number of zero pad bytes needed so `offset + pad` is aligned.
#[must_use]
pub fn pad_len(offset: u64) -> u64 {
    align_up(offset) - offset
}

/// Append zero bytes so `buf.len()` lands on a 4 KiB boundary.
pub fn pad_to_alignment(buf: &mut Vec<u8>) {
    let pad = pad_len(buf.len() as u64) as usize;
    if pad > 0 {
        buf.resize(buf.len() + pad, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let h = Header::new_sealed([7u8; 16], 1_700_000_000_000);
        let enc = h.encode();
        assert_eq!(enc.len(), 4096);
        assert_eq!(&enc[0..4], b"ORCD");
        assert!(enc[48..].iter().all(|&b| b == 0));
        assert_eq!(Header::decode(&enc).unwrap(), h);
    }

    #[test]
    fn region_header_roundtrip() {
        let r = RegionHeader {
            type_id: crate::region_type::CLEAN_TEXT,
            length: 42,
        };
        let enc = r.encode();
        assert_eq!(enc.len(), 14);
        assert_eq!(RegionHeader::decode(&enc, 4096).unwrap(), r);
    }

    #[test]
    fn footer_roundtrip() {
        let f = Footer {
            toc_offset_from_end: 8192,
            toc_blake3: [9u8; 32],
        };
        let enc = f.encode();
        assert_eq!(enc.len(), 44);
        assert_eq!(Footer::decode(&enc).unwrap(), f);
    }

    #[test]
    fn align_and_pad() {
        assert_eq!(align_up(0), 0);
        assert_eq!(align_up(1), 4096);
        assert_eq!(align_up(4096), 4096);
        assert_eq!(align_up(4097), 8192);
        let mut buf = vec![1u8; 100];
        pad_to_alignment(&mut buf);
        assert_eq!(buf.len(), 4096);
        assert!(buf[100..].iter().all(|&b| b == 0));
    }
}
