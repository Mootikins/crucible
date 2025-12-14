//! GBNF grammar loading and manipulation

use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GrammarError {
    #[error("Failed to read grammar file: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid grammar: {0}")]
    Invalid(String),
}

pub type GrammarResult<T> = Result<T, GrammarError>;

/// A GBNF grammar for constraining LLM output
#[derive(Debug, Clone)]
pub struct Grammar {
    /// Raw GBNF content
    pub content: String,
    /// Optional name/description
    pub name: Option<String>,
}

impl Grammar {
    /// Create a grammar from a string
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            name: None,
        }
    }

    /// Create a grammar with a name
    pub fn named(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            name: Some(name.into()),
        }
    }

    /// Load a grammar from a file
    pub fn from_file(path: impl AsRef<Path>) -> GrammarResult<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        let name = path
            .as_ref()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        Ok(Self { content, name })
    }

    /// Get the grammar content as a string slice
    pub fn as_str(&self) -> &str {
        &self.content
    }
}

/// Pre-built grammars for common tool-calling patterns
pub mod presets {
    use super::Grammar;

    /// Grammar for simple function-call syntax: tool(param="value")
    pub fn simple_tool_call() -> Grammar {
        Grammar::named(
            "simple_tool_call",
            r#"root ::= tool
tool ::= name "(" params? ")"
name ::= [a-z_]+
params ::= param ("," ws param)*
param ::= ident ws "=" ws value
ident ::= [a-z_]+
value ::= string | number | bool
string ::= "\"" chars "\""
chars ::= [^"\\]*
number ::= "-"? [0-9]+ ("." [0-9]+)?
bool ::= "true" | "false"
ws ::= [ \t]*"#,
        )
    }

    /// Grammar for L0+L1 tools: read, write, edit, ls, git, rg
    pub fn l0_l1_tools() -> Grammar {
        Grammar::named(
            "l0_l1_tools",
            r#"root ::= tool

tool ::= read | write | edit | ls | git | rg

read ::= "read(path=\"" path "\"" read-opts? ")"
read-opts ::= ", offset=" number ", length=" number

write ::= "write(path=\"" path "\", content=\"" content "\")"

edit ::= "edit(path=\"" path "\", search=\"" text "\", replace=\"" text "\")"

ls ::= "ls(path=\"" path "\"" ls-opts? ")"
ls-opts ::= ", depth=" number

git ::= "git(args=\"" git-args "\")"
git-args ::= [a-zA-Z0-9_ ./-]+

rg ::= "rg(pattern=\"" pattern "\"" rg-opts? ")"
rg-opts ::= ", path=\"" path "\""

path ::= [a-zA-Z0-9_./-]+
content ::= [^"]*
text ::= [^"]*
pattern ::= [^"]+
number ::= [0-9]+"#,
        )
    }

    /// Grammar for L0+L1 tools with optional thinking block
    /// Allows: <think>reasoning here</think>tool(...) OR just tool(...)
    pub fn l0_l1_tools_with_thinking() -> Grammar {
        Grammar::named(
            "l0_l1_tools_with_thinking",
            r#"root ::= thinking? tool

# Optional thinking block - captures model reasoning
thinking ::= "<think>" think-content "</think>" ws
think-content ::= think-char*
think-char ::= [^<] | "<" [^/] | "</" [^t] | "</t" [^h] | "</th" [^i] | "</thi" [^n] | "</thin" [^k] | "</think" [^>]

tool ::= read | write | edit | ls | git | rg

read ::= "read(path=\"" path "\"" read-opts? ")"
read-opts ::= ", offset=" number ", length=" number

write ::= "write(path=\"" path "\", content=\"" content "\")"

edit ::= "edit(path=\"" path "\", search=\"" text "\", replace=\"" text "\")"

ls ::= "ls(path=\"" path "\"" ls-opts? ")"
ls-opts ::= ", depth=" number

git ::= "git(args=\"" git-args "\")"
git-args ::= [a-zA-Z0-9_ ./-]+

rg ::= "rg(pattern=\"" pattern "\"" rg-opts? ")"
rg-opts ::= ", path=\"" path "\""

path ::= [a-zA-Z0-9_./-]+
content ::= [^"]*
text ::= [^"]*
pattern ::= [^"]+
number ::= [0-9]+
ws ::= [ \t\n]*"#,
        )
    }

    /// Grammar that allows either a tool call OR prose response
    pub fn tool_or_prose() -> Grammar {
        Grammar::named(
            "tool_or_prose",
            r#"root ::= tool | prose

tool ::= name "(" params? ")"
name ::= "read" | "write" | "edit" | "ls" | "git" | "rg"
params ::= param ("," ws param)*
param ::= ident ws "=" ws value
ident ::= [a-z_]+
value ::= "\"" [^"]* "\""
ws ::= [ \t]*

prose ::= "PROSE:" [^\n]+"#,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grammar_new() {
        let g = Grammar::new("root ::= \"hello\"");
        assert!(g.content.contains("hello"));
        assert!(g.name.is_none());
    }

    #[test]
    fn test_grammar_named() {
        let g = Grammar::named("test", "root ::= \"hi\"");
        assert_eq!(g.name, Some("test".to_string()));
    }

    #[test]
    fn test_preset_simple_tool_call() {
        let g = presets::simple_tool_call();
        assert!(g.content.contains("tool"));
        assert!(g.content.contains("param"));
    }

    #[test]
    fn test_preset_l0_l1_tools() {
        let g = presets::l0_l1_tools();
        assert!(g.content.contains("read"));
        assert!(g.content.contains("write"));
        assert!(g.content.contains("git"));
        assert!(g.content.contains("rg"));
    }
}
