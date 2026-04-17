use super::SqliteRenderer;
use crate::query::error::RenderError;
use crate::query::ir::{Filter, MatchOp};
use serde_json::Value;
use std::collections::HashMap;

/// Escape SQL LIKE metacharacters (%, _) in a pattern
pub(super) fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

impl SqliteRenderer {
    pub(super) fn render_filter(
        &self,
        filter: &Filter,
        index: usize,
        params: &mut HashMap<String, Value>,
    ) -> Result<String, RenderError> {
        let param_name = format!("filter_{}", index);

        match &filter.op {
            MatchOp::Eq => {
                if filter.value == Value::Null {
                    Ok(format!("{} IS NULL", filter.field))
                } else {
                    params.insert(param_name.clone(), filter.value.clone());
                    Ok(format!("{} = :{}", filter.field, param_name))
                }
            }
            MatchOp::Ne => {
                if filter.value == Value::Null {
                    Ok(format!("{} IS NOT NULL", filter.field))
                } else {
                    params.insert(param_name.clone(), filter.value.clone());
                    Ok(format!("{} != :{}", filter.field, param_name))
                }
            }
            MatchOp::Contains => {
                if let Value::String(s) = &filter.value {
                    let escaped = escape_like_pattern(s);
                    params.insert(param_name.clone(), Value::String(format!("%{}%", escaped)));
                    Ok(format!(
                        "{} LIKE :{} ESCAPE '\\\\'",
                        filter.field, param_name
                    ))
                } else {
                    Err(RenderError::UnsupportedFilter {
                        message: format!("CONTAINS requires string value, got {:?}", filter.value),
                    })
                }
            }
            MatchOp::StartsWith => {
                if let Value::String(s) = &filter.value {
                    let escaped = escape_like_pattern(s);
                    params.insert(param_name.clone(), Value::String(format!("{}%", escaped)));
                    Ok(format!(
                        "{} LIKE :{} ESCAPE '\\\\'",
                        filter.field, param_name
                    ))
                } else {
                    Err(RenderError::UnsupportedFilter {
                        message: format!(
                            "STARTS WITH requires string value, got {:?}",
                            filter.value
                        ),
                    })
                }
            }
            MatchOp::EndsWith => {
                if let Value::String(s) = &filter.value {
                    let escaped = escape_like_pattern(s);
                    params.insert(param_name.clone(), Value::String(format!("%{}", escaped)));
                    Ok(format!(
                        "{} LIKE :{} ESCAPE '\\\\'",
                        filter.field, param_name
                    ))
                } else {
                    Err(RenderError::UnsupportedFilter {
                        message: format!("ENDS WITH requires string value, got {:?}", filter.value),
                    })
                }
            }
        }
    }
}
