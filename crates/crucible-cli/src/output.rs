use crate::interactive::SearchResultWithScore;
use anyhow::Result;
use colored::Colorize;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use serde_json;

/// Format search results
pub fn format_search_results(
    results: &[SearchResultWithScore],
    format: &str,
    show_scores: bool,
    show_content: bool,
) -> Result<String> {
    match format {
        "json" => Ok(serde_json::to_string_pretty(results)?),
        "table" => Ok(format_as_table(results, show_scores, show_content)),
        _ => Ok(format_as_plain(results, show_scores, show_content)),
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
                .take(3)
                .collect::<Vec<_>>()
                .join("\n   ");
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
            let preview: String = result.content.lines().take(2).collect::<Vec<_>>().join(" ");
            let truncated = if preview.len() > 60 {
                format!("{}...", &preview[..60])
            } else {
                preview
            };
            row.push(Cell::new(truncated).fg(Color::DarkGrey));
        }

        table.add_row(row);
    }

    table.to_string()
}

/// Format file list
pub fn format_file_list(files: &[String], format: &str) -> Result<String> {
    match format {
        "json" => Ok(serde_json::to_string_pretty(files)?),
        "table" => {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec!["#", "Path"]);

            for (idx, file) in files.iter().enumerate() {
                table.add_row(vec![Cell::new(idx + 1), Cell::new(file)]);
            }

            Ok(table.to_string())
        }
        _ => Ok(files.join("\n")),
    }
}

/// Format statistics
pub fn format_stats(stats: &std::collections::HashMap<String, i64>) -> String {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);
    table.set_header(vec!["Metric", "Value"]);

    for (key, value) in stats {
        table.add_row(vec![Cell::new(key).fg(Color::Cyan), Cell::new(value)]);
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

/// Show a warning about degraded functionality in lightweight storage mode
///
/// Displays a yellow warning message to stderr that clearly indicates the feature
/// requires full storage mode (SurrealDB). The message always includes "storage"
/// to clarify the cause.
///
/// # Example
///
/// ```rust
/// use crucible_cli::output::storage_warning;
///
/// // When lightweight mode can't fulfill a request
/// storage_warning("SQL queries");
/// // Prints: ⚠ SQL queries requires full storage mode
/// ```
pub fn storage_warning(feature: &str) {
    eprintln!("{} {} requires full storage mode", "⚠".yellow(), feature);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
        let output = format_search_results(&results, "plain", false, false).unwrap();

        assert!(output.contains("Test Note 1"));
        assert!(output.contains("test1.md"));
        assert!(!output.contains("0.95")); // Score should not be shown
    }

    #[test]
    fn test_format_plain_with_scores() {
        let results = create_sample_results();
        let output = format_search_results(&results, "plain", true, false).unwrap();

        assert!(output.contains("Test Note 1"));
        assert!(output.contains("0.95"));
        assert!(output.contains("0.85"));
    }

    #[test]
    fn test_format_plain_with_content() {
        let results = create_sample_results();
        let output = format_search_results(&results, "plain", false, true).unwrap();

        assert!(output.contains("Test Note 1"));
        assert!(output.contains("This is test content"));
    }

    #[test]
    fn test_format_json() {
        let results = create_sample_results();
        let output = format_search_results(&results, "json", false, false).unwrap();

        // Verify it's valid JSON
        let parsed: Vec<SearchResultWithScore> = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].title, "Test Note 1");
        assert_eq!(parsed[1].title, "Test Note 2");
    }

    #[test]
    fn test_format_table() {
        let results = create_sample_results();
        let output = format_search_results(&results, "table", false, false).unwrap();

        assert!(output.contains("Test Note 1"));
        assert!(output.contains("Test Note 2"));
        // Table should contain borders
        assert!(output.contains("─"));
    }

    #[test]
    fn test_format_empty_results() {
        let results: Vec<SearchResultWithScore> = vec![];
        let output = format_search_results(&results, "plain", false, false).unwrap();

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

        let output = format_search_results(&results, "table", false, true).unwrap();

        // Should be truncated
        assert!(output.contains("..."));
    }

    #[test]
    fn test_format_file_list_plain() {
        let files = vec!["file1.md".to_string(), "file2.md".to_string()];

        let output = format_file_list(&files, "plain").unwrap();

        assert!(output.contains("file1.md"));
        assert!(output.contains("file2.md"));
        assert_eq!(output, "file1.md\nfile2.md");
    }

    #[test]
    fn test_format_file_list_json() {
        let files = vec!["test.md".to_string()];
        let output = format_file_list(&files, "json").unwrap();

        let parsed: Vec<String> = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], "test.md");
    }

    #[test]
    fn test_format_file_list_table() {
        let files = vec!["file1.md".to_string(), "file2.md".to_string()];

        let output = format_file_list(&files, "table").unwrap();

        assert!(output.contains("file1.md"));
        assert!(output.contains("file2.md"));
        assert!(output.contains("─")); // Table border
    }

    #[test]
    fn test_format_stats() {
        let mut stats = HashMap::new();
        stats.insert("total_files".to_string(), 42);
        stats.insert("indexed_files".to_string(), 40);

        let output = format_stats(&stats);

        assert!(output.contains("total_files"));
        assert!(output.contains("42"));
        assert!(output.contains("─")); // Table border
    }
}
