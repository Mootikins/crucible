// Output formatting for query results
//
// Provides different output formats: table (human), JSON (machines), CSV (export)

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

/// Query result representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Result rows (each row is a map of column -> value)
    pub rows: Vec<BTreeMap<String, serde_json::Value>>,

    /// Query execution time
    pub duration: Duration,

    /// Number of rows affected (for mutations)
    pub affected_rows: Option<u64>,

    /// Query status
    pub status: QueryStatus,
}

impl QueryResult {
    /// Create empty result
    pub fn empty() -> Self {
        Self {
            rows: vec![],
            duration: Duration::ZERO,
            affected_rows: None,
            status: QueryStatus::Success,
        }
    }

    /// Check if result is empty
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get row count
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Get column names (from first row)
    pub fn column_names(&self) -> Vec<String> {
        self.rows
            .first()
            .map(|row| row.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// Query execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryStatus {
    /// Query completed successfully
    Success,

    /// Query completed with warnings
    Warning,

    /// Query failed
    Error,
}

/// Output formatter trait for rendering query results
#[async_trait]
pub trait OutputFormatter: Send + Sync {
    /// Format query results for display
    async fn format(&self, result: QueryResult) -> Result<String>;

    /// Format error message
    fn format_error(&self, error: &anyhow::Error) -> String;
}

// ============================================================================
// Table Formatter (Human-Readable)
// ============================================================================

/// Table formatter for human-readable output
pub struct TableFormatter {
    max_column_width: usize,
    truncate_content: bool,
}

impl TableFormatter {
    pub fn new() -> Self {
        Self {
            max_column_width: 50,
            truncate_content: true,
        }
    }

    pub fn with_max_width(mut self, width: usize) -> Self {
        self.max_column_width = width;
        self
    }

    pub fn with_truncation(mut self, truncate: bool) -> Self {
        self.truncate_content = truncate;
        self
    }

    /// Format a single value for table display
    fn format_value(&self, value: &serde_json::Value) -> String {
        let s = match value {
            serde_json::Value::Null => "NULL".to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => {
                format!("[{} items]", arr.len())
            }
            serde_json::Value::Object(obj) => {
                format!("{{..{} fields..}}", obj.len())
            }
        };

        // Truncate if needed
        if self.truncate_content && s.len() > self.max_column_width {
            format!("{}...", &s[..self.max_column_width.saturating_sub(3)])
        } else {
            s
        }
    }
}

impl Default for TableFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OutputFormatter for TableFormatter {
    async fn format(&self, result: QueryResult) -> Result<String> {
        use comfy_table::{Table, Cell, Row, presets::UTF8_FULL};
        use colored::Colorize;

        if result.is_empty() {
            return Ok(format!(
                "\n{} No results ({:?})\n",
                "ℹ".cyan(),
                result.duration
            ));
        }

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);

        // Add headers
        let headers: Vec<String> = result.column_names();
        let header_cells: Vec<Cell> = headers.iter()
            .map(|h| Cell::new(h).fg(comfy_table::Color::Cyan))
            .collect();
        table.set_header(header_cells);

        // Add rows
        for row_data in &result.rows {
            let cells: Vec<Cell> = headers.iter()
                .map(|col_name| {
                    let value = row_data.get(col_name)
                        .unwrap_or(&serde_json::Value::Null);
                    Cell::new(self.format_value(value))
                })
                .collect();
            table.add_row(cells);
        }

        // Add footer with stats
        let footer = format!(
            "\n{} {} rows in {:?}",
            "✓".green(),
            result.row_count(),
            result.duration
        );

        Ok(format!("\n{}\n{}\n", table, footer))
    }

    fn format_error(&self, error: &anyhow::Error) -> String {
        use colored::Colorize;
        format!("{} {}", "❌".red(), error.to_string().red())
    }
}

// ============================================================================
// JSON Formatter (Machine-Readable)
// ============================================================================

/// JSON formatter for machine-readable output
pub struct JsonFormatter {
    pretty: bool,
}

impl JsonFormatter {
    pub fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

#[async_trait]
impl OutputFormatter for JsonFormatter {
    async fn format(&self, result: QueryResult) -> Result<String> {
        let output = serde_json::json!({
            "rows": result.rows,
            "row_count": result.row_count(),
            "duration_ms": result.duration.as_millis(),
            "status": result.status,
        });

        if self.pretty {
            Ok(serde_json::to_string_pretty(&output)?)
        } else {
            Ok(serde_json::to_string(&output)?)
        }
    }

