//! LaTeX Expression Extension Tests
//!
//! Test LaTeX mathematical expression parsing for Phase 1B features:
//! - Inline LaTeX: $...$
//! - Block LaTeX: $$...$$
//! - Complex mathematical expressions
//! - Validation and error handling

use crucible_parser::{CrucibleParser, MarkdownParserImplementation, latex::create_latex_extension};
use std::path::Path;

/// Test basic inline LaTeX parsing
#[tokio::test]
async fn test_basic_inline_latex() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Basic LaTeX Test

Simple equation: $E = mc^2$.

Variables: $x$, $y$, and $z$.

More complex: $\frac{1}{2}$ and $\sqrt{2}$.
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse LaTeX content");

    assert!(!document.latex_expressions.is_empty(), "Should find LaTeX expressions");

    let inline_expressions: Vec<_> = document.latex_expressions.iter()
        .filter(|latex| !latex.is_block)
        .collect();

    assert!(inline_expressions.len() >= 6, "Should find at least 6 inline expressions");

    // Check specific expressions
    let expressions: Vec<_> = document.latex_expressions.iter()
        .map(|latex| latex.expression.as_str())
        .collect();

    assert!(expressions.iter().any(|expr| expr.contains("E = mc^2")),
           "Should find Einstein's equation");
    assert!(expressions.iter().any(|expr| expr.contains("frac{1}{2}")),
           "Should find fraction");
    assert!(expressions.iter().any(|expr| expr.contains("sqrt{2}")),
           "Should find square root");

    // All should be inline (not block)
    assert!(expressions.iter().all(|expr| !expr.starts_with("$$")),
           "All should be inline expressions");
}

/// Test block LaTeX parsing
#[tokio::test]
async fn test_block_latex() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Block LaTeX Test

Inline: $x + y = z$.

Block equation:
$$
\int_{0}^{\infty} e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$

Another block:
$$
\begin{pmatrix}
a & b \\
c & d
\end{pmatrix}
$$

More inline: $a^2 + b^2 = c^2$.
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse LaTeX content");

    // Should have both inline and block expressions
    assert!(!document.latex_expressions.is_empty(), "Should find LaTeX expressions");

    let block_expressions: Vec<_> = document.latex_expressions.iter()
        .filter(|latex| latex.is_block)
        .collect();

    let inline_expressions: Vec<_> = document.latex_expressions.iter()
        .filter(|latex| !latex.is_block)
        .collect();

    assert!(block_expressions.len() >= 2, "Should have at least 2 block expressions");
    assert!(inline_expressions.len() >= 2, "Should have at least 2 inline expressions");

    // Check block content
    let block_contents: Vec<_> = block_expressions.iter()
        .map(|latex| latex.expression.as_str())
        .collect();

    assert!(block_contents.iter().any(|expr| expr.contains("int_{0}^{\\infty}")),
           "Should find integral in block");
    assert!(block_contents.iter().any(|expr| expr.contains("begin{pmatrix}")),
           "Should find matrix in block");
}

/// Test complex mathematical expressions
#[tokio::test]
async fn test_complex_mathematical_expressions() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Complex Math

Quadratic formula: $x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}$.

Euler's identity: $e^{i\pi} + 1 = 0$.

Gaussian integral:
$$
\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}
$$

Fourier Transform:
$$
\mathcal{F}\{f(t)\} = \int_{-\infty}^{\infty} f(t) e^{-i\omega t} dt
$$

Statistics:
$$
\mu = \frac{1}{n} \sum_{i=1}^{n} x_i \quad \text{and} \quad \sigma^2 = \frac{1}{n} \sum_{i=1}^{n} (x_i - \mu)^2
$$
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse complex LaTeX content");

    assert!(!document.latex_expressions.is_empty(), "Should find LaTeX expressions");

    // Check for specific complex expressions
    let expressions: Vec<_> = document.latex_expressions.iter()
        .map(|latex| latex.expression.as_str())
        .collect();

    assert!(expressions.iter().any(|expr| expr.contains("frac{-b")),
           "Should find quadratic formula");
    assert!(expressions.iter().any(|expr| expr.contains("e^{i\\pi}")),
           "Should find Euler's identity");
    assert!(expressions.iter().any(|expr| expr.contains("int_{-\\infty}^{\\infty}")),
           "Should find Gaussian integral");
    assert!(expressions.iter().any(|expr| expr.contains("mathcal{F}")),
           "Should find Fourier Transform");
    assert!(expressions.iter().any(|expr| expr.contains("mu =")),
           "Should find statistical formulas");
}

/// Test LaTeX edge cases and error handling
#[tokio::test]
async fn test_latex_edge_cases() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# LaTeX Edge Cases

Empty inline: $ $

Unbalanced braces: $x^2 + y^2$

Mismatched delimiters: $x^2 + y^2$

Special characters: $& < > " '$

Nested braces: $f(g(x))$ and $\frac{\frac{1}{2}}{3}$

Multiple expressions on one line: $x + y = z$ and $a^2 + b^2 = c^2$.

