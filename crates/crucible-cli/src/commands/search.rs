//! Top-level `cru search` command for searching kiln notes.
//!
//! Supports semantic search (via embeddings + vector search), text search
//! (title/name matching), or both combined.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::common::daemon_client;
use crate::config::CliConfig;
use crate::interactive::SearchResultWithScore;
use crate::output;

/// Which search backends to use
enum SearchMode {
    Semantic,
    Text,
    Both,
}

impl SearchMode {
    fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "semantic" => Self::Semantic,
            "text" => Self::Text,
            _ => Self::Both,
        }
    }

    fn includes_text(&self) -> bool {
        matches!(self, Self::Text | Self::Both)
    }

    fn includes_semantic(&self) -> bool {
        matches!(self, Self::Semantic | Self::Both)
    }
}

/// Execute the `cru search` command.
pub async fn execute(
    config: CliConfig,
    query: &str,
    limit: usize,
    search_type: &str,
    format: &str,
) -> Result<()> {
    let kiln_path = &config.kiln_path;

    // Verify a kiln is configured
    if !kiln_path.join(".crucible").join("kiln.toml").exists() {
        anyhow::bail!("No kiln is open. Run `cru init` to create one.");
    }

    let client = daemon_client().await?;

    // Ensure the kiln is registered with the daemon
    client
        .kiln_open(kiln_path)
        .await
        .context("Failed to open kiln in daemon")?;

    // Collect all kilns to search: primary + any others registered with the daemon
    let all_kilns = collect_search_kilns(&client, kiln_path).await;

    let mode = SearchMode::parse(search_type);
    let mut results: Vec<SearchResultWithScore> = Vec::new();

    // --- Text search: match query against note names/titles/paths ---
    if mode.includes_text() {
        let query_lower = query.to_lowercase();

        for kiln in &all_kilns {
            let notes = match client.list_notes(kiln, None).await {
                Ok(n) => n,
                Err(e) => {
                    output::warning(&format!(
                        "Failed to list notes from {}: {e:#}",
                        kiln.display()
                    ));
                    continue;
                }
            };

            for (name, path, title, _tags, _updated) in notes {
                let display_title = title.as_deref().unwrap_or(&name);
                if (display_title.to_lowercase().contains(&query_lower)
                    || name.to_lowercase().contains(&query_lower)
                    || path.to_lowercase().contains(&query_lower))
                    && !results.iter().any(|r| r.id == path)
                {
                    let content = extract_snippet(kiln, &path, 200);
                    results.push(SearchResultWithScore {
                        id: path,
                        title: display_title.to_string(),
                        content,
                        score: 1.0,
                    });
                }
            }
        }
    }

    // --- Semantic search: embed query → vector search ---
    if mode.includes_semantic() {
        for kiln in &all_kilns {
            match run_semantic_search(&config, &client, kiln, query, limit).await {
                Ok(semantic_hits) => {
                    for (doc_id, score) in semantic_hits {
                        // De-duplicate against text results
                        if !results.iter().any(|r| r.id == doc_id) {
                            let title = doc_id
                                .split('/')
                                .next_back()
                                .unwrap_or(&doc_id)
                                .trim_end_matches(".md")
                                .to_string();
                            let content = extract_snippet(kiln, &doc_id, 200);
                            results.push(SearchResultWithScore {
                                id: doc_id,
                                title,
                                content,
                                score,
                            });
                        }
                    }
                }
                Err(e) => {
                    output::warning(&format!(
                        "Semantic search unavailable for {}: {e:#}",
                        kiln.display()
                    ));
                }
            }
        }
    }

    // Sort by score descending, then truncate
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    // --- Output ---
    if results.is_empty() {
        if format == "json" {
            println!("[]");
        } else {
            output::info(&format!("No results found for '{query}'"));
        }
        return Ok(());
    }

    let formatted =
        output::format_search_results(&results, format, mode.includes_semantic(), false)?;
    println!("{formatted}");

    Ok(())
}

/// Collect all kiln paths to search: the primary kiln plus any other
/// kilns currently registered with the daemon.
async fn collect_search_kilns(
    client: &crucible_daemon::DaemonClient,
    primary_kiln: &Path,
) -> Vec<PathBuf> {
    let mut kilns = vec![primary_kiln.to_path_buf()];

    if let Ok(registered) = client.kiln_list().await {
        for kiln_info in registered {
            if let Some(path_str) = kiln_info.get("path").and_then(|v| v.as_str()) {
                let path = PathBuf::from(path_str);
                if path != primary_kiln {
                    kilns.push(path);
                }
            }
        }
    }

    kilns
}

/// Read first few non-frontmatter lines from a note file as a snippet preview.
fn extract_snippet(kiln_path: &Path, note_path: &str, max_chars: usize) -> String {
    let full_path = kiln_path.join(note_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // Skip YAML frontmatter (lines between --- delimiters)
    let body = if let Some(rest) = content.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            rest[end + 4..].trim_start()
        } else {
            content.as_str()
        }
    } else {
        content.as_str()
    };

    let snippet: String = body
        .lines()
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");

    if snippet.chars().count() > max_chars {
        let truncated: String = snippet.chars().take(max_chars).collect();
        format!("{truncated}...")
    } else {
        snippet
    }
}

