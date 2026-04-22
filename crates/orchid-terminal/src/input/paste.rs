//! Bracketed-paste handling + safety filter.

use crate::error::{Result, TerminalError};

/// Open marker emitted before pasted text when bracketed-paste is on.
pub const BP_START: &[u8] = b"\x1b[200~";
/// Close marker emitted after pasted text.
pub const BP_END: &[u8] = b"\x1b[201~";

/// Reject payloads that try to smuggle a bracketed-paste end marker (paste
/// injection) or embed stray control characters other than CR / LF / TAB.
///
/// Normalises line endings: CRLF → LF, bare CR → LF.
pub(crate) fn sanitise_paste(text: &str) -> Result<String> {
    if text.contains("\x1b[201~") {
        return Err(TerminalError::PasteRejected);
    }
    let normalised: String = text
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    for c in normalised.chars() {
        if c == '\n' || c == '\t' {
            continue;
        }
        if c.is_control() {
            return Err(TerminalError::PasteRejected);
        }
    }
    Ok(normalised)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_is_normalised() {
        assert_eq!(sanitise_paste("a\r\nb").unwrap(), "a\nb");
        assert_eq!(sanitise_paste("a\rb").unwrap(), "a\nb");
    }

    #[test]
    fn bracketed_end_is_rejected() {
        assert!(sanitise_paste("hi\x1b[201~evil").is_err());
    }

    #[test]
    fn stray_control_char_is_rejected() {
        assert!(sanitise_paste("a\x01b").is_err());
    }

    #[test]
    fn tab_and_newline_allowed() {
        assert!(sanitise_paste("col1\tcol2\nrow2").is_ok());
    }
}
