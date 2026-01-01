//! Wikilink search utilities

use anyhow::Result;
use crucible_core::traits::TextSearcher;
use std::path::PathBuf;

/// Find files that link to a given note
///
/// Uses a permissive regex to find all files that might contain a wikilink
/// to the target note. Handles various wikilink forms:
/// - `[[Note]]` - basic link
/// - `[[Note|alias]]` - display alias
/// - `[[Note#Heading]]` - heading reference
/// - `[[Note#^block-id]]` - block reference
/// - `[[folder/Note]]` - path
pub async fn find_backlinks(
    searcher: &dyn TextSearcher,
    target_note: &str,
    search_paths: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    // Permissive regex: matches [[...target...]] in any form
    // The target can appear anywhere between [[ and ]]
    let escaped = regex::escape(target_note);
    let pattern = format!(r"\[\[[^\]]*{}[^\]]*\]\]", escaped);

    let matches = searcher.search(&pattern, search_paths).await?;

    // Deduplicate by path (same file may have multiple links)
    let mut paths: Vec<PathBuf> = matches.into_iter().map(|m| m.path).collect();
    paths.sort();
    paths.dedup();

    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::RegexSearcher;
    use std::fs::write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_find_backlinks_basic() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path().join("a.md"), "Link to [[Target]]").unwrap();
        write(tmp.path().join("b.md"), "No links here").unwrap();

        let searcher = RegexSearcher::new();
        let results = find_backlinks(&searcher, "Target", &[tmp.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].ends_with("a.md"));
    }

    #[tokio::test]
    async fn test_find_backlinks_with_alias() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path().join("a.md"), "Link to [[Target|display text]]").unwrap();

        let searcher = RegexSearcher::new();
        let results = find_backlinks(&searcher, "Target", &[tmp.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_find_backlinks_with_heading() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path().join("a.md"), "Link to [[Target#Section]]").unwrap();

        let searcher = RegexSearcher::new();
        let results = find_backlinks(&searcher, "Target", &[tmp.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_find_backlinks_with_block_ref() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path().join("a.md"), "Link to [[Target#^block-id]]").unwrap();

        let searcher = RegexSearcher::new();
        let results = find_backlinks(&searcher, "Target", &[tmp.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_find_backlinks_deduplicates() {
        let tmp = TempDir::new().unwrap();
        // Multiple links in same file
        write(tmp.path().join("a.md"), "[[Target]] and [[Target|alias]]").unwrap();

        let searcher = RegexSearcher::new();
        let results = find_backlinks(&searcher, "Target", &[tmp.path().to_path_buf()])
            .await
            .unwrap();

        // Should deduplicate to single path
        assert_eq!(results.len(), 1);
    }
}
