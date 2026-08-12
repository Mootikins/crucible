use crate::formatting::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, ContentArrangement, Table,
};
use crucible_oil::truncate_to_chars;
use serde_json;
use std::io::IsTerminal;

/// A search hit with the display fields `cru search` renders.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResultWithScore {
    pub id: String,
    pub title: String,
    pub content: String,
    pub score: f64,
}

/// Detect if stdout is connected to an interactive terminal.
///
/// Returns `true` if stdout is a terminal (interactive), `false` if piped or redirected.
/// This is used to suppress ANSI colors and progress spinners when output is piped.
pub fn is_interactive() -> bool {
    std::io::stdout().is_terminal()
}

/// Lines of note content a `--preview` block shows, in either format.
const PREVIEW_MAX_LINES: usize = 2;
/// Char budget for the preview cell in the bordered table.
const PREVIEW_MAX_CHARS_TABLE: usize = 60;
/// Char budget for the free-flowing plain preview block. Matches the cap
/// `extract_snippet` already applies, so real snippets pass through untouched.
const PREVIEW_MAX_CHARS_PLAIN: usize = 200;

/// Format search results
pub fn format_search_results(
    results: &[SearchResultWithScore],
    format: OutputFormat,
    show_scores: bool,
    show_content: bool,
) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(results)?),
        OutputFormat::Table => Ok(format_as_table(results, show_scores, show_content)),
        OutputFormat::Plain => Ok(format_as_plain(results, show_scores, show_content)),
    }
}

fn format_as_plain(
    results: &[SearchResultWithScore],
    show_scores: bool,
    show_content: bool,
) -> String {
    let mut output = String::new();

    for (idx, result) in results.iter().enumerate() {
        output.push_str(&format!("{}. {}\n", idx + 1, result.title.bright_cyan()));

        if show_scores {
            output.push_str(&format!("   Score: {:.4}\n", result.score));
        }

        output.push_str(&format!("   Path: {}\n", result.id.dimmed()));

        if show_content {
            let preview = result
                .content
                .lines()
                .take(PREVIEW_MAX_LINES)
                .collect::<Vec<_>>()
                .join("\n   ");
            let preview = truncate_to_chars(&preview, PREVIEW_MAX_CHARS_PLAIN, true);
            output.push_str(&format!("   {}\n", preview.dimmed()));
        }

        output.push('\n');
    }

    output
}

fn format_as_table(
    results: &[SearchResultWithScore],
    show_scores: bool,
    show_content: bool,
) -> String {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    // Header
    let mut header = vec!["#", "Title", "Path"];
    if show_scores {
        header.push("Score");
    }
    if show_content {
        header.push("Preview");
    }
    table.set_header(header);

    // Rows
    for (idx, result) in results.iter().enumerate() {
        let mut row = vec![
            Cell::new(idx + 1),
            Cell::new(&result.title).fg(Color::Cyan),
            Cell::new(&result.id).fg(Color::DarkGrey),
        ];

        if show_scores {
            row.push(Cell::new(format!("{:.4}", result.score)));
        }

        if show_content {
            let preview: String = result
                .content
                .lines()
                .take(PREVIEW_MAX_LINES)
                .collect::<Vec<_>>()
                .join(" ");
            let truncated = truncate_to_chars(&preview, PREVIEW_MAX_CHARS_TABLE, true);
            row.push(Cell::new(truncated).fg(Color::DarkGrey));
        }

        table.add_row(row);
    }

    table.to_string()
}

/// Render a list of records as a bordered table, first column highlighted.
///
/// This body is `format_stats`', which *was* the `cru stats -f table` renderer
/// until `a644c2022` replaced its call site with a `println!` block and left the
/// function uncalled. It read as dead code and was nearly deleted as such; it is
/// in fact the missing implementation behind every `--format table` the CLI
/// advertised. Generalised over headers and rows so one helper serves each
/// list-shaped command rather than each growing its own `comfy_table` block.
pub fn records_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        // Wrap long cells instead of widening the table past the terminal. One
        // absolute kiln path in a Metric/Value table is enough to push it to a
        // hundred columns and wrap in the shell instead, which looks broken.
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(headers.iter().copied());

    for row in rows {
        table.add_row(row.iter().enumerate().map(|(column, value)| {
            let cell = Cell::new(value);
            // First column is the record's identity — the old Metric/Value
            // table coloured it, so keep that.
            if column == 0 {
                cell.fg(Color::Cyan)
            } else {
                cell
            }
        }));
    }

    table.to_string()
}

