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

    /// Highlight `line_count` lines of the **full** document `source`,
    /// starting at `first_line` (0-based).
    ///
    /// `source` must be the entire file (LF-normalised). Parsing only a
    /// visible window loses multi-line context (block comments, strings).
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
        let Some(tree) = self.parse(language, source, None) else {
            return plain_lines(source, first_line, line_count);
        };
        self.highlight_from_tree(language, source, &tree, first_line, line_count)
    }

    /// Parse `source` with an optional previous tree for incremental updates.
    #[must_use]
    pub fn parse(
        &self,
        language: &str,
        source: &str,
        old_tree: Option<&Tree>,
    ) -> Option<Tree> {
        let ts_lang = language_for_id(language)?;
        let mut parser = Parser::new();
        parser.set_language(&ts_lang).ok()?;
        parser.parse(source, old_tree)
    }

    /// Highlight a visible window using an already-parsed tree.
    #[must_use]
    pub fn highlight_from_tree(
        &self,
        language: &str,
        source: &str,
        tree: &Tree,
        first_line: u32,
        line_count: u32,
    ) -> Vec<SyntaxLine> {
        if line_count == 0 {
            return Vec::new();
        }
        let Some((window_start, window_end)) =
            visible_byte_window(source, first_line, line_count)
        else {
            return Vec::new();
        };
        let scopes = byte_scopes_window(source, tree, language, window_start, window_end);
        scoped_lines_window(source, &scopes, window_start, first_line, line_count)
    }

    /// Languages the registry currently supports.
    #[must_use]
    pub fn available_languages(&self) -> Vec<&'static str> {
        grammars::HIGHLIGHT_LANGUAGES.to_vec()
    }
}

fn plain_lines(source: &str, first_line: u32, line_count: u32) -> Vec<SyntaxLine> {
    let mut out = Vec::with_capacity(line_count as usize);
    for (idx, line) in source.split('\n').enumerate() {
        let line_number = idx as u32;
        if line_number < first_line {
            continue;
        }
        if out.len() >= line_count as usize {
            break;
        }
        out.push(SyntaxLine {
            line_number,
            segments: vec![SyntaxSegment {
                text: line.to_string(),
                scope: SyntaxScope::Plain,
            }],
        });
    }
    out
}

#[cfg(test)]
fn byte_scopes(source: &str, tree: &Tree, language: &str) -> Vec<SyntaxScope> {
    byte_scopes_window(source, tree, language, 0, source.len())
}

fn byte_scopes_window(
    source: &str,
    tree: &Tree,
    language: &str,
    window_start: usize,
    window_end: usize,
) -> Vec<SyntaxScope> {
    let window_end = window_end.min(source.len()).max(window_start);
    let mut scopes = vec![SyntaxScope::Plain; window_end.saturating_sub(window_start)];
    if scopes.is_empty() {
        return scopes;
    }
    let mut cursor = tree.walk();
    apply_node_scopes_window(&mut cursor, language, window_start, window_end, &mut scopes);
    scopes
}

