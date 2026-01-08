//! Context registry for agents, files, and notes
//!
//! Handles `@agent` mentions, file references, and `[[note]]` links.

use crate::tui::popup::PopupProvider;
use crate::tui::state::types::{PopupItem, PopupKind};
use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Matcher, Utf32String};

/// Registry for context items: agents, files, and notes
#[derive(Clone, Default)]
pub struct ContextRegistry {
    agents: Vec<AgentEntry>,
    files: Vec<String>,
    notes: Vec<String>,
}

/// An agent entry with metadata
#[derive(Clone, Debug)]
pub struct AgentEntry {
    pub id: String,
    pub description: String,
}

impl ContextRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an agent to the registry
    pub fn add_agent(&mut self, id: &str, description: &str) {
        self.agents.push(AgentEntry {
            id: id.to_string(),
            description: description.to_string(),
        });
    }

    /// Set all agents at once (replaces existing)
    pub fn set_agents(&mut self, agents: Vec<(String, String)>) {
        self.agents = agents
            .into_iter()
            .map(|(id, description)| AgentEntry { id, description })
            .collect();
    }

    /// Add a file to the registry
    pub fn add_file(&mut self, path: &str) {
        self.files.push(path.to_string());
    }

    /// Set all files at once (replaces existing)
    pub fn set_files(&mut self, files: Vec<String>) {
        self.files = files;
    }

    /// Add a note to the registry
    pub fn add_note(&mut self, path: &str) {
        self.notes.push(path.to_string());
    }

    /// Set all notes at once (replaces existing)
    pub fn set_notes(&mut self, notes: Vec<String>) {
        self.notes = notes;
    }

    /// Get agent count
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Get file count
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get note count
    pub fn note_count(&self) -> usize {
        self.notes.len()
    }
}

impl PopupProvider for ContextRegistry {
    fn provide(&self, _kind: PopupKind, query: &str) -> Vec<PopupItem> {
        let query = query.split_whitespace().next().unwrap_or("").trim();
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let mut pattern = nucleo::pattern::MultiPattern::new(1);
        pattern.reparse(0, query, CaseMatching::Ignore, Normalization::Smart, false);

        let mut out = Vec::new();

        // Match agents
        for agent in &self.agents {
            let match_col = Utf32String::from(agent.id.as_str());
            let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
            if let Some(score) = score {
                out.push(
                    PopupItem::agent(&agent.id)
                        .desc(&agent.description)
                        .with_score(score.min(i32::MAX as u32) as i32),
                );
            }
        }

        // Match files
        for file in &self.files {
            let match_col = Utf32String::from(file.as_str());
            let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
            if let Some(score) = score {
                out.push(PopupItem::file(file).with_score(score.min(i32::MAX as u32) as i32));
            }
        }

        // Match notes
        for note in &self.notes {
            let match_col = Utf32String::from(note.as_str());
            let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
            if let Some(score) = score {
                out.push(PopupItem::note(note).with_score(score.min(i32::MAX as u32) as i32));
            }
        }

        // Sort by score descending, truncate
        out.sort_by_key(|item| std::cmp::Reverse(item.score()));
        out.truncate(20);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_registry_matches_agents() {
        let mut registry = ContextRegistry::new();
        registry.add_agent("dev-agent", "Helps with development");
        registry.add_agent("code-reviewer", "Reviews code");

        // "dev-ag" should match only dev-agent, not code-reviewer
        let items = registry.provide(PopupKind::AgentOrFile, "dev-ag");
        assert_eq!(items.len(), 1);
        assert!(items[0].is_agent());
        assert_eq!(items[0].title(), "@dev-agent");
    }

    #[test]
    fn test_context_registry_matches_files() {
        let mut registry = ContextRegistry::new();
        registry.add_file("src/main.rs");
        registry.add_file("src/lib.rs");

        let items = registry.provide(PopupKind::AgentOrFile, "main");
        assert_eq!(items.len(), 1);
        assert!(items[0].is_file());
    }

    #[test]
    fn test_context_registry_matches_notes() {
        let mut registry = ContextRegistry::new();
        registry.add_note("note:project/readme.md");
        registry.add_note("note:ideas/brainstorm.md");

        let items = registry.provide(PopupKind::AgentOrFile, "readme");
        assert_eq!(items.len(), 1);
        assert!(items[0].is_note());
    }

    #[test]
    fn test_context_registry_fuzzy_path_match() {
        let mut registry = ContextRegistry::new();
        registry.add_file("src/components/button.tsx");

        // Path fuzzy matching
        let items = registry.provide(PopupKind::AgentOrFile, "btn");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_context_registry_set_bulk() {
        let mut registry = ContextRegistry::new();
        registry.set_agents(vec![
            ("a".into(), "Agent A".into()),
            ("b".into(), "Agent B".into()),
        ]);
        assert_eq!(registry.agent_count(), 2);

        registry.set_files(vec!["x.rs".into(), "y.rs".into()]);
        assert_eq!(registry.file_count(), 2);

        registry.set_notes(vec!["note:foo.md".into()]);
        assert_eq!(registry.note_count(), 1);
    }

    #[test]
    fn test_context_registry_mixed_results() {
        let mut registry = ContextRegistry::new();
        registry.add_agent("test-agent", "Test");
        registry.add_file("test.rs");
        registry.add_note("note:test.md");

        // All should match "test"
        let items = registry.provide(PopupKind::AgentOrFile, "test");
        assert_eq!(items.len(), 3);
    }
}
