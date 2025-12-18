use crate::tui::state::{PopupItem, PopupItemKind, PopupKind};
use crucible_core::traits::chat::CommandDescriptor;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Provider abstraction so the popup can be fed from CLI or exposed to Rune.
pub trait PopupProvider: Send + Sync {
    fn provide(&self, kind: PopupKind, query: &str) -> Vec<PopupItem>;
}

/// Simple in-memory provider backed by snapshots of commands/agents/files.
#[derive(Clone, Default)]
pub struct StaticPopupProvider {
    pub commands: Vec<CommandDescriptor>,
    pub agents: Vec<(String, String)>, // (id/slug, description)
    pub files: Vec<String>,            // workspace relative
    pub notes: Vec<String>,            // note:<path> or note:<kiln>/<path>
}

impl StaticPopupProvider {
    pub fn new() -> Self {
        Self::default()
    }
}

fn score_match(haystack: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay = haystack.to_lowercase();
    let nee = needle.to_lowercase();
    if let Some(pos) = hay.find(&nee) {
        // Prefix > substring; shorter paths slightly favored
        let base = if pos == 0 { 1000 } else { 500 };
        let len_penalty = (hay.len().saturating_sub(nee.len())) as i32;
        Some(base - len_penalty.min(400))
    } else {
        None
    }
}

impl PopupProvider for StaticPopupProvider {
    fn provide(&self, kind: PopupKind, query: &str) -> Vec<PopupItem> {
        let mut out = Vec::new();
        match kind {
            PopupKind::Command => {
                for cmd in &self.commands {
                    let title = format!("/{}", cmd.name);
                    let subtitle = cmd
                        .input_hint
                        .as_ref()
                        .map(|h| format!("{} — {}", cmd.description, h))
                        .unwrap_or_else(|| cmd.description.clone());
                    if let Some(score) = score_match(&title, query) {
                        out.push(PopupItem {
                            kind: PopupItemKind::Command,
                            title,
                            subtitle,
                            token: format!("/{} ", cmd.name),
                            score,
                            available: true,
                        });
                    }
                }
            }
            PopupKind::AgentOrFile => {
                for (id, desc) in &self.agents {
                    if let Some(score) = score_match(id, query) {
                        out.push(PopupItem {
                            kind: PopupItemKind::Agent,
                            title: format!("@{}", id),
                            subtitle: desc.clone(),
                            token: format!("@{}", id),
                            score,
                            available: true,
                        });
                    }
                }
                for file in &self.files {
                    if let Some(score) = score_match(file, query) {
                        out.push(PopupItem {
                            kind: PopupItemKind::File,
                            title: file.clone(),
                            subtitle: String::from("workspace"),
                            token: file.clone(),
                            score,
                            available: true,
                        });
                    }
                }
                for note in &self.notes {
                    if let Some(score) = score_match(note, query) {
                        out.push(PopupItem {
                            kind: PopupItemKind::Note,
                            title: note.clone(),
                            subtitle: String::from("note"),
                            token: note.clone(),
                            score,
                            available: true,
                        });
                    }
                }
            }
        }
        // Keep top N by score
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out.truncate(20);
        out
    }
}

/// Lightweight debounce helper for popup refresh
#[derive(Debug)]
pub struct PopupDebounce {
    last: Instant,
    interval: Duration,
}

impl PopupDebounce {
    pub fn new(interval: Duration) -> Self {
        Self {
            last: Instant::now(),
            interval,
        }
    }

    pub fn ready(&mut self) -> bool {
        if self.last.elapsed() >= self.interval {
            self.last = Instant::now();
            true
        } else {
            false
        }
    }
}

/// Thread-safe provider that can be updated dynamically (Arc<RwLock> friendly)
#[derive(Default)]
pub struct DynamicPopupProvider {
    inner: parking_lot::RwLock<StaticPopupProvider>,
}

impl DynamicPopupProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> StaticPopupProvider {
        self.inner.read().clone()
    }

    pub fn set_commands(&self, commands: Vec<CommandDescriptor>) {
        self.inner.write().commands = commands;
    }

    pub fn set_agents(&self, agents: Vec<(String, String)>) {
        self.inner.write().agents = agents;
    }

    pub fn set_files(&self, files: Vec<String>) {
        self.inner.write().files = files;
    }

    pub fn set_notes(&self, notes: Vec<String>) {
        self.inner.write().notes = notes;
    }
}

impl PopupProvider for DynamicPopupProvider {
    fn provide(&self, kind: PopupKind, query: &str) -> Vec<PopupItem> {
        let snap = self.inner.read().clone();
        snap.provide(kind, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{PopupItemKind, PopupKind};

    #[test]
    fn test_provider_commands_match_and_sort() {
        let mut provider = StaticPopupProvider::new();
        provider.commands = vec![
            CommandDescriptor {
                name: "search".into(),
                description: "Search".into(),
                input_hint: Some("query".into()),
                secondary_options: vec![],
            },
            CommandDescriptor {
                name: "exit".into(),
                description: "Exit".into(),
                input_hint: None,
                secondary_options: vec![],
            },
        ];

        let items = provider.provide(PopupKind::Command, "ex");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "/exit");
        assert_eq!(items[0].kind, PopupItemKind::Command);
    }

    #[test]
    fn test_provider_agents_files_notes() {
        let mut provider = StaticPopupProvider::new();
        provider.agents = vec![("dev-agent".into(), "Developer".into())];
        provider.files = vec!["src/main.rs".into()];
        provider.notes = vec!["note:project/foo.md".into()];

        let items = provider.provide(PopupKind::AgentOrFile, "dev");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, PopupItemKind::Agent);

        let files = provider.provide(PopupKind::AgentOrFile, "main");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, PopupItemKind::File);

        let notes = provider.provide(PopupKind::AgentOrFile, "foo");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].kind, PopupItemKind::Note);
    }

    #[test]
    fn test_provider_truncates_results() {
        let mut provider = StaticPopupProvider::new();
        for i in 0..25 {
            provider.files.push(format!("file{i}.txt"));
        }
        let items = provider.provide(PopupKind::AgentOrFile, "file");
        assert!(items.len() <= 20);
    }
}