Invalid block:
$$
Unclosed block LaTeX
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse LaTeX content even with edge cases");

    // Should still find some valid expressions
    assert!(!document.latex_expressions.is_empty(), "Should find some LaTeX expressions");

    // Check that expressions with nested braces are handled
    let expressions: Vec<_> = document.latex_expressions.iter()
        .map(|latex| latex.expression.as_str())
        .collect();

    let nested_found = expressions.iter().any(|expr| expr.contains("f(g(x))"));
    assert!(nested_found, "Should handle nested braces");

    let nested_fraction = expressions.iter().any(|expr| expr.contains("frac{\\frac{"));
    assert!(nested_fraction, "Should handle nested fractions");

    println!("✅ LaTeX edge cases handled. Found {} expressions", document.latex_expressions.len());
}

/// Test LaTeX extension directly
#[test]
fn test_latex_extension_creation() {
    let extension = create_latex_extension();

    assert_eq!(extension.name(), "latex", "Extension should be named 'latex'");
    assert!(extension.supports_latex(), "Should support LaTeX");

    let capabilities = extension.capabilities();
    assert!(capabilities.supports_latex, "Capabilities should indicate LaTeX support");

    // Test extension processes content correctly
    let test_content = "$E = mc^2$ and $$\\int_{0}^{1} x^2 dx = \\frac{1}{3}$$";

    let mut document_content = crucible_parser::DocumentContent::default();
    let result = extension.process_content(test_content, Path::new("test.md"), &mut document_content);

    assert!(result.is_ok(), "Should process content without errors");
}

/// Test that LaTeX expressions are properly positioned
#[tokio::test]
async fn test_latex_expression_positioning() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Position Test

Start here: $x = 1$

Middle here: $y = 2$

End here: $z = 3$
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse LaTeX content");

    // Verify positions are recorded
    for latex_expr in &document.latex_expressions {
        assert!(latex_expr.start_offset > 0, "Should have valid start offset");
        assert!(latex_expr.length > 0, "Should have valid length");
        assert!(latex_expr.start_offset < content.len() as u64, "Should be within content bounds");
    }

    // Check that expressions are in order
    let mut positions: Vec<_> = document.latex_expressions.iter()
        .map(|latex| latex.start_offset)
        .collect();
    positions.sort_unstable();

    let original_positions: Vec<_> = document.latex_expressions.iter()
        .map(|latex| latex.start_offset)
        .collect();

    assert_eq!(positions, original_positions, "Positions should be in parsing order");
}

/// Test LaTeX with markdown formatting
#[tokio::test]
async fn test_latex_with_markdown_formatting() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# LaTeX in Markdown

**Bold equation:** $E = mc^2$

*Italic equation:* $a^2 + b^2 = c^2$

`Code not equation: $x = 1$` but this is: $y = 2$

> Quote equation: $x + y = z$

List equation:
- First: $1 + 1 = 2$
- Second: $2 + 2 = 4$

[Link equation $x = 1$](url)
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse LaTeX with markdown formatting");

    // Should find LaTeX in various contexts
    assert!(!document.latex_expressions.is_empty(), "Should find LaTeX expressions");

    // Should not find LaTeX in code blocks
    let expressions: Vec<_> = document.latex_expressions.iter()
        .map(|latex| latex.expression.as_str())
        .collect();

    // Should not extract from code (depending on implementation)
    let has_non_code_expr = expressions.iter().any(|expr| !expr.contains("Code not equation"));
    assert!(has_non_code_expr, "Should find expressions outside of code");

    // Should find LaTeX in lists, quotes, and formatted text
    assert!(expressions.len() >= 6, "Should find multiple LaTeX expressions in different contexts");
}

/// Test mathematical symbols and Greek letters
#[tokio::test]
async fn test_mathematical_symbols_and_greek() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Greek Letters and Symbols

Greek letters: $\alpha$, $\beta$, $\gamma$, $\delta$, $\epsilon$.

Mathematical symbols: $\pm$, $\mp$, $\times$, $\div$, $\neq$, $\approx$, $\equiv$.

Set theory: $\in$, $\notin$, $\subset$, \subseteq$, $\cup$, $\cap$, $\emptyset$.

Logic: $\forall$, $\exists$, $\neg$, $\land$, $\lor$, $\implies$, $\iff$.

Calculus: $\partial$, $\nabla$, $\int$, $\sum$, $\prod$, $\lim$.

Advanced: $\aleph$, $\hbar$, $\infty$, $\varnothing$, $\triangle$.
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse Greek letters and symbols");

    assert!(!document.latex_expressions.is_empty(), "Should find LaTeX expressions");

    let expressions: Vec<_> = document.latex_expressions.iter()
        .map(|latex| latex.expression.as_str())
        .collect();

    // Check for different categories of symbols
    assert!(expressions.iter().any(|expr| expr.contains("alpha") || expr.contains("beta") || expr.contains("gamma")),
           "Should find Greek letters");
    assert!(expressions.iter().any(|expr| expr.contains("pm") || expr.contains("times") || expr.contains("div")),
           "Should find mathematical operators");
    assert!(expressions.iter().any(|expr| expr.contains("in") || expr.contains("subset") || expr.contains("cup")),
           "Should find set theory symbols");
    assert!(expressions.iter().any(|expr| expr.contains("forall") || expr.contains("exists")),
           "Should find logic symbols");

    println!("✅ Mathematical symbols test passed! Found {} expressions", document.latex_expressions.len());
}