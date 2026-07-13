//! Rope-backed text buffer with encoding detection.

use chardetng::EncodingDetector;
use encoding_rs::{Encoding, UTF_8};
use ropey::Rope;

use crate::error::{Result, ViewerError};

/// Line-ending kind detected on load.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    /// Short label used in snapshots (`"LF"` / `"CRLF"`).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
        }
    }

    /// Literal bytes.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}

/// Text buffer combining a `ropey::Rope` with detected encoding + line-ending.
pub struct TextBuffer {
    rope: Rope,
    line_ending: LineEnding,
    encoding: &'static Encoding,
    dirty: bool,
}

impl std::fmt::Debug for TextBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextBuffer")
            .field("lines", &self.rope.len_lines())
            .field("encoding", &self.encoding.name())
            .field("line_ending", &self.line_ending.label())
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl TextBuffer {
    /// Decode `bytes` using chardetng-guided encoding detection.
    ///
    /// BOMs are stripped. UTF-8 with BOM is treated as UTF-8.
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::TextDecode`] if the detected encoding reports
    /// hard errors on decode.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let (stripped, bom_encoding) = strip_bom(bytes);
        let encoding = bom_encoding.unwrap_or_else(|| detect_encoding(stripped));
        let (cow, _enc, had_errors) = encoding.decode(stripped);
        if had_errors {
            tracing::warn!(
                encoding = encoding.name(),
                "text buffer decoded with replacement characters"
            );
        }
        let mut s: String = cow.into_owned();
        // Normalise CRLF → LF in the rope; remember the original ending for save.
        let line_ending = detect_line_ending(&s);
        if line_ending == LineEnding::Crlf {
            s = s.replace("\r\n", "\n");
        }
        Ok(Self {
            rope: Rope::from_str(&s),
            line_ending,
            encoding,
            dirty: false,
        })
    }

    /// Re-encode the buffer back to bytes for writing.
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::TextDecode`] if encoding fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut text = self.rope.to_string();
        if self.line_ending == LineEnding::Crlf {
            text = text.replace('\n', "\r\n");
        }
        let (cow, _enc, had_errors) = self.encoding.encode(&text);
        if had_errors {
            return Err(ViewerError::TextDecode(format!(
                "cannot encode text as {}",
                self.encoding.name()
            )));
        }
        Ok(cow.into_owned())
    }

    /// Number of lines.
    #[must_use]
    pub fn line_count(&self) -> u32 {
        self.rope.len_lines() as u32
    }

    /// Total characters.
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.rope.len_chars()
    }

    /// Detected encoding.
    #[must_use]
    pub fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// Detected line-ending.
    #[must_use]
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Whether the buffer has pending edits.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the buffer clean (post-save).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Full buffer text (LF-normalised, without CRLF restoration).
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.rope.to_string()
    }

    /// Compare LF-normalised contents to `other` without allocating the rope.
    #[must_use]
    pub fn content_eq(&self, other: &str) -> bool {
        if self.rope.len_bytes() != other.len() {
            return false;
        }
        let mut offset = 0usize;
        for chunk in self.rope.chunks() {
            let end = offset + chunk.len();
            if other.as_bytes().get(offset..end) != Some(chunk.as_bytes()) {
                return false;
            }
            offset = end;
        }
        true
    }

    /// Byte span of a single contiguous difference vs LF-normalised `new_text`.
    ///
    /// Returns `(start_byte, old_end_byte, new_end_byte)` on the UTF-8 byte
    /// axis, or `None` when the texts are equal or the change is empty.
    #[must_use]
    pub fn single_span_diff(&self, new_text: &str) -> Option<(usize, usize, usize)> {
        let old_len = self.rope.len_bytes();
        let new_bytes = new_text.as_bytes();
        let new_len = new_bytes.len();

        let mut prefix = 0usize;
        'prefix: for chunk in self.rope.chunks() {
            for &b in chunk.as_bytes() {
                if prefix >= new_len || new_bytes[prefix] != b {
                    break 'prefix;
                }
                prefix += 1;
            }
        }
        while prefix > 0 && !new_text.is_char_boundary(prefix) {
            prefix -= 1;
        }
        // Align to a rope char boundary as well (same UTF-8 stream).
        prefix = self.align_byte_to_char_boundary(prefix);

        let mut old_suffix = 0usize;
        let mut new_suffix = 0usize;
        let old_remain = old_len.saturating_sub(prefix);
        let new_remain = new_len.saturating_sub(prefix);
        let max_suffix = old_remain.min(new_remain);

        // Walk suffix from the end without materialising the rope.
        while old_suffix < max_suffix {
            let old_idx = old_len - 1 - old_suffix;
            let new_idx = new_len - 1 - new_suffix;
            let old_byte = self.byte_at(old_idx)?;
            if old_byte != new_bytes[new_idx] {
                break;
            }
            old_suffix += 1;
            new_suffix += 1;
        }
        // Ensure suffix does not overlap prefix and lands on char boundaries.
        while old_suffix > 0 {
            let old_cut = old_len - old_suffix;
            let new_cut = new_len - new_suffix;
            if old_cut < prefix || new_cut < prefix {
                old_suffix -= 1;
                new_suffix -= 1;
                continue;
            }
            if !new_text.is_char_boundary(new_cut) || !self.is_char_boundary_byte(old_cut) {
                old_suffix -= 1;
                new_suffix -= 1;
                continue;
            }
            break;
        }

        let old_end = old_len - old_suffix;
        let new_end = new_len - new_suffix;
        if prefix == old_end && prefix == new_end {
            return None;
        }
        Some((prefix, old_end, new_end))
    }

    /// Line/column at a char index in the LF-normalised rope.
    #[must_use]
    pub fn line_col_at_char(&self, char_idx: usize) -> (u32, u32) {
        let len = self.rope.len_chars();
        let idx = char_idx.min(len);
        let line = self.rope.char_to_line(idx) as u32;
        let line_start = self.rope.line_to_char(line as usize);
        (line, (idx - line_start) as u32)
    }

    /// Convert a UTF-8 byte index to a char index.
    #[must_use]
    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        let capped = byte_idx.min(self.rope.len_bytes());
        self.rope
            .byte_to_char(self.align_byte_to_char_boundary(capped))
    }

    fn byte_at(&self, byte_idx: usize) -> Option<u8> {
        if byte_idx >= self.rope.len_bytes() {
            return None;
        }
        let mut offset = 0usize;
        for chunk in self.rope.chunks() {
            let end = offset + chunk.len();
            if byte_idx < end {
                return Some(chunk.as_bytes()[byte_idx - offset]);
            }
            offset = end;
        }
        None
    }

    fn is_char_boundary_byte(&self, byte_idx: usize) -> bool {
        if byte_idx == 0 || byte_idx == self.rope.len_bytes() {
            return true;
        }
        // UTF-8 continuation bytes have top bits 10xxxxxx.
        self.byte_at(byte_idx)
            .is_some_and(|b| (b & 0b1100_0000) != 0b1000_0000)
    }

    fn align_byte_to_char_boundary(&self, mut byte_idx: usize) -> usize {
        let len = self.rope.len_bytes();
        if byte_idx > len {
            byte_idx = len;
        }
        while byte_idx > 0 && !self.is_char_boundary_byte(byte_idx) {
            byte_idx -= 1;
        }
        byte_idx
    }

    /// Replace the entire buffer contents (LF-normalised). Marks dirty when changed.
    pub fn replace_content(&mut self, text: &str) {
        let normalized = text.replace("\r\n", "\n");
        if self.content_eq(&normalized) {
            return;
        }
        self.rope = Rope::from_str(&normalized);
        self.dirty = true;
    }

    /// Fetch a single line (without the trailing newline).
    #[must_use]
    pub fn line(&self, idx: u32) -> Option<String> {
        if (idx as usize) >= self.rope.len_lines() {
            return None;
        }
        let l = self.rope.line(idx as usize);
        let mut s = l.to_string();
        if s.ends_with('\n') {
            s.pop();
        }
        Some(s)
    }

    /// Contiguous slice of lines `[first, first + count)` (clamped).
    #[must_use]
    pub fn visible_slice(&self, first_line: u32, count: u32) -> Vec<String> {
        let total = self.rope.len_lines() as u32;
        let end = first_line.saturating_add(count).min(total);
        let mut out = Vec::with_capacity((end - first_line) as usize);
        for i in first_line..end {
            if let Some(line) = self.line(i) {
                out.push(line);
            }
        }
        out
    }

    /// Extract text in `[start, end)` (exclusive end).
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::EditOutOfBounds`] for invalid positions.
    pub fn text_range(
        &self,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Result<String> {
        let start = self
            .line_col_to_char(start_line, start_col)
            .ok_or(ViewerError::EditOutOfBounds)?;
        let end = self
            .line_col_to_char(end_line, end_col)
            .ok_or(ViewerError::EditOutOfBounds)?;
        if end < start {
            return Err(ViewerError::EditOutOfBounds);
        }
        Ok(self.rope.slice(start..end).to_string())
    }

    /// Insert `text` at `(line, column)` (zero-based).
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::EditOutOfBounds`] for invalid positions.
    pub fn insert(&mut self, line: u32, column: u32, text: &str) -> Result<()> {
        let char_idx = self
            .line_col_to_char(line, column)
            .ok_or(ViewerError::EditOutOfBounds)?;
        self.rope.insert(char_idx, text);
        self.dirty = true;
        Ok(())
    }

    /// Delete characters in `[start, end)` (exclusive at end).
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::EditOutOfBounds`] for invalid positions.
    pub fn delete(
        &mut self,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Result<()> {
        let start = self
            .line_col_to_char(start_line, start_col)
            .ok_or(ViewerError::EditOutOfBounds)?;
        let end = self
            .line_col_to_char(end_line, end_col)
            .ok_or(ViewerError::EditOutOfBounds)?;
        if end < start {
            return Err(ViewerError::EditOutOfBounds);
        }
        self.rope.remove(start..end);
        self.dirty = true;
        Ok(())
    }

    /// Byte offset of `(line, column)` in the LF-normalised rope (UTF-8).
    ///
    /// Tree-sitter edits use byte offsets; `column` is a character index.
    #[must_use]
    pub fn byte_index(&self, line: u32, column: u32) -> Option<usize> {
        let char_idx = self.line_col_to_char(line, column)?;
        Some(self.rope.char_to_byte(char_idx))
    }

    /// Tree-sitter [`tree_sitter::Point`] for `(line, column)`.
    ///
    /// The point's `column` is a **byte** offset within the row.
    #[must_use]
    pub fn tree_sitter_point(&self, line: u32, column: u32) -> Option<tree_sitter::Point> {
        let line_text = self.line(line)?;
        if column as usize > line_text.chars().count() {
            return None;
        }
        let byte_col: usize = line_text
            .chars()
            .take(column as usize)
            .map(char::len_utf8)
            .sum();
        Some(tree_sitter::Point {
            row: line as usize,
            column: byte_col,
        })
    }

    fn line_col_to_char(&self, line: u32, column: u32) -> Option<usize> {
        let line_count = self.rope.len_lines();
        if line as usize > line_count {
            return None;
        }
        let line_start = self.rope.line_to_char(line as usize);
        let line_len = if (line as usize) < line_count {
            let l = self.rope.line(line as usize);
            let s = l.to_string();
            let trimmed = if let Some(stripped) = s.strip_suffix('\n') {
                stripped.chars().count()
            } else {
                s.chars().count()
            };
            trimmed as u32
        } else {
            0
        };
        if column > line_len {
            return None;
        }
        Some(line_start + column as usize)
    }
}

fn strip_bom(bytes: &[u8]) -> (&[u8], Option<&'static Encoding>) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (&bytes[3..], Some(UTF_8));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return (&bytes[2..], Some(encoding_rs::UTF_16LE));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return (&bytes[2..], Some(encoding_rs::UTF_16BE));
    }
    (bytes, None)
}

fn detect_encoding(sample: &[u8]) -> &'static Encoding {
    let mut det = EncodingDetector::new();
    let head_len = sample.len().min(4 * 1024);
    det.feed(&sample[..head_len], true);
    det.guess(None, true)
}

fn detect_line_ending(sample: &str) -> LineEnding {
    let mut crlf = 0_usize;
    let mut lf = 0_usize;
    for (idx, ch) in sample.char_indices().take(4096) {
        if ch == '\n' {
            if idx > 0 && sample.as_bytes().get(idx - 1) == Some(&b'\r') {
                crlf += 1;
            } else {
                lf += 1;
            }
        }
    }
    if crlf > lf {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_without_bom_decodes() {
        let b = "hello\nворлд\n".as_bytes();
        let buf = TextBuffer::from_bytes(b).unwrap();
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line(0).as_deref(), Some("hello"));
        assert_eq!(buf.line(1).as_deref(), Some("ворлд"));
    }

    #[test]
    fn utf8_with_bom_strips_bom() {
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice("привет\n".as_bytes());
        let buf = TextBuffer::from_bytes(&b).unwrap();
        assert_eq!(buf.line(0).as_deref(), Some("привет"));
    }

    #[test]
    fn windows_1251_decodes() {
        // "привет" in Windows-1251.
        let b: &[u8] = &[0xEF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
        let buf = TextBuffer::from_bytes(b).unwrap();
        assert_eq!(buf.line(0).as_deref(), Some("привет"));
    }

    #[test]
    fn crlf_detection_preserved() {
        let b = "one\r\ntwo\r\n".as_bytes();
        let buf = TextBuffer::from_bytes(b).unwrap();
        assert_eq!(buf.line_ending(), LineEnding::Crlf);
        // Rope content is normalised to LF.
        assert_eq!(buf.line(0).as_deref(), Some("one"));
        // to_bytes puts CRLF back.
        let out = buf.to_bytes().unwrap();
        assert!(out.windows(2).any(|w| w == b"\r\n"));
    }

    #[test]
    fn content_eq_and_single_span_diff() {
        let buf = TextBuffer::from_bytes(b"hello world").unwrap();
        assert!(buf.content_eq("hello world"));
        assert!(!buf.content_eq("hello"));

        let (start, old_end, new_end) = buf.single_span_diff("hello Xorld").unwrap();
        assert_eq!(&"hello world".as_bytes()[start..old_end], b"w");
        assert_eq!(&"hello Xorld".as_bytes()[start..new_end], b"X");
    }

    #[test]
    fn insert_delete_basic() {
        let mut buf = TextBuffer::from_bytes(b"abc").unwrap();
        buf.insert(0, 1, "X").unwrap();
        assert_eq!(buf.line(0).as_deref(), Some("aXbc"));
        buf.delete(0, 1, 0, 2).unwrap();
        assert_eq!(buf.line(0).as_deref(), Some("abc"));
        assert!(buf.is_dirty());
    }

    #[test]
    fn visible_slice_clamps() {
        let buf = TextBuffer::from_bytes(b"a\nb\nc\n").unwrap();
        // Rope counts the trailing newline as an empty 4th line; we should
        // get back at most 4 lines regardless of the requested count.
        let lines = buf.visible_slice(0, 10);
        assert!(lines.len() >= 3 && lines.len() <= 4);
        assert_eq!(lines[0], "a");
        assert_eq!(lines[2], "c");
    }
}
