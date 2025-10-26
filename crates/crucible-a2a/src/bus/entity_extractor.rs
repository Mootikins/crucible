/// Entity extraction from message content
///
/// Phase 1: Simple regex-based extraction
/// Phase 2: NER with lightweight ML model
use regex::Regex;
use std::collections::HashSet;

/// Extract entities from text using regex patterns
pub struct EntityExtractor {
    /// Regex for #tags
    tag_pattern: Regex,

    /// Regex for file paths
    file_pattern: Regex,

    /// Regex for @mentions
    mention_pattern: Regex,

    /// Regex for capitalized words (potential entity names)
    capitalized_pattern: Regex,
}

impl EntityExtractor {
    /// Create a new entity extractor
    pub fn new() -> Self {
        Self {
            // Match #tag or #multi-word-tag
            tag_pattern: Regex::new(r"#[\w-]+").unwrap(),

            // Match file paths like src/main.rs, docs/README.md, ./file.txt
            file_pattern: Regex::new(r"\b[\w./\\-]+\.(rs|md|txt|py|js|ts|json|toml|yaml|yml)\b")
                .unwrap(),

            // Match @agent-name or @username
            mention_pattern: Regex::new(r"@[\w-]+").unwrap(),

            // Match capitalized words (2+ chars) that might be project names, etc.
            // Exclude common words at start of sentences
            capitalized_pattern: Regex::new(r"\b[A-Z][a-z]+[A-Z]\w*\b").unwrap(),
        }
    }

    /// Extract all entities from text
    ///
    /// Returns a set of unique entity names found in the text.
    pub fn extract(&self, text: &str) -> HashSet<String> {
        let mut entities = HashSet::new();

        // Extract #tags
        for cap in self.tag_pattern.captures_iter(text) {
            entities.insert(cap[0].to_string());
        }

        // Extract file paths
        for cap in self.file_pattern.captures_iter(text) {
            entities.insert(cap[0].to_string());
        }

        // Extract @mentions
        for cap in self.mention_pattern.captures_iter(text) {
            entities.insert(cap[0].to_string());
        }

        // Extract CamelCase words (ProjectNames, etc.)
        for cap in self.capitalized_pattern.captures_iter(text) {
            let word = &cap[0];
            // Filter out common sentence starters
            if !is_common_word(word) {
                entities.insert(word.to_string());
            }
        }

        entities
    }
}

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a word is a common sentence starter or word
fn is_common_word(word: &str) -> bool {
    matches!(
        word,
        "The"
            | "This"
            | "That"
            | "These"
            | "Those"
            | "When"
            | "Where"
            | "What"
            | "Which"
            | "Who"
            | "How"
            | "Why"
            | "Can"
            | "Could"
            | "Would"
            | "Should"
            | "Will"
            | "May"
            | "Might"
            | "Must"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tags() {
        let extractor = EntityExtractor::new();
        let text = "Working on #project-alpha and #Q4Goals for the team";

        let entities = extractor.extract(text);

        assert!(entities.contains("#project-alpha"));
        assert!(entities.contains("#Q4Goals"));
    }

    #[test]
    fn test_extract_file_paths() {
        let extractor = EntityExtractor::new();
        let text = "Updated src/main.rs and docs/README.md with new changes";

        let entities = extractor.extract(text);

        assert!(entities.contains("src/main.rs"));
        assert!(entities.contains("docs/README.md"));
    }

    #[test]
    fn test_extract_mentions() {
        let extractor = EntityExtractor::new();
        let text = "Coordinating with @researcher-001 and @backend-agent";

        let entities = extractor.extract(text);

        assert!(entities.contains("@researcher-001"));
        assert!(entities.contains("@backend-agent"));
    }

    #[test]
    fn test_extract_camelcase_names() {
        let extractor = EntityExtractor::new();
        let text = "ProjectAlpha and BudgetReport2025 are ready for review";

        let entities = extractor.extract(text);

        assert!(entities.contains("ProjectAlpha"));
        assert!(entities.contains("BudgetReport2025"));
    }

    #[test]
    fn test_filter_common_words() {
        let extractor = EntityExtractor::new();
        let text = "The ProjectAlpha is complete. This WorkItem was finished.";

        let entities = extractor.extract(text);

        assert!(entities.contains("ProjectAlpha"));
        assert!(entities.contains("WorkItem"));
        assert!(!entities.contains("The"));
        assert!(!entities.contains("This"));
    }

    #[test]
    fn test_mixed_entities() {
        let extractor = EntityExtractor::new();
        let text = "
            Working on #sprint-23 with @backend-team.
            Updated src/api/routes.rs for ProjectDelta.
            Need to sync with DatabaseMigration tasks.
        ";

        let entities = extractor.extract(text);

        assert!(entities.contains("#sprint-23"));
        assert!(entities.contains("@backend-team"));
        assert!(entities.contains("src/api/routes.rs"));
        assert!(entities.contains("ProjectDelta"));
        assert!(entities.contains("DatabaseMigration"));
    }

    #[test]
    fn test_empty_text() {
        let extractor = EntityExtractor::new();
        let entities = extractor.extract("");

        assert!(entities.is_empty());
    }

    #[test]
    fn test_no_entities() {
        let extractor = EntityExtractor::new();
        let text = "This is a simple sentence with no special entities.";

        let entities = extractor.extract(text);

        // Might be empty or just have "This" filtered out
        assert!(!entities.contains("simple"));
        assert!(!entities.contains("sentence"));
    }
}
