//! Tokeniser and parser for Orchid command strings.
//!
//! Input format (shell-like, without a real shell):
//!
//! ```text
//! orc fs move "C:\\Users\\a.txt" "D:\\backup\\" --verbose --conflict=skip
//! ```
//!
//! * A leading `orc` is optional and stripped.
//! * Double-quoted arguments may contain spaces and `\"` / `\\` escapes.
//! * `--flag` is a flag, `--key=value` is an option, `--key value` is also an
//!   option.
//! * Everything else is a positional argument.

use std::collections::{HashMap, HashSet};

use crate::command::registry::CommandRegistry;
use crate::command::descriptor::CommandDescriptor;
use crate::error::{CoreError, Result};

/// Structured form of a parsed command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedCommand {
    /// Resolved verb, e.g. `"fs move"`. If no registry is consulted this is
    /// just the first token.
    pub verb: String,
    /// Positional arguments in order.
    pub positional: Vec<String>,
    /// Boolean flags (`--flag`).
    pub flags: HashSet<String>,
    /// Options with values (`--key=value` or `--key value`).
    pub options: HashMap<String, String>,
}

/// Tokenise `input` and parse it into a [`ParsedCommand`], with no registry
/// consultation (the verb is just the first token).
///
/// # Errors
///
/// * [`CoreError::InvalidCommandSyntax`] for unterminated quotes or empty
///   input.
///
/// # Examples
///
/// ```
/// use orchid_core::parse_command_line;
/// let p = parse_command_line(r#"orc fs move "a" "b" --verbose"#).unwrap();
/// assert_eq!(p.verb, "fs");
/// assert_eq!(p.positional, vec!["move", "a", "b"]);
/// assert!(p.flags.contains("verbose"));
/// ```
pub fn parse_command_line(input: &str) -> Result<ParsedCommand> {
    let tokens = tokenise(input)?;
    build_parsed(tokens, None)
}

/// Like [`parse_command_line`], but uses a [`CommandRegistry`] to resolve
/// multi-word verbs (`"fs move"` vs `"fs"`).
///
/// Returns the parsed command plus the resolved [`CommandDescriptor`].
///
/// # Errors
///
/// In addition to [`parse_command_line`]'s errors, returns
/// [`CoreError::CommandNotFound`] if no registered verb matches the input.
pub fn parse_command_line_with_registry(
    input: &str,
    registry: &CommandRegistry,
) -> Result<(ParsedCommand, CommandDescriptor)> {
    let tokens = tokenise(input)?;
    let parsed = build_parsed(tokens, Some(registry))?;
    let desc = registry
        .get_by_verb(&parsed.verb)
        .ok_or_else(|| CoreError::CommandNotFound(parsed.verb.clone()))?;
    Ok((parsed, desc))
}

// ---------------------------------------------------------------------------
// Implementation details
// ---------------------------------------------------------------------------

fn tokenise(input: &str) -> Result<Vec<String>> {
    let mut trimmed = input.trim();
    if let Some(stripped) = trimmed.strip_prefix("orc ") {
        trimmed = stripped.trim_start();
    } else if trimmed == "orc" {
        return Err(CoreError::InvalidCommandSyntax {
            reason: "empty".into(),
        });
    }
    if trimmed.is_empty() {
        return Err(CoreError::InvalidCommandSyntax {
            reason: "empty".into(),
        });
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut chars = trimmed.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quote {
            match c {
                '"' => {
                    in_quote = false;
                    tokens.push(std::mem::take(&mut current));
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    } else {
                        return Err(CoreError::InvalidCommandSyntax {
                            reason: "trailing backslash in quoted string".into(),
                        });
                    }
                }
                _ => current.push(c),
            }
        } else {
            match c {
                '"' => in_quote = true,
                ws if ws.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(c),
            }
        }
    }

    if in_quote {
        return Err(CoreError::InvalidCommandSyntax {
            reason: "unterminated quoted string".into(),
        });
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    if tokens.is_empty() {
        return Err(CoreError::InvalidCommandSyntax {
            reason: "empty".into(),
        });
    }
    Ok(tokens)
}