fn apply_node_scopes_window(
    cursor: &mut TreeCursor,
    language: &str,
    window_start: usize,
    window_end: usize,
    scopes: &mut [SyntaxScope],
) {
    let node = cursor.node();
    if node.end_byte() <= window_start || node.start_byte() >= window_end {
        return;
    }
    if let Some(scope) = scope_for_node(language, &node) {
        paint_scope_window(
            scopes,
            window_start,
            node.start_byte(),
            node.end_byte(),
            scope,
        );
    }

    if cursor.goto_first_child() {
        loop {
            apply_node_scopes_window(cursor, language, window_start, window_end, scopes);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn paint_scope_window(
    scopes: &mut [SyntaxScope],
    window_start: usize,
    abs_start: usize,
    abs_end: usize,
    scope: SyntaxScope,
) {
    let start = abs_start.max(window_start);
    let end = abs_end.min(window_start + scopes.len());
    if start >= end {
        return;
    }
    let rel_start = start - window_start;
    let rel_end = end - window_start;
    for slot in &mut scopes[rel_start..rel_end] {
        *slot = scope;
    }
}

fn visible_byte_window(source: &str, first_line: u32, line_count: u32) -> Option<(usize, usize)> {
    if line_count == 0 {
        return None;
    }
    let mut byte = 0usize;
    let mut window_start = None;
    let mut window_end = source.len();
    let last_line = first_line.saturating_add(line_count.saturating_sub(1));

    for (idx, line) in source.split('\n').enumerate() {
        let line_number = idx as u32;
        let line_end = byte + line.len();
        if line_number == first_line {
            window_start = Some(byte);
        }
        if line_number == last_line {
            window_end = line_end;
            break;
        }
        byte = line_end.saturating_add(1);
    }

    let start = window_start?;
    Some((start, window_end.min(source.len()).max(start)))
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
        "javascript" | "typescript" | "tsx" => match kind {
            "type_identifier"
            | "type_annotation"
            | "predefined_type"
            | "type_parameter"
            | "type_alias_declaration"
            | "interface_declaration"
            | "enum_declaration" => Some(SyntaxScope::Type),
            "function_declaration"
            | "function"
            | "method_definition"
            | "arrow_function"
            | "generator_function"
            | "generator_function_declaration"
            | "function_signature" => Some(SyntaxScope::Function),
            "property_identifier" | "shorthand_property_identifier" | "identifier" => {
                Some(SyntaxScope::Variable)
            }
            "regex" | "regex_pattern" => Some(SyntaxScope::String),
            "jsx_element" | "jsx_self_closing_element" | "jsx_opening_element"
            | "jsx_closing_element" | "jsx_attribute" => Some(SyntaxScope::Tag),
            _ => None,
        },
        "yaml" => match kind {
            "block_mapping_pair" | "flow_pair" => Some(SyntaxScope::Property),
            "anchor_name" | "alias_name" | "tag" => Some(SyntaxScope::Attribute),
            "comment" => Some(SyntaxScope::Comment),
            _ => None,
        },
        "go" => match kind {
            "type_identifier" | "type_spec" | "type_declaration" | "qualified_type" => {
                Some(SyntaxScope::Type)
            }
            "function_declaration" | "method_declaration" | "func_literal" => {
                Some(SyntaxScope::Function)
            }
            "field_identifier" | "identifier" | "package_identifier" => {
                Some(SyntaxScope::Variable)
            }
            "interpreted_string_literal" | "raw_string_literal" | "rune_literal" => {
                Some(SyntaxScope::String)
            }
            _ => None,
        },
        "bash" => match kind {
            "command_name" | "function_definition" => Some(SyntaxScope::Function),
            "variable_name" | "special_variable_name" | "simple_expansion"
            | "expansion" | "variable_assignment" => Some(SyntaxScope::Variable),
            "string" | "raw_string" | "ansi_c_string" | "translated_string" => {
                Some(SyntaxScope::String)
            }
            "file_redirect" | "heredoc_redirect" | "herestring_redirect" => {
                Some(SyntaxScope::Operator)
            }
            "test_operator" | "file_descriptor" => Some(SyntaxScope::Operator),
            _ => None,
        },
        "html" => match kind {
            "tag_name" | "start_tag" | "end_tag" | "self_closing_tag" | "doctype" => {
                Some(SyntaxScope::Tag)
            }
            "attribute_name" => Some(SyntaxScope::Attribute),
            "attribute_value" | "quoted_attribute_value" => Some(SyntaxScope::String),
            "text" | "raw_text" => Some(SyntaxScope::Plain),
            "entity" => Some(SyntaxScope::Constant),
            _ => None,
        },
        "css" => match kind {
            "tag_name" | "class_name" | "id_name" | "nesting_selector" | "universal_selector" => {
                Some(SyntaxScope::Tag)
            }
            "property_name" | "feature_name" | "attribute_name" => Some(SyntaxScope::Property),
            "plain_value" | "string_value" | "color_value" | "integer_value" | "float_value"
            | "unit" => Some(SyntaxScope::String),
            "function_name" => Some(SyntaxScope::Function),
            "important" | "keyword_query" | "at_keyword" => Some(SyntaxScope::Keyword),
            _ => None,
        },
        "c" | "cpp" => match kind {
            "primitive_type"
            | "type_identifier"
            | "sized_type_specifier"
            | "type_descriptor"
            | "class_specifier"
            | "struct_specifier"
            | "enum_specifier"
            | "union_specifier"
            | "template_type"
            | "dependent_type" => Some(SyntaxScope::Type),
            "function_definition"
            | "function_declarator"
            | "call_expression"
            | "field_expression" => Some(SyntaxScope::Function),
            "field_identifier" | "identifier" | "namespace_identifier" => {
                Some(SyntaxScope::Variable)
            }
            "string_literal" | "char_literal" | "raw_string_literal" | "system_lib_string" => {
                Some(SyntaxScope::String)
            }
            "preproc_include"
            | "preproc_def"
            | "preproc_function_def"
            | "preproc_call"
            | "preproc_ifdef"
            | "preproc_ifndef"
            | "preproc_else"
            | "preproc_elif"
            | "preproc_endif"
            | "preproc_if"
            | "preproc_arg"
            | "preproc_directive" => Some(SyntaxScope::Preprocessor),
            "null" | "true" | "false" | "nullptr" => Some(SyntaxScope::Constant),
            _ => None,
        },
        "java" => match kind {
            "type_identifier"
            | "generic_type"
            | "scoped_type_identifier"
            | "integral_type"
            | "floating_point_type"
            | "boolean_type"
            | "void_type"
            | "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => Some(SyntaxScope::Type),
            "method_declaration"
            | "constructor_declaration"
            | "method_invocation"
            | "object_creation_expression" => Some(SyntaxScope::Function),
            "identifier" | "field_access" => Some(SyntaxScope::Variable),
            "string_literal" | "character_literal" | "text_block" => Some(SyntaxScope::String),
            "marker_annotation" | "annotation" | "annotation_argument_list" => {
                Some(SyntaxScope::Attribute)
            }
            "null_literal" | "true" | "false" => Some(SyntaxScope::Constant),
            _ => None,
        },
        "ruby" => match kind {
            "constant" | "scope_resolution" | "class" | "module" => Some(SyntaxScope::Type),
            "method" | "singleton_method" | "call" | "method_call" | "identifier" => {
                Some(SyntaxScope::Function)
            }
            "instance_variable" | "class_variable" | "global_variable" | "simple_symbol"
            | "symbol" | "hash_key_symbol" => Some(SyntaxScope::Variable),
            "string" | "string_content" | "heredoc_content" | "heredoc_beginning"
            | "heredoc_end" | "regex" | "subshell" => Some(SyntaxScope::String),
            "integer" | "float" | "complex" | "rational" => Some(SyntaxScope::Number),
            "true" | "false" | "nil" | "self" => Some(SyntaxScope::Constant),
            "comment" => Some(SyntaxScope::Comment),
            _ => None,
        },
        "sql" => match kind {
            "keyword"
            | "keyword_select"
            | "keyword_from"
            | "keyword_where"
            | "keyword_insert"
            | "keyword_update"
            | "keyword_delete"
            | "keyword_create"
            | "keyword_drop"
            | "keyword_alter"
            | "keyword_table"
            | "keyword_into"
            | "keyword_values"
            | "keyword_set"
            | "keyword_join"
            | "keyword_on"
            | "keyword_as"
            | "keyword_and"
            | "keyword_or"
            | "keyword_not"
            | "keyword_null"
            | "keyword_order"
            | "keyword_by"
            | "keyword_group"
            | "keyword_having"
            | "keyword_limit"
            | "keyword_offset"
            | "keyword_union"
            | "keyword_all"
            | "keyword_distinct"
            | "keyword_inner"
            | "keyword_left"
            | "keyword_right"
            | "keyword_full"
            | "keyword_outer"
            | "keyword_cross"
            | "keyword_primary"
            | "keyword_key"
            | "keyword_foreign"
            | "keyword_references"
            | "keyword_index"
            | "keyword_view"
            | "keyword_database"
            | "keyword_schema"
            | "keyword_if"
            | "keyword_exists"
            | "keyword_cascade"
            | "keyword_restrict"
            | "keyword_default"
            | "keyword_constraint"
            | "keyword_unique"
            | "keyword_check"
            | "keyword_with"
            | "keyword_recursive"
            | "keyword_case"
            | "keyword_when"
            | "keyword_then"
            | "keyword_else"
            | "keyword_end"
            | "keyword_in"
            | "keyword_is"
            | "keyword_like"
            | "keyword_between"
            | "keyword_cast"
            | "keyword_asc"
            | "keyword_desc" => Some(SyntaxScope::Keyword),
            "type" | "keyword_int" | "keyword_bigint" | "keyword_smallint"
            | "keyword_tinyint" | "keyword_boolean" | "keyword_bool" | "keyword_text"
            | "keyword_varchar" | "keyword_char" | "keyword_date" | "keyword_datetime"
            | "keyword_timestamp" | "keyword_time" | "keyword_numeric" | "keyword_decimal"
            | "keyword_float" | "keyword_real" | "keyword_double" | "keyword_json"
            | "keyword_uuid" | "keyword_bytea" | "keyword_blob" | "keyword_serial" => {
                Some(SyntaxScope::Type)
            }
            "identifier" | "dotted_name" | "field" | "column_definition" => {
                Some(SyntaxScope::Variable)
            }
            "function_call" | "invocation" => Some(SyntaxScope::Function),
            "string" | "string_content" | "literal" => Some(SyntaxScope::String),
            "number" | "literal_value" => Some(SyntaxScope::Number),
            "comment" | "marginalia" => Some(SyntaxScope::Comment),
            "true" | "false" | "null" => Some(SyntaxScope::Constant),
            _ => None,
        },
        "php" => match kind {
            "name" | "qualified_name" | "namespace_name" | "class_declaration"
            | "interface_declaration" | "trait_declaration" | "enum_declaration"
            | "primitive_type" | "cast_type" | "named_type" | "optional_type"
            | "union_type" | "intersection_type" => Some(SyntaxScope::Type),
            "function_definition"
            | "method_declaration"
            | "function_call_expression"
            | "member_call_expression"
            | "scoped_call_expression"
            | "nullsafe_member_call_expression" => Some(SyntaxScope::Function),
            "variable_name" | "dynamic_variable_name" | "member_access_expression"
            | "nullsafe_member_access_expression" | "scoped_property_access_expression" => {
                Some(SyntaxScope::Variable)
            }
            "string" | "encapsed_string" | "string_content" | "heredoc" | "nowdoc"
            | "shell_command_expression" => Some(SyntaxScope::String),
            "integer" | "float" => Some(SyntaxScope::Number),
            "null" | "true" | "false" => Some(SyntaxScope::Constant),
            "attribute" | "attribute_list" | "attribute_group" => Some(SyntaxScope::Attribute),
            "php_tag" | "text_interpolation" => Some(SyntaxScope::Preprocessor),
            _ => None,
        },
        "kotlin" => match kind {
            "type_identifier"
            | "user_type"
            | "nullable_type"
            | "function_type"
            | "class_declaration"
            | "object_declaration"
            | "interface_declaration"
            | "enum_class_body"
            | "type_alias" => Some(SyntaxScope::Type),
            "function_declaration"
            | "secondary_constructor"
            | "call_expression"
            | "navigation_expression" => Some(SyntaxScope::Function),
            "simple_identifier" | "identifier" => Some(SyntaxScope::Variable),
            "string_literal" | "character_literal" | "line_string_literal"
            | "multi_line_string_literal" => Some(SyntaxScope::String),
            "integer_literal" | "real_literal" | "hex_literal" | "bin_literal" => {
                Some(SyntaxScope::Number)
            }
            "null_literal" | "boolean_literal" | "true" | "false" => Some(SyntaxScope::Constant),
            "annotation" | "annotation_use_site_target" => Some(SyntaxScope::Attribute),
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
            // Go / Bash (shared tokens like package/export/select already listed above)
            | "func"
            | "defer"
            | "go"
            | "chan"
            | "map"
            | "range"
            | "fallthrough"
            | "goto"
            | "nil"
            | "iota"
            | "then"
            | "fi"
            | "do"
            | "done"
            | "esac"
            | "until"
            | "declare"
            | "local"
            | "readonly"
            | "unset"
            | "source"
            | "alias"
            | "unalias"
            | "builtin"
            | "command"
            | "coproc"
            | "time"
            // C / C++ (void already listed under JavaScript)
            | "typedef"
            | "sizeof"
            | "alignof"
            | "alignas"
            | "volatile"
            | "register"
            | "signed"
            | "unsigned"
            | "short"
            | "long"
            | "int"
            | "char"
            | "float"
            | "double"
            | "auto"
            | "inline"
            | "restrict"
            | "namespace"
            | "using"
            | "template"
            | "typename"
            | "virtual"
            | "override"
            | "final"
            | "explicit"
            | "friend"
            | "operator"
            | "constexpr"
            | "consteval"
            | "constinit"
            | "noexcept"
            | "concept"
            | "requires"
            | "co_await"
            | "co_yield"
            | "co_return"
            | "nullptr"
            | "wchar_t"
            | "char8_t"
            | "char16_t"
            | "char32_t"
            // Java / Ruby / PHP (shared tokens already listed above)
            | "abstract"
            | "synchronized"
            | "transient"
            | "native"
            | "strictfp"
            | "throws"
            | "begin"
            | "end"
            | "unless"
            | "rescue"
            | "ensure"
            | "retry"
            | "redo"
            | "next"
            | "module"
            | "undef"
            | "defined?"
            | "echo"
            | "print"
            | "die"
            | "exit"
            | "isset"
            | "empty"
            | "list"
            | "array"
            | "callable"
            | "iterable"
            | "clone"
            | "include"
            | "include_once"
            | "require"
            | "require_once"
            | "insteadof"
            | "foreach"
            | "enddeclare"
            | "endfor"
            | "endforeach"
            | "endif"
            | "endswitch"
            | "endwhile"
            | "parent"
            | "xor"
            // Kotlin (shared tokens already listed above)
            | "fun"
            | "val"
            | "object"
            | "companion"
            | "data"
            | "sealed"
            | "inner"
            | "open"
            | "lateinit"
            | "suspend"
            | "tailrec"
            | "infix"
            | "noinline"
            | "crossinline"
            | "reified"
            | "expect"
            | "actual"
            | "typealias"
            | "when"
            | "by"
            | "init"
            | "constructor"
            | "field"
            | "it"
    )
}

fn scoped_lines_window(
    source: &str,
    scopes: &[SyntaxScope],
    window_start: usize,
    first_line: u32,
    line_count: u32,
) -> Vec<SyntaxLine> {
    let mut out = Vec::with_capacity(line_count as usize);
    let mut byte_offset = 0usize;

    for (idx, line) in source.split('\n').enumerate() {
        let line_number = idx as u32;
        let line_start = byte_offset;
        let line_end = line_start.saturating_add(line.len());

        if line_number >= first_line && out.len() < line_count as usize {
            let rel_start = line_start.saturating_sub(window_start);
            let rel_end = line_end.saturating_sub(window_start).min(scopes.len());
            let line_scopes = if line.is_empty() || rel_start >= scopes.len() {
                &[]
            } else {
                &scopes[rel_start..rel_end.max(rel_start)]
            };
            out.push(SyntaxLine {
                line_number,
                segments: line_to_segments(line, line_scopes),
            });
        }

        byte_offset = line_end.saturating_add(1);
        if out.len() >= line_count as usize && line_number >= first_line {
            break;
        }
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
    fn block_comment_keeps_scope_when_window_starts_mid_comment() {
        let h = SyntaxHighlighter::new();
        let source = "/*\ncomment body\nstill comment\n*/\nfn main() {}\n";
        // Visible window starts on line 1 ("comment body"), inside the block comment.
        let lines = h.highlight_lines("rust", source, 1, 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_number, 1);
        assert!(
            lines[0]
                .segments
                .iter()
                .any(|s| s.scope == SyntaxScope::Comment),
            "expected comment scope when window starts mid block-comment, got {:?}",
            lines[0].segments
        );
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
    fn typescript_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "type Id = string;\nfunction greet(name: string): string {\n  return name;\n}\n";
        let lines = h.highlight_lines("typescript", source, 0, 4);
        assert_eq!(lines.len(), 4);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for TypeScript"
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
    fn go_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "package main\n\nfunc greet(name string) string {\n\treturn name\n}\n";
        let lines = h.highlight_lines("go", source, 0, 5);
        assert_eq!(lines.len(), 5);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for Go"
        );
    }

    #[test]
    fn bash_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "#!/bin/bash\nif [ -n \"$NAME\" ]; then\n  echo \"hello $NAME\"\nfi\n";
        let lines = h.highlight_lines("bash", source, 0, 4);
        assert_eq!(lines.len(), 4);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for Bash"
        );
    }

    #[test]
    fn html_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "<div class=\"box\">\n  <span>hi</span>\n</div>\n";
        let lines = h.highlight_lines("html", source, 0, 3);
        assert_eq!(lines.len(), 3);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for HTML"
        );
    }

    #[test]
    fn css_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = ".box {\n  color: #fff;\n  margin: 1rem;\n}\n";
        let lines = h.highlight_lines("css", source, 0, 4);
        assert_eq!(lines.len(), 4);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for CSS"
        );
    }

    #[test]
    fn c_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "#include <stdio.h>\nint main(void) {\n  return 0;\n}\n";
        let lines = h.highlight_lines("c", source, 0, 4);
        assert_eq!(lines.len(), 4);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for C"
        );
    }

    #[test]
    fn cpp_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "#include <string>\nstd::string greet(const std::string& name) {\n  return name;\n}\n";
        let lines = h.highlight_lines("cpp", source, 0, 4);
        assert_eq!(lines.len(), 4);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for C++"
        );
    }

    #[test]
    fn java_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "class Greeter {\n  String greet(String name) {\n    return name;\n  }\n}\n";
        let lines = h.highlight_lines("java", source, 0, 5);
        assert_eq!(lines.len(), 5);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for Java"
        );
    }

    #[test]
    fn ruby_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "def greet(name)\n  \"hello #{name}\"\nend\n";
        let lines = h.highlight_lines("ruby", source, 0, 3);
        assert_eq!(lines.len(), 3);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for Ruby"
        );
    }

    #[test]
    fn sql_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "SELECT id, name\nFROM users\nWHERE active = 1;\n";
        let lines = h.highlight_lines("sql", source, 0, 3);
        assert_eq!(lines.len(), 3);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for SQL"
        );
    }

    #[test]
    fn php_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "<?php\nfunction greet(string $name): string {\n  return $name;\n}\n";
        let lines = h.highlight_lines("php", source, 0, 4);
        assert_eq!(lines.len(), 4);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for PHP"
        );
    }

    #[test]
    fn kotlin_highlighting_produces_non_plain_segments() {
        let h = SyntaxHighlighter::new();
        let source = "fun greet(name: String): String {\n  return name\n}\n";
        let lines = h.highlight_lines("kotlin", source, 0, 3);
        assert_eq!(lines.len(), 3);
        assert!(
            lines.iter().any(|line| has_non_plain(&line.segments)),
            "expected at least one non-Plain segment for Kotlin"
        );
    }

    #[test]
    fn lists_available_languages() {
        let h = SyntaxHighlighter::new();
        let langs = h.available_languages();
        assert!(langs.contains(&"rust"));
        assert!(langs.contains(&"python"));
        assert!(langs.contains(&"javascript"));
        assert!(langs.contains(&"typescript"));
        assert!(langs.contains(&"tsx"));
        assert!(langs.contains(&"yaml"));
        assert!(langs.contains(&"go"));
        assert!(langs.contains(&"bash"));
        assert!(langs.contains(&"html"));
        assert!(langs.contains(&"css"));
        assert!(langs.contains(&"c"));
        assert!(langs.contains(&"cpp"));
        assert!(langs.contains(&"java"));
        assert!(langs.contains(&"ruby"));
        assert!(langs.contains(&"sql"));
        assert!(langs.contains(&"php"));
        assert!(langs.contains(&"kotlin"));
    }
}
