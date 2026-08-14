//! Hex / binary dumps and encoding labels for the text viewer.

/// How the text viewer presents the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDisplayMode {
    /// Decoded text (syntax highlighting when available).
    #[default]
    Text,
    /// Classic hex dump with ASCII gutter.
    Hex,
    /// Continuous hex bytes, no ASCII column.
    Binary,
}

impl TextDisplayMode {
    /// Stable UI discriminant (`0` text, `1` hex, `2` binary).
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Text => 0,
            Self::Hex => 1,
            Self::Binary => 2,
        }
    }

    /// Parse the UI discriminant.
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Hex,
            2 => Self::Binary,
            _ => Self::Text,
        }
    }
}

/// Encodings the viewer can switch to on the fly.
pub const VIEWER_ENCODINGS: &[&str] = &[
    "UTF-8",
    "UTF-16LE",
    "UTF-16BE",
    "windows-1252",
    "windows-1251",
    "ISO-8859-1",
    "ISO-8859-5",
    "Shift_JIS",
    "EUC-JP",
    "EUC-KR",
    "GBK",
    "GB18030",
    "Big5",
    "KOI8-R",
];

const HEX_BYTE_LIMIT: usize = 256 * 1024;

/// Format `bytes` as a hex dump (`offset  hex  |ascii|`).
#[must_use]
pub fn format_hex_dump(bytes: &[u8]) -> String {
    let slice = &bytes[..bytes.len().min(HEX_BYTE_LIMIT)];
    let mut out = String::with_capacity(slice.len() * 4 + 64);
    for (row, chunk) in slice.chunks(16).enumerate() {
        let offset = row * 16;
        out.push_str(&format!("{offset:08x}  "));
        for (i, b) in chunk.iter().enumerate() {
            out.push_str(&format!("{b:02x} "));
            if i == 7 {
                out.push(' ');
            }
        }
        for _ in chunk.len()..16 {
            out.push_str("   ");
        }
        if chunk.len() <= 8 {
            out.push(' ');
        }
        out.push_str(" |");
        for b in chunk {
            let c = if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            };
            out.push(c);
        }
        out.push_str("|\n");
    }
    if bytes.len() > HEX_BYTE_LIMIT {
        out.push_str(&format!(
            "\n… {} more bytes not shown\n",
            bytes.len() - HEX_BYTE_LIMIT
        ));
    }
    out
}

/// Format `bytes` as a continuous hex stream (16 bytes per line).
#[must_use]
pub fn format_binary_hex(bytes: &[u8]) -> String {
    let slice = &bytes[..bytes.len().min(HEX_BYTE_LIMIT)];
    let mut out = String::with_capacity(slice.len() * 3 + 32);
    for chunk in slice.chunks(16) {
        for (i, b) in chunk.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&format!("{b:02x}"));
        }
        out.push('\n');
    }
    if bytes.len() > HEX_BYTE_LIMIT {
        out.push_str(&format!(
            "\n… {} more bytes not shown\n",
            bytes.len() - HEX_BYTE_LIMIT
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_dump_contains_ascii() {
        let s = format_hex_dump(b"Hello");
        assert!(s.contains("48 65 6c 6c 6f"));
        assert!(s.contains("|Hello|"));
    }

    #[test]
    fn binary_is_hex_only() {
        let s = format_binary_hex(b"Hi");
        assert!(s.contains("48 69"));
        assert!(!s.contains('|'));
    }
}
