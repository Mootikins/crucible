use serde::{Deserialize, Serialize};

/// LaTeX mathematical expression
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatexExpression {
    /// LaTeX expression content
    pub expression: String,

    /// Whether this is inline ($) or block ($$) math
    pub is_block: bool,

    /// Character offset in source note
    pub offset: usize,

    /// Length of the expression in source
    pub length: usize,
}

impl LatexExpression {
    /// Create a new LaTeX expression
    pub fn new(expression: String, is_block: bool, offset: usize, length: usize) -> Self {
        Self {
            expression,
            is_block,
            offset,
            length,
        }
    }
}
