// Syntax highlighting for SurrealQL queries and commands

use nu_ansi_term::{Color, Style};
use reedline::{Highlighter, StyledText};
use std::collections::HashSet;

/// SurrealQL syntax highlighter
pub struct SurrealQLHighlighter {
    /// SQL keywords (highlighted in blue)
    keywords: HashSet<String>,

    /// Built-in functions (highlighted in magenta)
    functions: HashSet<String>,
}

impl SurrealQLHighlighter {
    pub fn new() -> Self {
        let keywords = Self::build_keyword_set();
        let functions = Self::build_function_set();

        Self {
            keywords,
            functions,
        }
    }

    /// Build set of SurrealQL keywords
    fn build_keyword_set() -> HashSet<String> {
        vec![
            // Query keywords
            "SELECT",
            "FROM",
            "WHERE",
            "ORDER",
            "BY",
            "LIMIT",
            "START",
            "FETCH",
            "GROUP",
            "SPLIT",
            "EXPLAIN",
            "TIMEOUT",
            // Data manipulation
            "CREATE",
            "UPDATE",
            "DELETE",
            "INSERT",
            "INTO",
            "RELATE",
            "SET",
            "UNSET",
            "MERGE",
            "PATCH",
            "CONTENT",
            "REPLACE",
            // Operators
            "AND",
            "OR",
            "NOT",
            "IN",
            "CONTAINS",
            "CONTAINSNOT",
            "CONTAINSALL",
            "CONTAINSANY",
            "CONTAINSNONE",
            "INSIDE",
            "OUTSIDE",
            "INTERSECTS",
            "IS",
            "NULL",
            "NONE",
            "EMPTY",
            // Data types
            "AS",
            "VALUE",
            "ONLY",
            // Control flow
            "IF",
            "ELSE",
            "THEN",
            "END",
            "RETURN",
            // Transaction
            "BEGIN",
            "TRANSACTION",
            "COMMIT",
            "CANCEL",
            // Database operations
            "USE",
            "NAMESPACE",
            "DATABASE",
            "TABLE",
            "INFO",
            "FOR",
            "DEFINE",
            "REMOVE",
            "FIELD",
            "INDEX",
            "UNIQUE",
            "EVENT",
            "ANALYZER",
            "TOKEN",
            "SCOPE",
            "PARAM",
            "FUNCTION",
            // Misc
            "LET",
            "LIVE",
            "KILL",
            "PARALLEL",
            "OMIT",
            "PERMISSIONS",
            "FULL",
            "FLEXIBLE",
            "READONLY",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Build set of SurrealQL built-in functions
    fn build_function_set() -> HashSet<String> {
        vec![
            // String functions
            "string::concat",
            "string::contains",
            "string::endsWith",
            "string::join",
            "string::length",
            "string::lowercase",
            "string::repeat",
            "string::replace",
            "string::reverse",
            "string::slice",
            "string::split",
            "string::startsWith",
            "string::trim",
            "string::uppercase",
            "string::words",
            // Array functions
            "array::add",
            "array::all",
            "array::any",
            "array::append",
            "array::combine",
            "array::concat",
            "array::difference",
            "array::distinct",
            "array::flatten",
            "array::group",
            "array::insert",
            "array::intersect",
            "array::len",
            "array::max",
            "array::min",
            "array::pop",
            "array::push",
            "array::remove",
            "array::reverse",
            "array::slice",
            "array::sort",
            "array::union",
            // Math functions
            "math::abs",
            "math::ceil",
            "math::floor",
            "math::round",
            "math::sqrt",
            "math::pow",
            "math::max",
            "math::min",
            // Time functions
            "time::now",
            "time::unix",
            "time::day",
            "time::hour",
            "time::minute",
            "time::month",
            "time::year",
            // Type functions
            "type::bool",
            "type::int",
            "type::float",
            "type::string",
            "type::array",
            "type::object",
            // Count/aggregation
            "count",
            "sum",
            "avg",
            "max",
            "min",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Determine if a word is a keyword
    fn is_keyword(&self, word: &str) -> bool {
        self.keywords.contains(&word.to_uppercase())
    }

    /// Determine if a word is a function
    fn is_function(&self, word: &str) -> bool {
        self.functions.contains(&word.to_lowercase())
    }

    /// Highlight a single token
    fn highlight_token(&self, token: &str) -> (Style, String) {
        // Command prefix (cyan)
        if token.starts_with(':') {
            return (Style::new().fg(Color::Cyan).bold(), token.to_string());
        }

        // Comments (dim gray)
        if token.starts_with("--") || token.starts_with("/*") {
            return (Style::new().fg(Color::DarkGray), token.to_string());
        }

        // String literals (green)
        if (token.starts_with('\'') && token.ends_with('\''))
            || (token.starts_with('"') && token.ends_with('"'))
        {
            return (Style::new().fg(Color::Green), token.to_string());
        }

        // Numbers (yellow)
        if token.parse::<f64>().is_ok() {
            return (Style::new().fg(Color::Yellow), token.to_string());
        }

        // Keywords (blue, bold)
        if self.is_keyword(token) {
            return (Style::new().fg(Color::Blue).bold(), token.to_uppercase());
        }

        // Functions (magenta)
        if self.is_function(token) || token.contains("::") {
            return (Style::new().fg(Color::Magenta), token.to_string());
        }

        // Operators (cyan)
        if matches!(
            token,
            "=" | "!=" | ">" | "<" | ">=" | "<=" | "+" | "-" | "*" | "/" | "%" | "~" | "?" | "@"
        ) {
            return (Style::new().fg(Color::Cyan), token.to_string());
        }

        // Record IDs (e.g., note:123) - yellow
        if token.contains(':') && !token.starts_with(':') {
            return (Style::new().fg(Color::Yellow), token.to_string());
        }

        // Default (white/default)
        (Style::default(), token.to_string())
    }
}

impl Default for SurrealQLHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter for SurrealQLHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();

        // Simple tokenization (split by whitespace and common delimiters)
        let mut current_token = String::new();
        let mut in_string = false;
        let mut string_char = ' ';
        let mut in_comment = false;

        for ch in line.chars() {
            match ch {
                '\'' | '"' if !in_comment => {
                    if in_string && ch == string_char {
                        // End of string
                        current_token.push(ch);
                        let (style, text) = self.highlight_token(&current_token);
                        styled.push((style, text));
                        current_token.clear();
                        in_string = false;
                    } else if !in_string {
                        // Start of string
                        if !current_token.is_empty() {
                            let (style, text) = self.highlight_token(&current_token);
                            styled.push((style, text));
                            current_token.clear();
                        }
                        in_string = true;
                        string_char = ch;
                        current_token.push(ch);
                    } else {
                        current_token.push(ch);
                    }
                }
                '-' if !in_string && current_token == "-" => {
                    // Start of comment
                    current_token.push(ch);
                    in_comment = true;
                }
                ' ' | '\t' | '\n' if !in_string && !in_comment => {
                    // Whitespace delimiter
                    if !current_token.is_empty() {
                        let (style, text) = self.highlight_token(&current_token);
                        styled.push((style, text));
                        current_token.clear();
                    }
                    styled.push((Style::default(), ch.to_string()));
                }
                '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' if !in_string && !in_comment => {
                    // Punctuation
                    if !current_token.is_empty() {
                        let (style, text) = self.highlight_token(&current_token);
                        styled.push((style, text));
                        current_token.clear();
                    }
                    styled.push((Style::new().fg(Color::White), ch.to_string()));
                }
                _ => {
                    current_token.push(ch);
                }
            }
        }

        // Flush remaining token
        if !current_token.is_empty() {
            let (style, text) = self.highlight_token(&current_token);
            styled.push((style, text));
        }

        styled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_recognition() {
        let highlighter = SurrealQLHighlighter::new();
        assert!(highlighter.is_keyword("SELECT"));
        assert!(highlighter.is_keyword("select"));
        assert!(highlighter.is_keyword("WHERE"));
        assert!(!highlighter.is_keyword("custom_field"));
    }

    #[test]
    fn test_function_recognition() {
        let highlighter = SurrealQLHighlighter::new();
        assert!(highlighter.is_function("string::concat"));
        assert!(highlighter.is_function("array::len"));
        assert!(highlighter.is_function("count"));
        assert!(!highlighter.is_function("my_function"));
    }

    #[test]
    fn test_command_highlighting() {
        let highlighter = SurrealQLHighlighter::new();
        let styled = highlighter.highlight(":tools", 0);

        // Should have cyan color for command
        let parts: Vec<_> = styled.buffer.iter().collect();
        assert!(!parts.is_empty());
    }

    #[test]
    fn test_query_highlighting() {
        let highlighter = SurrealQLHighlighter::new();
        let styled = highlighter.highlight("SELECT * FROM notes WHERE id = note:123", 0);

        // Should have multiple styled parts
        assert!(styled.buffer.len() > 5);
    }

    #[test]
    fn test_string_literal_highlighting() {
        let highlighter = SurrealQLHighlighter::new();
        let styled = highlighter.highlight("SELECT * FROM notes WHERE title = 'test'", 0);

        // Should have green style for string
        let parts: Vec<_> = styled.buffer.iter().collect();
        let has_string = parts.iter().any(|(style, text)| text.contains("'test'"));
        assert!(has_string);
    }
}
