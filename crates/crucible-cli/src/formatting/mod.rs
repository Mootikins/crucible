// Shared formatting utilities for CLI output
//
// This module provides common formatting functions and types to eliminate
// duplication across command implementations, following the DRY principle.

use anyhow::Result;
use serde::Serialize;
use serde_json;
use tabled::{settings::Style, Table, Tabled};

mod markdown_renderer;
pub use markdown_renderer::render_markdown;

mod syntax;
pub use syntax::{HighlightedLine, HighlightedSpan, SyntaxHighlighter};

/// Standard output format types supported across all commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Plain text output
    Plain,
    /// JSON output for programmatic consumption
    Json,
    /// Human-readable table format
    Table,
    /// CSV format for data export
    Csv,
    /// Detailed/verbose output
    Detailed,
}

impl OutputFormat {
    /// Parse format from string
    #[allow(clippy::should_implement_trait)] // Infallible parsing with default, not FromStr semantics
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => OutputFormat::Json,
            "table" => OutputFormat::Table,
            "csv" => OutputFormat::Csv,
            "detailed" | "detail" => OutputFormat::Detailed,
            _ => OutputFormat::Plain,
        }
    }

    /// Check if format is machine-readable
    pub fn is_machine_readable(&self) -> bool {
        matches!(self, OutputFormat::Json | OutputFormat::Csv)
    }
}

impl From<String> for OutputFormat {
    fn from(s: String) -> Self {
        Self::from_str(&s)
    }
}

impl From<&str> for OutputFormat {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

/// Format bytes into human-readable format (B, KB, MB, GB, TB)
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    const THRESHOLD: f64 = 1024.0;

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size /= THRESHOLD;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Format duration in a human-readable way
pub fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    let millis = duration.as_millis();

    if secs > 60 {
        let mins = secs / 60;
        let rem_secs = secs % 60;
        format!("{}m {}s", mins, rem_secs)
    } else if secs > 0 {
        format!("{:.2}s", duration.as_secs_f64())
    } else if millis > 0 {
        format!("{}ms", millis)
    } else {
        format!("{}μs", duration.as_micros())
    }
}

/// Format a timestamp as RFC3339
pub fn format_timestamp(timestamp: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;

    let duration_since_epoch = timestamp.duration_since(UNIX_EPOCH).unwrap_or_default();

    // Simple RFC3339-like format
    let secs = duration_since_epoch.as_secs();
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Truncate a string to a maximum length, adding ellipsis if needed
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Get a preview of text content (first N lines or characters)
pub fn get_text_preview(text: &str, max_lines: usize, max_chars: usize) -> String {
    let lines: Vec<&str> = text.lines().take(max_lines).collect();
    let preview = lines.join(" ");
    truncate_string(&preview, max_chars)
}

/// Get a preview of binary data
pub fn get_block_preview(data: &[u8], max_chars: usize) -> String {
    let preview = String::from_utf8_lossy(data);
    get_text_preview(&preview, 3, max_chars)
}

/// Format a percentage
pub fn format_percentage(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

/// Format a ratio as a human-readable string
pub fn format_ratio(numerator: u64, denominator: u64) -> String {
    if denominator == 0 {
        "N/A".to_string()
    } else {
        let ratio = numerator as f64 / denominator as f64;
        format!("{:.2}x", ratio)
    }
}

/// Generic table renderer for any Tabled struct
pub fn render_table<T: Tabled>(rows: &[T]) -> String {
    Table::new(rows).with(Style::modern()).to_string()
}

/// Generic JSON renderer for any serializable type
pub fn render_json<T: Serialize>(data: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(data)?)
}

/// Generic JSON renderer with compact output
pub fn render_json_compact<T: Serialize>(data: &T) -> Result<String> {
    Ok(serde_json::to_string(data)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(100), "100 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(1536 * 1024 * 1024), "1.5 GB");
    }

    #[test]
    fn test_output_format_from_str() {
        assert_eq!(OutputFormat::from_str("json"), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str("JSON"), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str("table"), OutputFormat::Table);
        assert_eq!(OutputFormat::from_str("csv"), OutputFormat::Csv);
        assert_eq!(OutputFormat::from_str("detailed"), OutputFormat::Detailed);
        assert_eq!(OutputFormat::from_str("unknown"), OutputFormat::Plain);
        assert_eq!(OutputFormat::from_str(""), OutputFormat::Plain);
    }

    #[test]
    fn test_is_machine_readable() {
        assert!(OutputFormat::Json.is_machine_readable());
        assert!(OutputFormat::Csv.is_machine_readable());
        assert!(!OutputFormat::Plain.is_machine_readable());
        assert!(!OutputFormat::Table.is_machine_readable());
        assert!(!OutputFormat::Detailed.is_machine_readable());
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("hello", 10), "hello");
        assert_eq!(truncate_string("hello world", 8), "hello...");
        assert_eq!(truncate_string("hi", 10), "hi");
        assert_eq!(truncate_string("", 10), "");
    }

    #[test]
    fn test_get_text_preview() {
        let text = "line1\nline2\nline3\nline4";
        assert_eq!(get_text_preview(text, 2, 100), "line1 line2");

        let long_text = "a".repeat(100);
        let preview = get_text_preview(&long_text, 1, 50);
        assert!(preview.len() <= 50);
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn test_format_percentage() {
        assert_eq!(format_percentage(0.5), "50.0%");
        assert_eq!(format_percentage(0.123), "12.3%");
        assert_eq!(format_percentage(1.0), "100.0%");
        assert_eq!(format_percentage(0.0), "0.0%");
    }

    #[test]
    fn test_format_ratio() {
        assert_eq!(format_ratio(100, 50), "2.00x");
        assert_eq!(format_ratio(50, 100), "0.50x");
        assert_eq!(format_ratio(0, 100), "0.00x");
        assert_eq!(format_ratio(100, 0), "N/A");
    }

    #[test]
    fn test_format_duration() {
        use std::time::Duration;

        assert!(format_duration(Duration::from_secs(0)).contains("μs"));
        assert!(format_duration(Duration::from_millis(500)).contains("ms"));
        assert!(format_duration(Duration::from_secs(5)).contains("s"));
        assert!(format_duration(Duration::from_secs(65)).contains("m"));
        assert!(format_duration(Duration::from_secs(125)).contains("m"));
    }
}
