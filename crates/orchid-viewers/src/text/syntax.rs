//! Syntax highlighter backed by tree-sitter grammars.

use tree_sitter::{Node, Parser, Tree, TreeCursor};

use crate::snapshot::{SyntaxLine, SyntaxScope, SyntaxSegment};
use crate::text::grammars::{self, language_for_id};

/// Pluggable highlighter with a tree-sitter grammar registry.
#[derive(Default)]
pub struct SyntaxHighlighter;

impl std::fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxHighlighter").finish_non_exhaustive()
    }
}

impl SyntaxHighlighter {
    /// Build a highlighter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Highlight `count` lines of `source` starting at `first_line`.
    ///
    /// When no grammar is registered for `language`, each line is returned
    /// as a single [`SyntaxScope::Plain`] segment.
    #[must_use]
    pub fn highlight_lines(
        &self,
        language: &str,
        source: &str,
        first_line: u32,
        line_count: u32,
    ) -> Vec<SyntaxLine> {
        let Some(ts_lang) = language_for_id(language) else {
            return plain_lines(source, first_line, line_count);
        };

        let mut parser = Parser::new();
        if parser.set_language(&ts_lang).is_err() {
            return plain_lines(source, first_line, line_count);
        }

        let Some(tree) = parser.parse(source, None) else {
            return plain_lines(source, first_line, line_count);
        };

        let scopes = byte_scopes(source, &tree, language);
        scoped_lines(source, &scopes, first_line, line_count)
    }

    /// Languages the registry currently supports.
    #[must_use]
    pub fn available_languages(&self) -> Vec<&'static str> {
        grammars::HIGHLIGHT_LANGUAGES.to_vec()
    }
}

fn plain_lines(source: &str, first_line: u32, line_count: u32) -> Vec<SyntaxLine> {
    let mut out = Vec::with_capacity(line_count as usize);
    for (offset, line) in source.split('\n').take(line_count as usize).enumerate() {
        out.push(SyntaxLine {
            line_number: first_line + offset as u32,
            segments: vec![SyntaxSegment {
                text: line.to_string(),
                scope: SyntaxScope::Plain,
            }],
        });
    }
    out
}

fn byte_scopes(source: &str, tree: &Tree, language: &str) -> Vec<SyntaxScope> {
    let mut scopes = vec![SyntaxScope::Plain; source.len()];
    let mut cursor = tree.walk();
    apply_node_scopes(&mut cursor, language, &mut scopes);
    scopes
}

