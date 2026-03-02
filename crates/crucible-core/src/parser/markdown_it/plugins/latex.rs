//! LaTeX plugin for markdown-it ($...$ and $$...$$)
//!
//! Implements LaTeX mathematical expression syntax:
//! - `$...$` - Inline math
//! - `$$...$$` - Block math (display mode)
//!
//! Includes validation for balanced braces and dangerous commands.

use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};

/// Custom AST node for LaTeX expressions
#[derive(Debug, Clone)]
pub struct LatexNode {
    pub expression: String,
    pub is_block: bool,
    pub offset: usize,
}

impl NodeValue for LatexNode {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        if self.is_block {
            // Block math - render as display mode
            fmt.open("div", &[("class", "math-block".to_string())]);
            fmt.text(&format!("$${}$$", self.expression));
            fmt.close("div");
        } else {
            // Inline math
            fmt.open("span", &[("class", "math-inline".to_string())]);
            fmt.text(&format!("${}<br>$", self.expression));
            fmt.close("span");
        }
    }
}

/// LaTeX scanner - matches $...$ and $$...$$ patterns
pub struct LatexScanner;

impl InlineRule for LatexScanner {
    const MARKER: char = '$';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let input = &state.src[state.pos..];

        // Must start with $
        if !input.starts_with('$') {
            return None;
        }

        // Check for block math ($$)
        let is_block = input.starts_with("$$");
        let delimiter = if is_block { "$$" } else { "$" };
        let start = delimiter.len();

        // Find closing delimiter
        let end_pos = if is_block {
            // For block math, look for $$
            input[start..].find("$$")?
        } else {
            // For inline math, look for single $ (not followed by another $)
            let mut pos = start;
            let chars: Vec<char> = input.chars().collect();

            loop {
                if pos >= chars.len() {
                    return None;
                }

                if chars[pos] == '$' {
                    // Make sure it's not $$
                    if pos + 1 < chars.len() && chars[pos + 1] == '$' {
                        pos += 2; // Skip $$
                        continue;
                    }
                    // Found single $
                    break;
                }

                // Handle escape sequences
                if chars[pos] == '\\' && pos + 1 < chars.len() {
                    pos += 2; // Skip escaped character
                } else {
                    pos += 1;
                }
            }
            pos - start
        };

        let expression = &input[start..start + end_pos];

        // Validate expression
        if !is_valid_latex(expression) {
            return None;
        }

        let total_length = start + end_pos + delimiter.len();

        // Create LaTeX node
        let latex = LatexNode {
            expression: expression.to_string(),
            is_block,
            offset: state.pos,
        };

        let node = Node::new(latex);
        Some((node, total_length))
    }
}

/// Validate LaTeX expression
fn is_valid_latex(expr: &str) -> bool {
    // Empty expressions are not valid
    if expr.trim().is_empty() {
        return false;
    }

    // Check for balanced braces
    if !has_balanced_braces(expr) {
        return false;
    }

    // Check for dangerous commands (basic security check)
    if has_dangerous_commands(expr) {
        return false;
    }

    true
}

/// Check if braces are balanced
fn has_balanced_braces(expr: &str) -> bool {
    let mut depth = 0;
    let mut chars = expr.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                // Skip escaped character
                chars.next();
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }

    depth == 0
}

/// Check for dangerous LaTeX commands
fn has_dangerous_commands(expr: &str) -> bool {
    // List of commands that could be dangerous in LaTeX rendering
    const DANGEROUS: &[&str] = &[
        "\\input",
        "\\include",
        "\\write",
        "\\openout",
        "\\closeout",
        "\\loop",
        "\\def",
        "\\edef",
        "\\xdef",
        "\\gdef",
        "\\let",
        "\\futurelet",
        "\\newcommand",
        "\\renewcommand",
        "\\catcode",
    ];

    for cmd in DANGEROUS {
        if expr.contains(cmd) {
            return true;
        }
    }

    false
}

/// Add LaTeX plugin to markdown-it parser
pub fn add_latex_plugin(md: &mut MarkdownIt) {
    md.inline.add_rule::<LatexScanner>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_latex(input: &str) -> Vec<(String, bool)> {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        add_latex_plugin(&mut md);

        let ast = md.parse(input);
        let mut expressions = Vec::new();

        fn walk(node: &Node, expressions: &mut Vec<(String, bool)>) {
            if let Some(latex) = node.cast::<LatexNode>() {
                expressions.push((latex.expression.clone(), latex.is_block));
            }
            for child in &node.children {
                walk(child, expressions);
            }
        }

        walk(&ast, &mut expressions);
        expressions
    }

    #[test]
    fn test_inline_math() {
        let exprs = parse_latex("This has $x^2$ inline math");
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].0, "x^2");
        assert!(!exprs[0].1); // inline
    }

    #[test]
    fn test_block_math() {
        let exprs = parse_latex("Block math: $$E = mc^2$$");
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].0, "E = mc^2");
        assert!(exprs[0].1); // block
    }

    #[test]
    fn test_multiple_expressions() {
        let exprs = parse_latex("Inline $a + b$ and block $$\\int_0^1 x dx$$");
        assert_eq!(exprs.len(), 2);
        assert_eq!(exprs[0].0, "a + b");
        assert!(!exprs[0].1);
        assert_eq!(exprs[1].0, "\\int_0^1 x dx");
        assert!(exprs[1].1);
    }

    #[test]
    fn test_balanced_braces() {
        let exprs = parse_latex("Math with braces $\\frac{a}{b}$ here");
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].0, "\\frac{a}{b}");
    }

    #[test]
    fn test_unbalanced_braces() {
        // Should not match due to unbalanced braces
        let exprs = parse_latex("Invalid $\\frac{a}{b$ math");
        assert_eq!(exprs.len(), 0);
    }

    #[test]
    fn test_dangerous_command() {
        // Should not match due to dangerous command
        let exprs = parse_latex("Dangerous $\\input{file}$ command");
        assert_eq!(exprs.len(), 0);
    }

    #[test]
    fn test_empty_expression() {
        // Empty expressions should not match
        let exprs = parse_latex("Empty $$ expression");
        assert_eq!(exprs.len(), 0);
    }

    #[test]
    fn test_escaped_dollar() {
        // This is tricky - escaped $ should not end expression
        let exprs = parse_latex("With escaped $a \\$ b$ dollar");
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].0, "a \\$ b");
    }
}
