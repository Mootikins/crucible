//! Filter translation transform.
//!
//! Translates jaq filter expressions into IR Filter nodes that can be
//! pushed to the database side.
//!
//! Supported patterns:
//! - `select(.field)` -> existence check
//! - `select(.field == "value")` -> equality check
//! - `select(.tags | contains("x"))` -> array contains
//! - `select(.title | startswith("x"))` -> string prefix

use crate::error::TransformError;
use crate::ir::{Filter, GraphIR, MatchOp};
use crate::transform::QueryTransform;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

// ============================================================================
// Filter patterns
// ============================================================================

/// Pattern: select(.field == "value") or select(.field == 'value')
static EQUALITY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^\s*select\s*\(\s*\.(\w+)\s*==\s*["']([^"']+)["']\s*\)\s*$"#).unwrap()
});

/// Pattern: select(.field != "value") or select(.field != 'value')
static NOT_EQUAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^\s*select\s*\(\s*\.(\w+)\s*!=\s*["']([^"']+)["']\s*\)\s*$"#).unwrap()
});

/// Pattern: select(.field | contains("value"))
static CONTAINS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^\s*select\s*\(\s*\.(\w+)\s*\|\s*contains\s*\(\s*["']([^"']+)["']\s*\)\s*\)\s*$"#)
        .unwrap()
});

/// Pattern: select(.field | startswith("value"))
static STARTSWITH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"^\s*select\s*\(\s*\.(\w+)\s*\|\s*startswith\s*\(\s*["']([^"']+)["']\s*\)\s*\)\s*$"#,
    )
    .unwrap()
});

/// Pattern: select(.field | endswith("value"))
static ENDSWITH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"^\s*select\s*\(\s*\.(\w+)\s*\|\s*endswith\s*\(\s*["']([^"']+)["']\s*\)\s*\)\s*$"#,
    )
    .unwrap()
});

/// Pattern: select(.field) - existence check
static EXISTS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*select\s*\(\s*\.(\w+)\s*\)\s*$"#).unwrap());

// ============================================================================
// Transform implementation
// ============================================================================

/// Filter translation transform.
///
/// Attempts to translate jaq filters to IR Filter nodes.
/// Unsupported patterns are left in `post_filter` for in-memory processing.
pub struct FilterTransform;

impl QueryTransform for FilterTransform {
    fn name(&self) -> &'static str {
        "filter"
    }

    fn transform(&self, mut ir: GraphIR) -> Result<GraphIR, TransformError> {
        if let Some(post_filter) = &ir.post_filter {
            // Split by " | " but only at top level (not inside parentheses)
            // Simple heuristic: split by " | " which is the jaq pipe syntax
            let segments = self.split_top_level_pipes(post_filter);
            let mut remaining = Vec::new();

            for segment in segments {
                let segment = segment.trim();
                if let Some(filter) = self.try_translate_filter(segment) {
                    ir.filters.push(filter);
                } else {
                    remaining.push(segment.to_string());
                }
            }

            // Update post_filter with remaining untranslated segments
            ir.post_filter = if remaining.is_empty() {
                None
            } else {
                Some(remaining.join(" | "))
            };
        }

        Ok(ir)
    }
}

