//! Quick calculator source — queries that start with `=` evaluate an expression.

use async_trait::async_trait;

use super::{ActionTarget, SearchCandidate, SearchSource};
use crate::builtin::calculator::{evaluate_expression, format_result, AngleMode};

/// Source id.
pub const SOURCE_ID: &str = "calculator";

/// Evaluates `=expr` queries for universal search.
#[derive(Debug, Default)]
pub struct CalculatorSource;

impl CalculatorSource {
    /// Convenience constructor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SearchSource for CalculatorSource {
    fn id(&self) -> &'static str { SOURCE_ID }
    fn name_key(&self) -> &'static str { "search-source-calculator" }
    fn icon(&self) -> &'static str { "calculator" }
    async fn search(&self, query: &str, limit: usize) -> Vec<SearchCandidate> {
        if limit == 0 { return Vec::new(); }
        let trimmed = query.trim();
        let Some(rest) = trimmed.strip_prefix('=') else { return Vec::new(); };
        let expr = rest.trim();
        if expr.is_empty() { return Vec::new(); }
        match evaluate_expression(expr, AngleMode::Degrees) {
            Ok(v) => {
                let result = format_result(v);
                vec![SearchCandidate {
                    id: format!("calc:{expr}"),
                    source_id: SOURCE_ID,
                    title: result.clone(),
                    subtitle: Some(format!("={expr}")),
                    icon: "calculator",
                    score: 200,
                    action_hint: Some("search-calc-copy-hint".into()),
                    action_target: ActionTarget::CopyText(result),
                }]
            }
            Err(_) => Vec::new(),
        }
    }
}