/// Print a formatted header
pub fn header(title: &str) {
    println!("\n{}", title.bold().underline());
    println!("{}", "─".repeat(title.len()));
}

/// Print an info message
pub fn info(message: &str) {
    println!("{} {}", "ℹ".blue(), message);
}

/// Print a success message
pub fn success(message: &str) {
    println!("{} {}", "✓".green(), message);
}

/// Print an error message
pub fn error(message: &str) {
    eprintln!("{} {}", "✗".red(), message);
}

/// Print a warning message
pub fn warning(message: &str) {
    println!("{} {}", "⚠".yellow(), message);
}

/// Print a hint/suggestion message
pub fn hint(message: &str) {
    eprintln!("  {} {}", "→".cyan(), message);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_sample_results() -> Vec<SearchResultWithScore> {
        vec![
            SearchResultWithScore {
                id: "test1.md".to_string(),
                title: "Test Note 1".to_string(),
                content: "This is test content".to_string(),
                score: 0.95,
            },
            SearchResultWithScore {
                id: "test2.md".to_string(),
                title: "Test Note 2".to_string(),
                content: "Another test content".to_string(),
                score: 0.85,
            },
        ]
    }

    #[test]
    fn test_format_plain_without_scores() {
        let results = create_sample_results();
        let output = format_search_results(&results, OutputFormat::Plain, false, false).unwrap();

        assert!(output.contains("Test Note 1"));
        assert!(output.contains("test1.md"));
        assert!(!output.contains("0.95")); // Score should not be shown
    }

    #[test]
    fn test_format_plain_with_scores() {
        let results = create_sample_results();
        let output = format_search_results(&results, OutputFormat::Plain, true, false).unwrap();

        assert!(output.contains("Test Note 1"));
        assert!(output.contains("0.95"));
        assert!(output.contains("0.85"));
    }

    #[test]
    fn test_format_plain_with_content() {
        let results = create_sample_results();
        let output = format_search_results(&results, OutputFormat::Plain, false, true).unwrap();

        assert!(output.contains("Test Note 1"));
        assert!(output.contains("This is test content"));
    }

    #[test]
    fn test_format_json() {
        let results = create_sample_results();
        let output = format_search_results(&results, OutputFormat::Json, false, false).unwrap();

        // Verify it's valid JSON
        let parsed: Vec<SearchResultWithScore> = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].title, "Test Note 1");
        assert_eq!(parsed[1].title, "Test Note 2");
    }

    #[test]
    fn test_format_table() {
        let results = create_sample_results();
        let output = format_search_results(&results, OutputFormat::Table, false, false).unwrap();

        assert!(output.contains("Test Note 1"));
        assert!(output.contains("Test Note 2"));
        // Table should contain borders
        assert!(output.contains("─"));
    }

    #[test]
    fn test_format_empty_results() {
        let results: Vec<SearchResultWithScore> = vec![];
        let output = format_search_results(&results, OutputFormat::Plain, false, false).unwrap();

        assert_eq!(output, "");
    }

    #[test]
    fn test_format_with_long_content() {
        let results = vec![SearchResultWithScore {
            id: "long.md".to_string(),
            title: "Long Content".to_string(),
            content: "a".repeat(200), // Very long content
            score: 0.9,
        }];

        let output = format_search_results(&results, OutputFormat::Table, false, true).unwrap();

        // Truncated with the single-character ellipsis, one char of the budget spent on it.
        assert!(output.contains(&format!(
            "{}{}",
            "a".repeat(PREVIEW_MAX_CHARS_TABLE - 1),
            '\u{2026}'
        )));
    }

    #[test]
    fn search_preview_truncates_on_a_char_boundary() {
        // Byte 60 lands inside the twentieth 日 (bytes 58..61), which is what the
        // old `&preview[..60]` slice aborted on.
        let results = vec![SearchResultWithScore {
            id: "cjk.md".to_string(),
            title: "CJK".to_string(),
            content: format!("x{}", "\u{65E5}".repeat(70)),
            score: 0.9,
        }];

        let output = format_search_results(&results, OutputFormat::Table, false, true).unwrap();

        assert!(output.contains('x'));
        assert!(output.contains('\u{2026}'));
        assert!(!output.contains('\u{FFFD}'));
        // 'x' plus whole 日 characters, ellipsis included, inside the budget.
        assert_eq!(
            output.chars().filter(|c| *c == '\u{65E5}').count(),
            PREVIEW_MAX_CHARS_TABLE - 2
        );
    }

    #[test]
    fn search_preview_truncates_emoji_on_a_char_boundary() {
        // 4-byte sequences: byte 60 lands mid-emoji.
        let results = vec![SearchResultWithScore {
            id: "emoji.md".to_string(),
            title: "Emoji".to_string(),
            content: format!("ab{}", "\u{1F525}".repeat(70)),
            score: 0.9,
        }];

        let output = format_search_results(&results, OutputFormat::Table, false, true).unwrap();

        assert!(output.contains("ab"));
        assert!(output.contains('\u{2026}'));
        assert!(!output.contains('\u{FFFD}'));
        assert_eq!(
            output.chars().filter(|c| *c == '\u{1F525}').count(),
            PREVIEW_MAX_CHARS_TABLE - 3
        );
    }

    #[test]
    fn plain_preview_caps_a_pathologically_long_line() {
        let results = vec![SearchResultWithScore {
            id: "long.md".to_string(),
            title: "Long".to_string(),
            content: "\u{65E5}".repeat(4000),
            score: 0.9,
        }];

        let output = format_search_results(&results, OutputFormat::Plain, false, true).unwrap();

        let preview_chars = output.chars().filter(|c| *c == '\u{65E5}').count();
        assert_eq!(preview_chars, PREVIEW_MAX_CHARS_PLAIN - 1);
        assert!(output.contains('\u{2026}'));
    }

    #[test]
    fn records_table_renders_every_header_and_cell() {
        let rows = vec![
            vec!["total_files".to_string(), "42".to_string()],
            vec!["indexed_files".to_string(), "40".to_string()],
        ];

        let output = records_table(&["Metric", "Value"], &rows);

        for expected in [
            "Metric",
            "Value",
            "total_files",
            "42",
            "indexed_files",
            "40",
        ] {
            assert!(output.contains(expected), "missing {expected}:\n{output}");
        }
        assert!(output.contains('─'), "no table border:\n{output}");
    }

    #[test]
    fn records_table_of_no_rows_is_still_a_table() {
        // `cru stats` on an empty kiln, `cru tools list` with nothing installed:
        // the header is the answer, and a bare border beats a panic.
        let output = records_table(&["Name", "Scope"], &[]);

        assert!(output.contains("Name"));
        assert!(output.contains("Scope"));
    }

    #[test]
    fn records_table_tolerates_a_short_row() {
        // Rows are built per command; a ragged one should render, not panic.
        let output = records_table(&["A", "B", "C"], &[vec!["only".to_string()]]);

        assert!(output.contains("only"));
    }

    #[test]
    fn test_colored_override_removes_ansi() {
        // Force colors ON first (tests run without a terminal, so colored defaults to no ANSI)
        colored::control::set_override(true);
        let test_string = "test".red().to_string();

        // With override(true), should contain ANSI escape codes
        assert!(test_string.contains("\x1b["));

        // Apply override to disable
        colored::control::set_override(false);
        let overridden_string = "test".red().to_string();

        // After override(false), should NOT contain ANSI escape codes
        assert!(!overridden_string.contains("\x1b["));
        assert_eq!(overridden_string, "test");

        // Reset — unset the override to restore automatic terminal detection
        colored::control::unset_override();
    }
}
