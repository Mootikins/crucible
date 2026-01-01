//! Regex-based text search (always available fallback)

use anyhow::Result;
use async_trait::async_trait;
use crucible_core::traits::{TextSearchMatch, TextSearcher};
use regex::Regex;
use std::path::PathBuf;
use tokio::fs;

pub struct RegexSearcher;

impl RegexSearcher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RegexSearcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TextSearcher for RegexSearcher {
    async fn search(&self, pattern: &str, paths: &[PathBuf]) -> Result<Vec<TextSearchMatch>> {
        let re = Regex::new(pattern)?;
        let mut matches = Vec::new();

        for path in paths {
            if path.is_file() {
                matches.extend(search_file(&re, path).await?);
            } else if path.is_dir() {
                matches.extend(search_dir(&re, path).await?);
            }
        }

        Ok(matches)
    }

    fn backend_name(&self) -> &'static str {
        "regex"
    }
}

async fn search_file(re: &Regex, path: &PathBuf) -> Result<Vec<TextSearchMatch>> {
    let content = fs::read_to_string(path).await?;
    let mut matches = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        for m in re.find_iter(line) {
            matches.push(TextSearchMatch {
                path: path.clone(),
                line_number: line_num + 1,
                line_content: line.to_string(),
                match_start: m.start(),
                match_end: m.end(),
            });
        }
    }

    Ok(matches)
}

async fn search_dir(re: &Regex, dir: &PathBuf) -> Result<Vec<TextSearchMatch>> {
    let mut matches = Vec::new();
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            matches.extend(search_file(re, &path).await?);
        } else if path.is_dir() {
            matches.extend(Box::pin(search_dir(re, &path)).await?);
        }
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_regex_search_finds_pattern() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.md");
        write(&file, "hello world\nfoo bar\nhello again").unwrap();

        let searcher = RegexSearcher::new();
        let results = searcher.search("hello", &[file]).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].line_number, 1);
        assert_eq!(results[1].line_number, 3);
    }

    #[tokio::test]
    async fn test_regex_search_directory() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path().join("a.md"), "find me").unwrap();
        write(tmp.path().join("b.md"), "nothing here").unwrap();
        write(tmp.path().join("c.txt"), "find me too").unwrap(); // ignored, not .md

        let searcher = RegexSearcher::new();
        let results = searcher
            .search("find", &[tmp.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(results.len(), 1); // only a.md
    }

    #[tokio::test]
    async fn test_regex_backend_name() {
        let searcher = RegexSearcher::new();
        assert_eq!(searcher.backend_name(), "regex");
    }
}