/// Generate query embedding and call `search_vectors` on the daemon.
async fn run_semantic_search(
    config: &CliConfig,
    client: &crucible_daemon::DaemonClient,
    kiln_path: &std::path::Path,
    query: &str,
    limit: usize,
) -> Result<Vec<(String, f64)>> {
    let embedding_config = crate::factories::embedding_provider_config_from_cli(config);
    let provider = crucible_llm::embeddings::create_provider(embedding_config)
        .await
        .context("Failed to create embedding provider")?;
    let query_embedding = provider
        .embed(query)
        .await
        .context("Failed to generate query embedding")?;
    client
        .search_vectors(kiln_path, &query_embedding, limit)
        .await
        .context("Vector search failed")
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactive::SearchResultWithScore;
    use clap::Parser;

    // ---- Output formatting (TDD: written before implementation) ----

    fn sample_results() -> Vec<SearchResultWithScore> {
        vec![
            SearchResultWithScore {
                id: "Help/Wikilinks.md".into(),
                title: "Wikilinks".into(),
                content: "Wikilinks connect notes together".into(),
                score: 0.92,
            },
            SearchResultWithScore {
                id: "Help/Tags.md".into(),
                title: "Tags".into(),
                content: "Tags categorize notes".into(),
                score: 0.78,
            },
        ]
    }

    #[test]
    fn search_command_format_json_is_valid() {
        let results = sample_results();
        let json = output::format_search_results(&results, "json", true, false).unwrap();
        let parsed: Vec<SearchResultWithScore> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].title, "Wikilinks");
        assert_eq!(parsed[1].title, "Tags");
    }

    #[test]
    fn search_command_format_plain_contains_titles() {
        let results = sample_results();
        let plain = output::format_search_results(&results, "plain", true, false).unwrap();
        assert!(plain.contains("Wikilinks"));
        assert!(plain.contains("Tags"));
        assert!(plain.contains("0.92"));
    }

    #[test]
    fn search_command_format_table_contains_titles() {
        let results = sample_results();
        let table = output::format_search_results(&results, "table", false, false).unwrap();
        assert!(table.contains("Wikilinks"));
        assert!(table.contains("Tags"));
    }

    #[test]
    fn search_command_format_empty_json() {
        let results: Vec<SearchResultWithScore> = vec![];
        let json = output::format_search_results(&results, "json", true, false).unwrap();
        let parsed: Vec<SearchResultWithScore> = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn search_command_format_scores_hidden_when_text_only() {
        let results = sample_results();
        let plain = output::format_search_results(&results, "plain", false, false).unwrap();
        // Score column should not appear when show_scores=false
        assert!(!plain.contains("0.92"));
    }

    // ---- CLI parsing ----

    #[test]
    fn search_command_parses_basic() {
        let cli = crate::cli::Cli::try_parse_from(["cru", "search", "wikilink"]).unwrap();
        if let Some(crate::cli::Commands::Search {
            query,
            limit,
            r#type,
            ..
        }) = cli.command
        {
            assert_eq!(query, "wikilink");
            assert_eq!(limit, 10);
            assert_eq!(r#type, "both");
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn search_command_parses_with_options() {
        let cli = crate::cli::Cli::try_parse_from([
            "cru", "search", "rust", "--limit", "5", "--type", "semantic", "-f", "json",
        ])
        .unwrap();
        if let Some(crate::cli::Commands::Search {
            query,
            limit,
            r#type,
            format,
        }) = cli.command
        {
            assert_eq!(query, "rust");
            assert_eq!(limit, 5);
            assert_eq!(r#type, "semantic");
            assert_eq!(format, "json");
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn search_command_parses_text_type() {
        let cli =
            crate::cli::Cli::try_parse_from(["cru", "search", "test", "--type", "text"]).unwrap();
        if let Some(crate::cli::Commands::Search { r#type, .. }) = cli.command {
            assert_eq!(r#type, "text");
        } else {
            panic!("Expected Search command");
        }
    }

    // ---- SearchMode ----

    #[test]
    fn search_command_mode_parsing() {
        assert!(SearchMode::parse("semantic").includes_semantic());
        assert!(!SearchMode::parse("semantic").includes_text());
        assert!(SearchMode::parse("text").includes_text());
        assert!(!SearchMode::parse("text").includes_semantic());
        assert!(SearchMode::parse("both").includes_text());
        assert!(SearchMode::parse("both").includes_semantic());
        assert!(SearchMode::parse("anything").includes_text()); // defaults to both
    }

    // ---- extract_snippet ----

    #[test]
    fn search_command_extract_snippet_plain_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "First line\nSecond line\n").unwrap();
        let snippet = extract_snippet(dir.path(), "note.md", 200);
        assert_eq!(snippet, "First line Second line");
    }

    #[test]
    fn search_command_extract_snippet_skips_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("note.md"),
            "---\ntitle: Test\ntags: [a]\n---\nBody content here\n",
        )
        .unwrap();
        let snippet = extract_snippet(dir.path(), "note.md", 200);
        assert_eq!(snippet, "Body content here");
    }

    #[test]
    fn search_command_extract_snippet_truncates_long_content() {
        let dir = tempfile::tempdir().unwrap();
        let long = "a".repeat(300);
        std::fs::write(dir.path().join("note.md"), &long).unwrap();
        let snippet = extract_snippet(dir.path(), "note.md", 50);
        assert!(snippet.ends_with("..."));
        assert!(snippet.len() <= 54); // 50 chars + "..."
    }

    #[test]
    fn search_command_extract_snippet_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let snippet = extract_snippet(dir.path(), "nonexistent.md", 200);
        assert!(snippet.is_empty());
    }

    #[test]
    fn search_command_extract_snippet_skips_empty_lines() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "\n\nFirst\n\nSecond\n").unwrap();
        let snippet = extract_snippet(dir.path(), "note.md", 200);
        assert_eq!(snippet, "First Second");
    }
}
