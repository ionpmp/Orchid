//! Tantivy schema used by [`crate::SearchEngine`].

use tantivy::schema::{
    Field, NumericOptions, Schema as TantivySchema, SchemaBuilder, TextFieldIndexing, TextOptions,
    FAST, INDEXED, STORED, STRING, TEXT,
};

/// Convenience bundle combining the Tantivy schema with direct handles to
/// every field we care about.
#[derive(Debug, Clone)]
pub struct Schema {
    /// The backing Tantivy schema.
    pub tantivy: TantivySchema,
    /// Full canonical path (`FsPath::as_str`), exact-match.
    pub field_path: Field,
    /// File / directory name, tokenized + lowercased.
    pub field_name: Field,
    /// Lowercased file extension.
    pub field_extension: Field,
    /// Extracted text content, tokenized + stemmed.
    pub field_content: Field,
    /// Tag tokens, lowercased, exact-match.
    pub field_tags: Field,
    /// Colour label as a string.
    pub field_color_label: Field,
    /// File size in bytes (fast field).
    pub field_size: Field,
    /// Last-modified Unix timestamp in seconds (fast field).
    pub field_modified: Field,
    /// Sniffed MIME type.
    pub field_mime: Field,
    /// `"file"` or `"directory"`.
    pub field_kind: Field,
    /// If the entry lives inside an archive, the archive's outer path.
    pub field_in_archive: Field,
}

impl Default for Schema {
    fn default() -> Self {
        Self::new()
    }
}

impl Schema {
    /// Build a fresh schema.
    #[must_use]
    pub fn new() -> Self {
        let mut b = SchemaBuilder::new();

        // Path: stored, exact-match raw tokenizer so we can delete-by-path.
        let path_opts = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("raw")
                    .set_index_option(tantivy::schema::IndexRecordOption::Basic),
            )
            .set_stored();
        let field_path = b.add_text_field("path", path_opts);

        // Name: default tokenizer + lowercase.
        let field_name = b.add_text_field("name", TEXT | STORED);

        // Extension: exact match raw.
        let field_extension = b.add_text_field("extension", STRING | STORED);

        // Content: default + stemmed English for BM25 ranking.
        let content_opts = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("en_stem")
                    .set_index_option(tantivy::schema::IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();
        let field_content = b.add_text_field("content", content_opts);

        // Tags: raw, multiple values per doc.
        let field_tags = b.add_text_field("tags", STRING | STORED);
        let field_color_label = b.add_text_field("color_label", STRING | STORED);

        // Numerics: stored + fast for range queries / sort.
        let num_opts: NumericOptions = NumericOptions::default().set_stored().set_indexed() | FAST;
        let field_size = b.add_u64_field("size", num_opts.clone());
        let field_modified = b.add_i64_field("modified", num_opts);

        let field_mime = b.add_text_field("mime", STRING | STORED);
        let field_kind = b.add_text_field("kind", STRING | STORED);
        let field_in_archive = b.add_text_field("in_archive", STRING | STORED);

        let _ = INDEXED; // keep import warnings quiet if Tantivy changes its re-exports
        let tantivy = b.build();
        Self {
            tantivy,
            field_path,
            field_name,
            field_extension,
            field_content,
            field_tags,
            field_color_label,
            field_size,
            field_modified,
            field_mime,
            field_kind,
            field_in_archive,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_registers_every_field() {
        let s = Schema::new();
        let t = &s.tantivy;
        for name in [
            "path",
            "name",
            "extension",
            "content",
            "tags",
            "color_label",
            "size",
            "modified",
            "mime",
            "kind",
            "in_archive",
        ] {
            assert!(
                t.get_field(name).is_ok(),
                "schema missing field `{name}`"
            );
        }
    }
}