fn apply_node_scopes(cursor: &mut TreeCursor, language: &str, scopes: &mut [SyntaxScope]) {
    let node = cursor.node();
    if let Some(scope) = scope_for_node(language, &node) {
        paint_scope(scopes, node.start_byte(), node.end_byte(), scope);
    }

    if cursor.goto_first_child() {
        loop {
            apply_node_scopes(cursor, language, scopes);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn paint_scope(scopes: &mut [SyntaxScope], start: usize, end: usize, scope: SyntaxScope) {
    let end = end.min(scopes.len());
    for byte in start..end {
        scopes[byte] = scope;
    }
}

fn scope_for_node(language: &str, node: &Node) -> Option<SyntaxScope> {
    let kind = node.kind();
    match kind {
        "comment" | "line_comment" | "block_comment" => Some(SyntaxScope::Comment),
        "string"
        | "string_literal"
        | "char_literal"
        | "raw_string_literal"
        | "string_content"
        | "interpreted_string_literal"
        | "string_fragment"
        | "template_string"
        | "template_chars"
        | "double_quote_scalar"
        | "single_quote_scalar"
        | "block_scalar" => Some(SyntaxScope::String),
        "integer"
        | "integer_literal"
        | "float"
        | "float_literal"
        | "number"
        | "number_literal" => Some(SyntaxScope::Number),
        "boolean" | "true" | "false" | "null" | "undefined" => Some(SyntaxScope::Constant),
        "attribute_item" | "attribute" | "annotation" => Some(SyntaxScope::Attribute),
        "preproc" | "preproc_def" | "preproc_call" | "preproc_if" | "preproc_elif" => {
            Some(SyntaxScope::Preprocessor)
        }
        "html_tag_name" | "tag_name" | "start_tag" | "end_tag" => Some(SyntaxScope::Tag),
        "property_name" | "field_name" | "pair" | "flow_pair" | "block_mapping_pair" => {
            Some(SyntaxScope::Property)
        }
        "ERROR" | "error" => Some(SyntaxScope::Error),
        _ if is_keyword_kind(kind) => Some(SyntaxScope::Keyword),
        _ => scope_for_language_node(language, kind, node),
    }
}

fn scope_for_language_node(language: &str, kind: &str, _node: &Node) -> Option<SyntaxScope> {
    match language {
        "rust" => match kind {
            "primitive_type" | "type_identifier" | "scoped_type_identifier" | "type_binding" => {
                Some(SyntaxScope::Type)
            }
            "function_item" | "function_signature" | "function_declarator" => {
                Some(SyntaxScope::Function)
            }
            "field_identifier" | "identifier" | "shorthand_field_identifier" => {
                Some(SyntaxScope::Variable)
            }
            _ => None,
        },
        "python" => match kind {
            "type" | "type_identifier" => Some(SyntaxScope::Type),
            "function_definition" | "lambda" => Some(SyntaxScope::Function),
            "identifier" => Some(SyntaxScope::Variable),
            "decorator" => Some(SyntaxScope::Attribute),
            _ => None,
        },
        "json" => match kind {
            "string" => Some(SyntaxScope::String),
            "number" => Some(SyntaxScope::Number),
            "true" | "false" | "null" => Some(SyntaxScope::Constant),
            _ => None,
        },
        "toml" => match kind {
            "string" => Some(SyntaxScope::String),
            "integer" | "float" => Some(SyntaxScope::Number),
            "boolean" => Some(SyntaxScope::Constant),
            "key" => Some(SyntaxScope::Property),
            _ => None,
        },
        "markdown" => match kind {
            "atx_heading" | "setext_heading" => Some(SyntaxScope::Keyword),
            "fenced_code_block" | "indented_code_block" | "code_fence_content" => {
                Some(SyntaxScope::String)
            }
            "emphasis" | "strong_emphasis" => Some(SyntaxScope::Type),
            "link" | "link_text" | "link_destination" => Some(SyntaxScope::Function),
            _ => None,
        },
        "javascript" => match kind {
            "type_identifier" | "type_annotation" => Some(SyntaxScope::Type),
            "function_declaration"
            | "function"
            | "method_definition"
            | "arrow_function"
            | "generator_function"
            | "generator_function_declaration" => Some(SyntaxScope::Function),
            "property_identifier" | "shorthand_property_identifier" | "identifier" => {
                Some(SyntaxScope::Variable)
            }
            "regex" | "regex_pattern" => Some(SyntaxScope::String),
            _ => None,
        },
        "yaml" => match kind {
            "block_mapping_pair" | "flow_pair" => Some(SyntaxScope::Property),
            "anchor_name" | "alias_name" | "tag" => Some(SyntaxScope::Attribute),
            "comment" => Some(SyntaxScope::Comment),
            _ => None,
        },
        _ => None,
    }
}

fn is_keyword_kind(kind: &str) -> bool {
    matches!(
        kind,
        "fn"
            | "let"
            | "mut"
            | "pub"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "use"
            | "mod"
            | "const"
            | "static"
            | "async"
            | "await"
            | "return"
            | "if"
            | "else"
            | "match"
            | "for"
            | "while"
            | "loop"
            | "break"
            | "continue"
            | "where"
            | "type"
            | "in"
            | "ref"
            | "move"
            | "unsafe"
            | "extern"
            | "crate"
            | "super"
            | "self"
            | "Self"
            | "as"
            | "dyn"
            | "box"
            | "yield"
            | "macro"
            | "def"
            | "class"
            | "import"
            | "from"
            | "pass"
            | "raise"
            | "try"
            | "except"
            | "finally"
            | "with"
            | "lambda"
            | "global"
            | "nonlocal"
            | "del"
            | "assert"
            | "elif"
            | "and"
            | "or"
            | "not"
            | "is"
            | "None"
            | "True"
            | "False"
            // JavaScript
            | "var"
            | "function"
            | "typeof"
            | "instanceof"
            | "new"
            | "this"
            | "throw"
            | "catch"
            | "switch"
            | "case"
            | "default"
            | "export"
            | "extends"
            | "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "void"
            | "delete"
            | "debugger"
            | "of"
            | "get"
            | "set"
            // YAML document markers often surface as keyword-like tokens
            | "---"
            | "..."
    )
}

fn scoped_lines(
    source: &str,
    scopes: &[SyntaxScope],
    first_line: u32,
    line_count: u32,
) -> Vec<SyntaxLine> {
    let mut out = Vec::with_capacity(line_count as usize);
    let mut byte_offset = 0usize;

    for (offset, line) in source.split('\n').take(line_count as usize).enumerate() {
        let line_start = byte_offset;
        let line_end = line_start.saturating_add(line.len());
        let line_scopes = if line.is_empty() {
            &[]
        } else {
            &scopes[line_start..line_end.min(scopes.len())]
        };

        out.push(SyntaxLine {
            line_number: first_line + offset as u32,
            segments: line_to_segments(line, line_scopes),
        });

        byte_offset = line_end.saturating_add(1);
    }

    out
}

fn line_to_segments(line: &str, scopes: &[SyntaxScope]) -> Vec<SyntaxSegment> {
    if line.is_empty() {
        return vec![SyntaxSegment {
            text: String::new(),
            scope: SyntaxScope::Plain,
        }];
    }

    let mut segments: Vec<SyntaxSegment> = Vec::new();
    let mut chars = line.char_indices().peekable();

    while let Some((byte_start, ch)) = chars.next() {
        let scope = scopes
            .get(byte_start)
            .copied()
            .unwrap_or(SyntaxScope::Plain);
        let mut text = ch.to_string();

        while let Some(&(next_byte, next_ch)) = chars.peek() {
            let next_scope = scopes
                .get(next_byte)
                .copied()
                .unwrap_or(SyntaxScope::Plain);
            if next_scope != scope {
                break;
            }
            chars.next();
            text.push(next_ch);
        }

        if let Some(last) = segments.last_mut() {
            if last.scope == scope {
                last.text.push_str(&text);
                continue;
            }
        }

        segments.push(SyntaxSegment { text, scope });
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_non_plain(segments: &[SyntaxSegment]) -> bool {
        segments.iter().any(|s| s.scope != SyntaxScope::Plain)
    }

    #[test]
    fn emits_plain_segments_for_unknown_language() {
        let h = SyntaxHighlighter::new();
        let lines = h.highlight_lines("plaintext", "fn main() {}\nprintln!(\"hi\");", 0, 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].segments.len(), 1);
        assert_eq!(lines[0].segments[0].scope, SyntaxScope::Plain);
    }

    #[test]
    fn rust_grammar_parses_and_tags_nodes() {
        use tree_sitter::Parser;

        let lang = crate::text::grammars::language_for_id("rust").expect("rust grammar");
        let mut parser = Parser::new();
        parser
            .set_language(&lang)
            .expect("rust grammar should load");
        let source = "fn main() { let x = 42; }";
        let tree = parser.parse(source, None).expect("parse should succeed");
        let scopes = super::byte_scopes(source, &tree, "rust");
        assert!(
            scopes.iter().any(|s| *s != SyntaxScope::Plain),
            "expected rust parse to assign non-Plain scopes"
        );
    }

    #[test]
    fn rust_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "fn main() {\n    let x = 42;\n}\n";
        let lines = h.highlight_lines("rust", source, 0, 3);
        assert_eq!(lines.len(), 3);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for Rust"
        );
    }

    #[test]
    fn python_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "def greet(name):\n    return f\"hello {name}\"\n";
        let lines = h.highlight_lines("python", source, 0, 2);
        assert_eq!(lines.len(), 2);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for Python"
        );
    }

    #[test]
    fn javascript_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "const greet = (name) => {\n  return `hello ${name}`;\n};\n";
        let lines = h.highlight_lines("javascript", source, 0, 3);
        assert_eq!(lines.len(), 3);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for JavaScript"
        );
    }

    #[test]
    fn yaml_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "name: orchid\nversion: 1\n# comment\n";
        let lines = h.highlight_lines("yaml", source, 0, 3);
        assert_eq!(lines.len(), 3);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for YAML"
        );
    }

    #[test]
    fn lists_available_languages() {
        let h = SyntaxHighlighter::new();
        let langs = h.available_languages();
        assert!(langs.contains(&"rust"));
        assert!(langs.contains(&"python"));
        assert!(langs.contains(&"javascript"));
        assert!(langs.contains(&"yaml"));
    }
}