impl FilterTransform {
    /// Split by " | " at the top level only (not inside parentheses)
    fn split_top_level_pipes<'a>(&self, input: &'a str) -> Vec<&'a str> {
        let mut segments = Vec::new();
        let mut depth: i32 = 0;
        let mut last_split = 0;
        let bytes = input.as_bytes();

        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth = depth.saturating_sub(1),
                b'|' if depth == 0 => {
                    // Check if this is " | " (with spaces)
                    if i > 0 && i + 1 < bytes.len() {
                        let before_space = i > 0 && bytes[i - 1] == b' ';
                        let after_space = i + 1 < bytes.len() && bytes[i + 1] == b' ';
                        if before_space && after_space {
                            segments.push(&input[last_split..i - 1]);
                            last_split = i + 2; // Skip " | "
                            i += 1; // Skip past the space after |
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }

        // Add the last segment
        if last_split < input.len() {
            segments.push(&input[last_split..]);
        }

        // If no splits were made, return the whole input
        if segments.is_empty() {
            segments.push(input);
        }

        segments
    }

    /// Try to translate a single filter segment to an IR Filter
    fn try_translate_filter(&self, segment: &str) -> Option<Filter> {
        // select(.field == "value")
        if let Some(caps) = EQUALITY_RE.captures(segment) {
            return Some(Filter {
                field: caps[1].to_string(),
                op: MatchOp::Eq,
                value: Value::String(caps[2].to_string()),
            });
        }

        // select(.field != "value")
        if let Some(caps) = NOT_EQUAL_RE.captures(segment) {
            return Some(Filter {
                field: caps[1].to_string(),
                op: MatchOp::Ne,
                value: Value::String(caps[2].to_string()),
            });
        }

        // select(.field | contains("value"))
        if let Some(caps) = CONTAINS_RE.captures(segment) {
            return Some(Filter {
                field: caps[1].to_string(),
                op: MatchOp::Contains,
                value: Value::String(caps[2].to_string()),
            });
        }

        // select(.field | startswith("value"))
        if let Some(caps) = STARTSWITH_RE.captures(segment) {
            return Some(Filter {
                field: caps[1].to_string(),
                op: MatchOp::StartsWith,
                value: Value::String(caps[2].to_string()),
            });
        }

        // select(.field | endswith("value"))
        if let Some(caps) = ENDSWITH_RE.captures(segment) {
            return Some(Filter {
                field: caps[1].to_string(),
                op: MatchOp::EndsWith,
                value: Value::String(caps[2].to_string()),
            });
        }

        // select(.field) - existence (we'll treat it as not-null check)
        if let Some(caps) = EXISTS_RE.captures(segment) {
            return Some(Filter {
                field: caps[1].to_string(),
                op: MatchOp::Ne,
                value: Value::Null,
            });
        }

        // Unsupported pattern
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::QuerySource;

    fn make_ir_with_filter(filter: &str) -> GraphIR {
        GraphIR {
            post_filter: Some(filter.to_string()),
            source: QuerySource::ByTitle("Test".to_string()),
            ..Default::default()
        }
    }

    // =========================================================================
    // Equality tests
    // =========================================================================

    #[test]
    fn test_translate_equality() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"select(.status == "active")"#);

        let result = transform.transform(ir).unwrap();

        assert!(result.post_filter.is_none());
        assert_eq!(result.filters.len(), 1);
        assert_eq!(result.filters[0].field, "status");
        assert_eq!(result.filters[0].op, MatchOp::Eq);
        assert_eq!(result.filters[0].value, Value::String("active".to_string()));
    }

    #[test]
    fn test_translate_equality_single_quotes() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"select(.status == 'active')"#);

        let result = transform.transform(ir).unwrap();

        assert!(result.post_filter.is_none());
        assert_eq!(result.filters[0].value, Value::String("active".to_string()));
    }

    // =========================================================================
    // Inequality tests
    // =========================================================================

    #[test]
    fn test_translate_not_equal() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"select(.status != "archived")"#);

        let result = transform.transform(ir).unwrap();

        assert_eq!(result.filters[0].op, MatchOp::Ne);
        assert_eq!(
            result.filters[0].value,
            Value::String("archived".to_string())
        );
    }

    // =========================================================================
    // Contains tests
    // =========================================================================

    #[test]
    fn test_translate_contains() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"select(.tags | contains("project"))"#);

        let result = transform.transform(ir).unwrap();

        assert!(result.post_filter.is_none());
        assert_eq!(result.filters[0].field, "tags");
        assert_eq!(result.filters[0].op, MatchOp::Contains);
        assert_eq!(
            result.filters[0].value,
            Value::String("project".to_string())
        );
    }

    // =========================================================================
    // StartsWith tests
    // =========================================================================

    #[test]
    fn test_translate_startswith() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"select(.title | startswith("Chapter"))"#);

        let result = transform.transform(ir).unwrap();

        assert!(result.post_filter.is_none());
        assert_eq!(result.filters[0].field, "title");
        assert_eq!(result.filters[0].op, MatchOp::StartsWith);
        assert_eq!(
            result.filters[0].value,
            Value::String("Chapter".to_string())
        );
    }

    // =========================================================================
    // EndsWith tests
    // =========================================================================

    #[test]
    fn test_translate_endswith() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"select(.path | endswith(".md"))"#);

        let result = transform.transform(ir).unwrap();

        assert!(result.post_filter.is_none());
        assert_eq!(result.filters[0].field, "path");
        assert_eq!(result.filters[0].op, MatchOp::EndsWith);
        assert_eq!(result.filters[0].value, Value::String(".md".to_string()));
    }

    // =========================================================================
    // Existence tests
    // =========================================================================

    #[test]
    fn test_translate_exists() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"select(.description)"#);

        let result = transform.transform(ir).unwrap();

        assert!(result.post_filter.is_none());
        assert_eq!(result.filters[0].field, "description");
        assert_eq!(result.filters[0].op, MatchOp::Ne);
        assert_eq!(result.filters[0].value, Value::Null);
    }

    // =========================================================================
    // Mixed/unsupported tests
    // =========================================================================

    #[test]
    fn test_unsupported_filter_preserved() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"map(.title)"#);

        let result = transform.transform(ir).unwrap();

        assert!(result.filters.is_empty());
        assert_eq!(result.post_filter, Some("map(.title)".to_string()));
    }

    #[test]
    fn test_mixed_filters() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(r#"select(.status == "active") | map(.title)"#);

        let result = transform.transform(ir).unwrap();

        // One translated, one preserved
        assert_eq!(result.filters.len(), 1);
        assert_eq!(result.filters[0].field, "status");
        assert_eq!(result.post_filter, Some("map(.title)".to_string()));
    }

    #[test]
    fn test_multiple_translatable_filters() {
        let transform = FilterTransform;
        let ir = make_ir_with_filter(
            r#"select(.status == "active") | select(.tags | contains("project"))"#,
        );

        let result = transform.transform(ir).unwrap();

        assert!(result.post_filter.is_none());
        assert_eq!(result.filters.len(), 2);
        assert_eq!(result.filters[0].field, "status");
        assert_eq!(result.filters[1].field, "tags");
    }

    #[test]
    fn test_no_filter() {
        let transform = FilterTransform;
        let ir = GraphIR::default();

        let result = transform.transform(ir).unwrap();

        assert!(result.post_filter.is_none());
        assert!(result.filters.is_empty());
    }
}
