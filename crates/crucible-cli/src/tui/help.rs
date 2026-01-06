//! Help system for the TUI.
//!
//! Provides searchable access to embedded documentation via the `:help` command.
//! Documentation is extracted from the binary to disk on first use.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::help_assets::EmbeddedDocs;
use super::widgets::FuzzyMatcher;
use rust_embed::Embed;

/// A help topic representing a documentation file.
#[derive(Debug, Clone)]
pub struct HelpTopic {
    /// Display name (e.g., "Help/CLI/chat")
    pub name: String,
    /// Full path to the extracted markdown file
    pub path: PathBuf,
    /// First non-empty line of content (for preview)
    pub summary: String,
}

impl HelpTopic {
    /// Get the topic name for display in popups
    pub fn display_name(&self) -> &str {
        &self.name
    }
}

/// Index of available help topics with search capability.
pub struct DocsIndex {
    topics: Vec<HelpTopic>,
    docs_dir: PathBuf,
    matcher: FuzzyMatcher,
}

impl DocsIndex {
    /// Initialize the docs index, extracting embedded docs if needed.
    pub fn init() -> io::Result<Self> {
        let docs_dir = ensure_docs_extracted()?;
        let topics = scan_topics(&docs_dir)?;

        Ok(Self {
            topics,
            docs_dir,
            matcher: FuzzyMatcher::new(),
        })
    }

    /// Get the docs directory path.
    pub fn docs_dir(&self) -> &Path {
        &self.docs_dir
    }

    /// Get all topics (for showing full list in popup).
    pub fn all_topics(&self) -> &[HelpTopic] {
        &self.topics
    }

    /// Fuzzy search topics by name.
    ///
    /// Returns topics sorted by match score (best first).
    pub fn search(&mut self, query: &str) -> Vec<&HelpTopic> {
        if query.is_empty() {
            return self.topics.iter().collect();
        }

        let mut scored: Vec<(usize, u32)> = self
            .topics
            .iter()
            .enumerate()
            .filter_map(|(idx, topic)| {
                self.matcher
                    .score(query, &topic.name)
                    .map(|score| (idx, score))
            })
            .collect();

        // Sort by score descending (higher is better)
        scored.sort_by(|a, b| b.1.cmp(&a.1));

        scored.iter().map(|(idx, _)| &self.topics[*idx]).collect()
    }

    /// Find an exact topic match by name.
    pub fn find_exact(&self, name: &str) -> Option<&HelpTopic> {
        self.topics.iter().find(|t| t.name == name)
    }

    /// Load the content of a topic.
    pub fn load_content(&self, topic: &HelpTopic) -> io::Result<String> {
        let content = fs::read_to_string(&topic.path)?;
        Ok(strip_frontmatter(&content))
    }
}

/// Ensure embedded docs are extracted to disk.
///
/// Path priority:
/// 1. $CRUCIBLE_DOCS environment variable
/// 2. ~/.local/share/crucible/docs/ (or platform equivalent)
fn ensure_docs_extracted() -> io::Result<PathBuf> {
    // Check env override first
    if let Ok(docs_path) = std::env::var("CRUCIBLE_DOCS") {
        let path = PathBuf::from(docs_path);
        if path.exists() {
            return Ok(path);
        }
    }

    // Default location
    let target = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crucible")
        .join("docs");

    let version_file = target.join(".version");
    let current_version = env!("CARGO_PKG_VERSION");

    // Check if already extracted and version matches
    if version_file.exists() {
        if let Ok(existing_version) = fs::read_to_string(&version_file) {
            if existing_version.trim() == current_version {
                return Ok(target);
            }
        }
    }

    // Extract embedded docs
    tracing::info!("Extracting help documentation to {:?}", target);
    fs::create_dir_all(&target)?;

    for file_path in <EmbeddedDocs as Embed>::iter() {
        if let Some(file) = EmbeddedDocs::get(&file_path) {
            let dest = target.join(file_path.as_ref());
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, file.data.as_ref())?;
        }
    }

    // Write version marker
    fs::write(&version_file, current_version)?;

    Ok(target)
}

/// Scan the docs directory for markdown files and build topic list.
fn scan_topics(docs_dir: &Path) -> io::Result<Vec<HelpTopic>> {
    let mut topics = Vec::new();

    for entry in walkdir::WalkDir::new(docs_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip non-markdown files
        if !path.extension().is_some_and(|ext| ext == "md") {
            continue;
        }

        // Skip version marker
        if path.file_name().is_some_and(|n| n == ".version") {
            continue;
        }

        // Build topic name from relative path
        let relative = path.strip_prefix(docs_dir).unwrap_or(path);
        let name = relative
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/"); // Normalize Windows paths

        // Extract summary from first non-empty, non-frontmatter line
        let summary = extract_summary(path).unwrap_or_default();

        topics.push(HelpTopic {
            name,
            path: path.to_path_buf(),
            summary,
        });
    }

    // Sort alphabetically
    topics.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(topics)
}

/// Extract the first meaningful line from a markdown file for preview.
fn extract_summary(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let content = strip_frontmatter(&content);

    for line in content.lines() {
        let trimmed = line.trim();
        // Skip empty lines and headings
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Return first content line, truncated
        let summary = if trimmed.len() > 80 {
            format!("{}...", &trimmed[..77])
        } else {
            trimmed.to_string()
        };
        return Some(summary);
    }

    None
}

/// Strip YAML frontmatter from markdown content.
fn strip_frontmatter(content: &str) -> String {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return content.to_string();
    }

    // Find the closing ---
    if let Some(end_pos) = trimmed[3..].find("\n---") {
        // Skip past the closing --- and newline
        let after_frontmatter = &trimmed[3 + end_pos + 4..];
        return after_frontmatter.trim_start().to_string();
    }

    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_frontmatter_with_frontmatter() {
        let content = "---\ntitle: Test\n---\n\nActual content";
        let result = strip_frontmatter(content);
        assert_eq!(result, "Actual content");
    }

    #[test]
    fn strip_frontmatter_without_frontmatter() {
        let content = "# Title\n\nContent";
        let result = strip_frontmatter(content);
        assert_eq!(result, content);
    }

    #[test]
    fn strip_frontmatter_empty() {
        let content = "";
        let result = strip_frontmatter(content);
        assert_eq!(result, "");
    }

    #[test]
    fn extract_summary_skips_headers() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.md");
        fs::write(&file, "# Header\n\nThis is the summary.").unwrap();

        let summary = extract_summary(&file);
        assert_eq!(summary, Some("This is the summary.".to_string()));
    }

    #[test]
    fn extract_summary_truncates_long_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.md");
        let long_line = "a".repeat(100);
        fs::write(&file, &long_line).unwrap();

        let summary = extract_summary(&file);
        assert!(summary.is_some());
        let s = summary.unwrap();
        assert!(s.ends_with("..."));
        assert!(s.len() <= 80);
    }
}
