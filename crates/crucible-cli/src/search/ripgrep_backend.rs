//! Ripgrep-based text search (preferred when available)

use anyhow::{Context, Result};
use async_trait::async_trait;
use crucible_core::traits::{TextSearchMatch, TextSearcher};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct RipgrepSearcher;

impl RipgrepSearcher {
    /// Check if ripgrep is available
    pub async fn is_available() -> bool {
        Command::new("rg")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    pub fn new() -> Self {
        Self
    }
}

impl Default for RipgrepSearcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TextSearcher for RipgrepSearcher {
    async fn search(&self, pattern: &str, paths: &[PathBuf]) -> Result<Vec<TextSearchMatch>> {
        let mut cmd = Command::new("rg");
        cmd.arg("--line-number")
            .arg("--column")
            .arg("--no-heading")
            .arg("--color=never")
            .arg("-g")
            .arg("*.md")
            .arg(pattern);

        for path in paths {
            cmd.arg(path);
        }

        let output = cmd.output().await.context("Failed to run ripgrep")?;

        // rg returns exit code 1 when no matches found, which is fine
        // Only treat as error if we have stderr output and non-zero exit
        if !output.status.success() && output.stdout.is_empty() && !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("ripgrep error: {}", stderr);
        }

        parse_rg_output(&output.stdout)
    }

    fn backend_name(&self) -> &'static str {
        "ripgrep"
    }
}

fn parse_rg_output(output: &[u8]) -> Result<Vec<TextSearchMatch>> {
    let text = String::from_utf8_lossy(output);
    let mut matches = Vec::new();

    for line in text.lines() {
        // Format: path:line:column:content
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 4 {
            let path = PathBuf::from(parts[0]);
            let line_number: usize = parts[1].parse().unwrap_or(0);
            let column: usize = parts[2].parse().unwrap_or(0);
            let content = parts[3].to_string();

            matches.push(TextSearchMatch {
                path,
                line_number,
                line_content: content,
                match_start: column.saturating_sub(1),
                match_end: column, // Approximate; rg doesn't give match length easily
            });
        }
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ripgrep_availability_check() {
        // Just test it doesn't panic
        let _ = RipgrepSearcher::is_available().await;
    }

    #[test]
    fn test_parse_rg_output() {
        let output = b"test.md:5:10:hello world\nother.md:1:1:foo bar\n";
        let matches = parse_rg_output(output).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].path, PathBuf::from("test.md"));
        assert_eq!(matches[0].line_number, 5);
        assert_eq!(matches[1].line_content, "foo bar");
    }

    #[test]
    fn test_parse_rg_empty_output() {
        let matches = parse_rg_output(b"").unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_parse_rg_output_match_positions() {
        let output = b"file.md:10:5:some text here\n";
        let matches = parse_rg_output(output).unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].match_start, 4); // column 5 -> 0-indexed = 4
        assert_eq!(matches[0].match_end, 5);
    }

    #[test]
    fn test_ripgrep_backend_name() {
        let searcher = RipgrepSearcher::new();
        assert_eq!(searcher.backend_name(), "ripgrep");
    }

    #[test]
    fn test_ripgrep_default() {
        let _searcher: RipgrepSearcher = Default::default();
    }
}
