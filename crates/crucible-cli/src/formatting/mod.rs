// Shared formatting utilities for CLI output
//
// This module provides common formatting functions and types to eliminate
// duplication across command implementations, following the DRY principle.

mod markdown_renderer;
pub use markdown_renderer::render_markdown;

pub mod syntax;
pub mod syntax_theme;
pub use syntax::SyntaxHighlighter;

/// Output format for commands whose payload is a list of records.
///
/// A `ValueEnum` rather than a `String`, because clap then derives the help
/// text and the accepted set from this one declaration and the two cannot drift
/// apart. The predecessor was a `from_str` documented as "infallible parsing
/// with default" that mapped everything unrecognised to `Plain` — which is how
/// `csv` came to be advertised on commands that have no CSV writer. Nothing
/// rejected it, so nothing revealed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Bordered table, for reading.
    #[default]
    Table,
    /// JSON, for scripting.
    Json,
    /// Unadorned lines, for piping.
    #[value(alias = "text")]
    Plain,
}

/// Output format for commands whose payload has no tabular shape — nested
/// config, a step tree, a status report.
///
/// Separate from [`OutputFormat`] so `table` is not offered where it could only
/// ever be a synonym for the human-readable rendering. It stays accepted as an
/// alias, because it was the documented default on these commands and scripts
/// pass it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum TextFormat {
    /// Human-readable report.
    #[default]
    #[value(alias = "table", alias = "plain")]
    Text,
    /// JSON, for scripting.
    Json,
}

/// `default_value_t` needs `Display`, and the only spelling that must not drift
/// from what clap accepts is the one clap itself derived. So ask it.
macro_rules! display_via_possible_value {
    ($ty:ty) => {
        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                clap::ValueEnum::to_possible_value(self)
                    .expect("no variant is #[value(skip)]")
                    .get_name()
                    .fmt(f)
            }
        }
    };
}

display_via_possible_value!(OutputFormat);
display_via_possible_value!(TextFormat);

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    /// The values clap accepts are the values the enum declares. This is the
    /// invariant the old `from_str` could not hold: it accepted every string.
    #[test]
    fn output_format_accepts_exactly_its_variants() {
        for (input, expected) in [
            ("table", OutputFormat::Table),
            ("json", OutputFormat::Json),
            ("plain", OutputFormat::Plain),
            ("text", OutputFormat::Plain),
        ] {
            assert_eq!(
                OutputFormat::from_str(input, true).ok(),
                Some(expected),
                "{input}"
            );
        }
        for rejected in ["csv", "detailed", "binary", "xyzzy", ""] {
            assert!(
                OutputFormat::from_str(rejected, true).is_err(),
                "`{rejected}` must be rejected, not silently downgraded"
            );
        }
    }

    #[test]
    fn text_format_keeps_table_and_plain_as_aliases() {
        for input in ["text", "table", "plain"] {
            assert_eq!(
                TextFormat::from_str(input, true).ok(),
                Some(TextFormat::Text),
                "{input}"
            );
        }
        assert_eq!(
            TextFormat::from_str("json", true).ok(),
            Some(TextFormat::Json)
        );
        assert!(TextFormat::from_str("csv", true).is_err());
    }
}