    fn format_error(&self, error: &anyhow::Error) -> String {
        let error_json = serde_json::json!({
            "error": error.to_string(),
            "status": "error",
        });

        if self.pretty {
            serde_json::to_string_pretty(&error_json).unwrap()
        } else {
            serde_json::to_string(&error_json).unwrap()
        }
    }
}

// ============================================================================
// CSV Formatter (Export-Friendly)
// ============================================================================

/// CSV formatter for export to spreadsheets
pub struct CsvFormatter {
    include_header: bool,
}

impl CsvFormatter {
    pub fn new() -> Self {
        Self {
            include_header: true,
        }
    }

    pub fn without_header(mut self) -> Self {
        self.include_header = false;
        self
    }

    /// Format a value for CSV output
    fn format_value(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => String::new(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => {
                serde_json::to_string(arr).unwrap_or_default()
            }
            serde_json::Value::Object(obj) => {
                serde_json::to_string(obj).unwrap_or_default()
            }
        }
    }
}

impl Default for CsvFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OutputFormatter for CsvFormatter {
    async fn format(&self, result: QueryResult) -> Result<String> {
        if result.is_empty() {
            return Ok(String::new());
        }

        let mut writer = csv::Writer::from_writer(vec![]);
        let headers = result.column_names();

        // Write headers
        if self.include_header {
            writer.write_record(&headers)?;
        }

        // Write rows
        for row_data in &result.rows {
            let values: Vec<String> = headers.iter()
                .map(|col_name| {
                    let value = row_data.get(col_name)
                        .unwrap_or(&serde_json::Value::Null);
                    self.format_value(value)
                })
                .collect();
            writer.write_record(&values)?;
        }

        let bytes = writer.into_inner()?;
        Ok(String::from_utf8(bytes)?)
    }

    fn format_error(&self, error: &anyhow::Error) -> String {
        format!("ERROR,{}", error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> QueryResult {
        let mut row1 = BTreeMap::new();
        row1.insert("id".to_string(), serde_json::json!("note:123"));
        row1.insert("title".to_string(), serde_json::json!("Test Note"));
        row1.insert("tags".to_string(), serde_json::json!(["#project", "#test"]));

        let mut row2 = BTreeMap::new();
        row2.insert("id".to_string(), serde_json::json!("note:456"));
        row2.insert("title".to_string(), serde_json::json!("Another Note"));
        row2.insert("tags".to_string(), serde_json::json!(["#work"]));

        QueryResult {
            rows: vec![row1, row2],
            duration: Duration::from_millis(42),
            affected_rows: None,
            status: QueryStatus::Success,
        }
    }

    #[tokio::test]
    async fn test_table_formatter() {
        let formatter = TableFormatter::new();
        let result = sample_result();
        let output = formatter.format(result).await.unwrap();

        // Should contain column headers
        assert!(output.contains("id"));
        assert!(output.contains("title"));
        assert!(output.contains("tags"));

        // Should contain data
        assert!(output.contains("Test Note"));
        assert!(output.contains("Another Note"));

        // Should contain stats
        assert!(output.contains("2 rows"));
    }

    #[tokio::test]
    async fn test_json_formatter() {
        let formatter = JsonFormatter::new(true);
        let result = sample_result();
        let output = formatter.format(result).await.unwrap();

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        // Check structure
        assert!(parsed["rows"].is_array());
        assert_eq!(parsed["row_count"], 2);
        assert!(parsed["duration_ms"].is_number());
    }

    #[tokio::test]
    async fn test_csv_formatter() {
        let formatter = CsvFormatter::new();
        let result = sample_result();
        let output = formatter.format(result).await.unwrap();

        let lines: Vec<&str> = output.lines().collect();

        // Should have header + 2 data rows
        assert_eq!(lines.len(), 3);

        // Header should contain column names
        assert!(lines[0].contains("id"));
        assert!(lines[0].contains("title"));

        // Data rows should contain values
        assert!(lines[1].contains("note:123"));
        assert!(lines[2].contains("note:456"));
    }

    #[tokio::test]
    async fn test_empty_result() {
        let result = QueryResult::empty();

        let table_formatter = TableFormatter::new();
        let output = table_formatter.format(result.clone()).await.unwrap();
        assert!(output.contains("No results"));

        let json_formatter = JsonFormatter::new(true);
        let output = json_formatter.format(result.clone()).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["row_count"], 0);

        let csv_formatter = CsvFormatter::new();
        let output = csv_formatter.format(result).await.unwrap();
        assert!(output.is_empty());
    }

    #[test]
    fn test_error_formatting() {
        let error = anyhow::anyhow!("Test error");

        let table_formatter = TableFormatter::new();
        let output = table_formatter.format_error(&error);
        assert!(output.contains("Test error"));

        let json_formatter = JsonFormatter::new(true);
        let output = json_formatter.format_error(&error);
        assert!(output.contains("Test error"));
        assert!(output.contains("error"));

        let csv_formatter = CsvFormatter::new();
        let output = csv_formatter.format_error(&error);
        assert!(output.starts_with("ERROR"));
    }
}
