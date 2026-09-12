//! Parse / serialise `word/document.xml`.

use std::collections::HashMap;

mod helpers;
mod parse;
mod write;

/// Relationship map: `rId` → target path relative to `word/`.
pub type Relationships = HashMap<String, String>;

pub use helpers::{image_from_part, word_part_path};
pub use parse::{parse_document_xml, parse_relationships, parse_story_xml};
pub use write::{write_document_xml, write_story_xml};

#[cfg(test)]
mod tests;