fn build_parsed(
    tokens: Vec<String>,
    registry: Option<&CommandRegistry>,
) -> Result<ParsedCommand> {
    // Split the leading tokens into "verb tokens" (no leading dash) and
    // arguments. Arguments start at the first token beginning with `-` or
    // after the longest registered verb prefix, whichever comes first.
    let mut non_flag_prefix_end = 0;
    while non_flag_prefix_end < tokens.len()
        && !tokens[non_flag_prefix_end].starts_with('-')
    {
        non_flag_prefix_end += 1;
    }
    let verb_candidates = &tokens[..non_flag_prefix_end];

    let (verb, verb_len) = match registry {
        Some(reg) => {
            // Longest prefix that resolves in the registry.
            let mut best: Option<(String, usize)> = None;
            for end in (1..=verb_candidates.len()).rev() {
                let candidate = verb_candidates[..end].join(" ");
                if reg.has_verb(&candidate) {
                    best = Some((candidate, end));
                    break;
                }
            }
            match best {
                Some((v, n)) => (v, n),
                None => {
                    // Fall back to first token; caller will most likely raise
                    // CommandNotFound, but we still produce a best-effort
                    // parse for diagnostics.
                    (verb_candidates[0].clone(), 1)
                }
            }
        }
        None => (verb_candidates[0].clone(), 1),
    };

    let mut positional = Vec::new();
    let mut flags = HashSet::new();
    let mut options = HashMap::new();

    let mut it = tokens.into_iter().skip(verb_len).peekable();
    while let Some(tok) = it.next() {
        if let Some(rest) = tok.strip_prefix("--") {
            if rest.is_empty() {
                // `--` alone terminates option parsing; everything after is
                // positional.
                positional.extend(it.by_ref());
                break;
            }
            if let Some((k, v)) = rest.split_once('=') {
                options.insert(k.to_string(), v.to_string());
            } else {
                // Look ahead: if the next token exists and does not start
                // with '-', treat this as `--key value`. Otherwise it's a
                // flag.
                let next_is_value = it
                    .peek()
                    .map(|n| !n.starts_with('-'))
                    .unwrap_or(false);
                if next_is_value {
                    let value = it.next().expect("peeked above");
                    options.insert(rest.to_string(), value);
                } else {
                    flags.insert(rest.to_string());
                }
            }
        } else if let Some(rest) = tok.strip_prefix('-') {
            // Short flag form; treat identically to `--<rest>` flag.
            if rest.is_empty() {
                positional.push(tok);
            } else {
                flags.insert(rest.to_string());
            }
        } else {
            positional.push(tok);
        }
    }

    Ok(ParsedCommand {
        verb,
        positional,
        flags,
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenise_quotes_and_escapes() {
        let p =
            parse_command_line(r#"orc fs move "C:\\Users\\a.txt" "D:\\backup\\" --verbose"#)
                .unwrap();
        assert_eq!(p.verb, "fs");
        assert_eq!(
            p.positional,
            vec![
                "move".to_string(),
                r#"C:\Users\a.txt"#.to_string(),
                r#"D:\backup\"#.to_string(),
            ]
        );
        assert!(p.flags.contains("verbose"));
    }

    #[test]
    fn options_both_forms() {
        let p = parse_command_line("orc foo --k1=v1 --k2 v2 --flag --k3=v3").unwrap();
        assert_eq!(p.options.get("k1"), Some(&"v1".to_string()));
        assert_eq!(p.options.get("k2"), Some(&"v2".to_string()));
        assert_eq!(p.options.get("k3"), Some(&"v3".to_string()));
        assert!(p.flags.contains("flag"));
    }

    #[test]
    fn leading_orc_is_optional() {
        let a = parse_command_line("orc foo bar").unwrap();
        let b = parse_command_line("foo bar").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_input_is_an_error() {
        assert!(parse_command_line("").is_err());
        assert!(parse_command_line("   ").is_err());
        assert!(parse_command_line("orc").is_err());
        assert!(parse_command_line("orc   ").is_err());
    }

    #[test]
    fn unterminated_quote_is_an_error() {
        let err = parse_command_line(r#"orc fs move "C:\a.txt"#).unwrap_err();
        match err {
            CoreError::InvalidCommandSyntax { reason } => assert!(reason.contains("unterminated")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn double_dash_terminates_options() {
        let p = parse_command_line("orc foo a -- --not-a-flag b").unwrap();
        assert_eq!(
            p.positional,
            vec!["a".to_string(), "--not-a-flag".to_string(), "b".to_string()]
        );
        assert!(p.flags.is_empty());
    }
}
