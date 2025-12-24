use crate::tui::state::{PopupItem, PopupItemKind, PopupKind};
use crucible_core::traits::chat::CommandDescriptor;
use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Matcher, Nucleo, Utf32String};
use std::sync::atomic::{AtomicU64, Ordering};
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
    pub skills: Vec<(String, String, String)>, // (name, description, scope)
}

impl StaticPopupProvider {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PopupProvider for StaticPopupProvider {
    fn provide(&self, kind: PopupKind, query: &str) -> Vec<PopupItem> {
        let query = query.split_whitespace().next().unwrap_or("").trim();
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let mut pattern = nucleo::pattern::MultiPattern::new(1);
        pattern.reparse(0, query, CaseMatching::Ignore, Normalization::Smart, false);

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
                    let match_col = Utf32String::from(cmd.name.as_str());
                    let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
                    if let Some(score) = score {
                        out.push(PopupItem {
                            kind: PopupItemKind::Command,
                            title,
                            subtitle,
                            token: format!("/{} ", cmd.name),
                            score: score.min(i32::MAX as u32) as i32,
                            available: true,
                        });
                    }
                }

                // Also show skills in command popup
                for (name, description, scope) in &self.skills {
                    let title = format!("skill:{}", name);
                    let subtitle = format!("{} ({})", description, scope);
                    let match_col = Utf32String::from(name.as_str());
                    let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
                    if let Some(score) = score {
                        out.push(PopupItem {
                            kind: PopupItemKind::Skill,
                            title,
                            subtitle,
                            token: format!("skill:{} ", name),
                            score: score.min(i32::MAX as u32) as i32,
                            available: true,
                        });
                    }
                }
            }
            PopupKind::AgentOrFile => {
                for (id, desc) in &self.agents {
                    let match_col = Utf32String::from(id.as_str());
                    let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
                    if let Some(score) = score {
                        out.push(PopupItem {
                            kind: PopupItemKind::Agent,
                            title: format!("@{}", id),
                            subtitle: desc.clone(),
                            token: format!("@{}", id),
                            score: score.min(i32::MAX as u32) as i32,
                            available: true,
                        });
                    }
                }
                for file in &self.files {
                    let match_col = Utf32String::from(file.as_str());
                    let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
                    if let Some(score) = score {
                        out.push(PopupItem {
                            kind: PopupItemKind::File,
                            title: file.clone(),
                            subtitle: String::from("workspace"),
                            token: file.clone(),
                            score: score.min(i32::MAX as u32) as i32,
                            available: true,
                        });
                    }
                }
                for note in &self.notes {
                    let match_col = Utf32String::from(note.as_str());
                    let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
                    if let Some(score) = score {
                        out.push(PopupItem {
                            kind: PopupItemKind::Note,
                            title: note.clone(),
                            subtitle: String::from("note"),
                            token: note.clone(),
                            score: score.min(i32::MAX as u32) as i32,
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

#[derive(Debug, Clone)]
struct PopupCandidate {
    kind: PopupItemKind,
    title: String,
    subtitle: String,
    token: String,
    available: bool,
    match_col: Utf32String,
}

impl PopupCandidate {
    fn to_item(&self, score: u32) -> PopupItem {
        PopupItem {
            kind: self.kind.clone(),
            title: self.title.clone(),
            subtitle: self.subtitle.clone(),
            token: self.token.clone(),
            score: score.min(i32::MAX as u32) as i32,
            available: self.available,
        }
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
pub struct DynamicPopupProvider {
    inner: parking_lot::RwLock<StaticPopupProvider>,
    commands_version: AtomicU64,
    agents_version: AtomicU64,
    files_version: AtomicU64,
    notes_version: AtomicU64,
    skills_version: AtomicU64,
    command_matcher: parking_lot::Mutex<PopupMatcherCache>,
    agent_file_matcher: parking_lot::Mutex<PopupMatcherCache>,
}

impl Default for DynamicPopupProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicPopupProvider {
    pub fn new() -> Self {
        let notify: Arc<dyn Fn() + Sync + Send> = Arc::new(|| {});
        let config_commands = Config::DEFAULT;
        let config_paths = Config::DEFAULT.match_paths();

        Self {
            inner: parking_lot::RwLock::new(StaticPopupProvider::default()),
            commands_version: AtomicU64::new(1),
            agents_version: AtomicU64::new(1),
            files_version: AtomicU64::new(1),
            notes_version: AtomicU64::new(1),
            skills_version: AtomicU64::new(1),
            command_matcher: parking_lot::Mutex::new(PopupMatcherCache::new(
                Nucleo::new(config_commands.clone(), notify.clone(), None, 1),
                Matcher::new(config_commands),
            )),
            agent_file_matcher: parking_lot::Mutex::new(PopupMatcherCache::new(
                Nucleo::new(config_paths.clone(), notify, None, 1),
                Matcher::new(config_paths),
            )),
        }
    }

    pub fn snapshot(&self) -> StaticPopupProvider {
        self.inner.read().clone()
    }

    pub fn set_commands(&self, commands: Vec<CommandDescriptor>) {
        self.inner.write().commands = commands;
        self.commands_version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_agents(&self, agents: Vec<(String, String)>) {
        self.inner.write().agents = agents;
        self.agents_version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_files(&self, files: Vec<String>) {
        self.inner.write().files = files;
        self.files_version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_notes(&self, notes: Vec<String>) {
        self.inner.write().notes = notes;
        self.notes_version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_skills(&self, skills: Vec<(String, String, String)>) {
        self.inner.write().skills = skills;
        self.skills_version.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheKind {
    Command,
    AgentOrFile,
}

struct PopupMatcherCache {
    nucleo: Nucleo<PopupCandidate>,
    score_matcher: Matcher,
    last_query: String,
    last_versions: (u64, u64, u64, u64, u64), // commands, agents, files, notes, skills
}

impl PopupMatcherCache {
    fn new(nucleo: Nucleo<PopupCandidate>, score_matcher: Matcher) -> Self {
        Self {
            nucleo,
            score_matcher,
            last_query: String::new(),
            last_versions: (0, 0, 0, 0, 0),
        }
    }

    fn needs_rebuild(&self, kind: CacheKind, versions: (u64, u64, u64, u64, u64)) -> bool {
        match kind {
            CacheKind::Command => {
                versions.0 != self.last_versions.0 || versions.4 != self.last_versions.4
            }
            CacheKind::AgentOrFile => {
                versions.1 != self.last_versions.1
                    || versions.2 != self.last_versions.2
                    || versions.3 != self.last_versions.3
            }
        }
    }

    fn maybe_rebuild(
        &mut self,
        kind: CacheKind,
        versions: (u64, u64, u64, u64, u64),
        data: StaticPopupProvider,
    ) {
        if !self.needs_rebuild(kind, versions) {
            return;
        }

        self.last_versions = versions;
        self.last_query.clear();
        self.nucleo.restart(true);
        let injector = self.nucleo.injector();

        match kind {
            CacheKind::Command => {
                for cmd in data.commands {
                    let name = cmd.name;
                    let description = cmd.description;
                    let input_hint = cmd.input_hint;
                    let title = format!("/{}", name);
                    let subtitle = input_hint
                        .as_ref()
                        .map(|h| format!("{} — {}", description, h))
                        .unwrap_or_else(|| description.clone());
                    let cand = PopupCandidate {
                        kind: PopupItemKind::Command,
                        match_col: Utf32String::from(name.as_str()),
                        title,
                        subtitle,
                        token: format!("/{} ", name),
                        available: true,
                    };
                    injector.push(cand, |c, cols| cols[0] = c.match_col.clone());
                }

                // Include skills in command cache
                for (name, description, scope) in data.skills {
                    let title = format!("skill:{}", name);
                    let subtitle = format!("{} ({})", description, scope);
                    let cand = PopupCandidate {
                        kind: PopupItemKind::Skill,
                        match_col: Utf32String::from(name.as_str()),
                        title,
                        subtitle,
                        token: format!("skill:{} ", name),
                        available: true,
                    };
                    injector.push(cand, |c, cols| cols[0] = c.match_col.clone());
                }
            }
            CacheKind::AgentOrFile => {
                for (id, desc) in data.agents {
                    let cand = PopupCandidate {
                        kind: PopupItemKind::Agent,
                        match_col: Utf32String::from(id.as_str()),
                        title: format!("@{}", id),
                        subtitle: desc,
                        token: format!("@{}", id),
                        available: true,
                    };
                    injector.push(cand, |c, cols| cols[0] = c.match_col.clone());
                }
                for file in data.files {
                    let cand = PopupCandidate {
                        kind: PopupItemKind::File,
                        match_col: Utf32String::from(file.as_str()),
                        title: file.clone(),
                        subtitle: String::from("workspace"),
                        token: file,
                        available: true,
                    };
                    injector.push(cand, |c, cols| cols[0] = c.match_col.clone());
                }
                for note in data.notes {
                    let cand = PopupCandidate {
                        kind: PopupItemKind::Note,
                        match_col: Utf32String::from(note.as_str()),
                        title: note.clone(),
                        subtitle: String::from("note"),
                        token: note,
                        available: true,
                    };
                    injector.push(cand, |c, cols| cols[0] = c.match_col.clone());
                }
            }
        }
    }

    fn provide(&mut self, query: &str) -> Vec<PopupItem> {
        let query = query.trim();
        let append = !self.last_query.is_empty()
            && query.len() >= self.last_query.len()
            && query.starts_with(&self.last_query);
        self.nucleo
            .pattern
            .reparse(0, query, CaseMatching::Ignore, Normalization::Smart, append);
        self.last_query.clear();
        self.last_query.push_str(query);

        // Wait a bit for the worker to finish; if it's still running we'll use the last snapshot.
        let _ = self.nucleo.tick(10);
        let snap = self.nucleo.snapshot();
        let pat = snap.pattern();

        let limit = 20u32;
        let end = snap.matched_item_count().min(limit);
        let mut out = Vec::new();
        for item in snap.matched_items(0..end) {
            let score = pat
                .score(item.matcher_columns, &mut self.score_matcher)
                .unwrap_or(0);
            out.push(item.data.to_item(score));
        }
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out.truncate(20);
        out
    }
}

impl PopupProvider for DynamicPopupProvider {
    fn provide(&self, kind: PopupKind, query: &str) -> Vec<PopupItem> {
        let query = query.split_whitespace().next().unwrap_or("").trim();
        let versions = (
            self.commands_version.load(Ordering::Relaxed),
            self.agents_version.load(Ordering::Relaxed),
            self.files_version.load(Ordering::Relaxed),
            self.notes_version.load(Ordering::Relaxed),
            self.skills_version.load(Ordering::Relaxed),
        );

        match kind {
            PopupKind::Command => {
                let mut cache = self.command_matcher.lock();
                if cache.needs_rebuild(CacheKind::Command, versions) {
                    let data = self.inner.read().clone();
                    cache.maybe_rebuild(CacheKind::Command, versions, data);
                }
                cache.provide(query)
            }
            PopupKind::AgentOrFile => {
                let mut cache = self.agent_file_matcher.lock();
                if cache.needs_rebuild(CacheKind::AgentOrFile, versions) {
                    let data = self.inner.read().clone();
                    cache.maybe_rebuild(CacheKind::AgentOrFile, versions, data);
                }
                cache.provide(query)
            }
        }
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
    fn test_provider_commands_fuzzy_subsequence_match() {
        let mut provider = StaticPopupProvider::new();
        provider.commands = vec![CommandDescriptor {
            name: "search".into(),
            description: "Search".into(),
            input_hint: None,
            secondary_options: vec![],
        }];

        let items = provider.provide(PopupKind::Command, "srch");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "/search");
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
    fn test_provider_files_fuzzy_subsequence_match() {
        let mut provider = StaticPopupProvider::new();
        provider.files = vec!["src/main.rs".into()];

        let items = provider.provide(PopupKind::AgentOrFile, "srs");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, PopupItemKind::File);
        assert_eq!(items[0].title, "src/main.rs");
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
#[cfg(test)]
mod skill_popup_tests {
    use super::{PopupProvider, StaticPopupProvider};
    use crate::tui::state::{PopupItemKind, PopupKind};

    #[test]
    fn test_skills_appear_in_command_popup() {
        let mut provider = StaticPopupProvider::new();
        provider.skills = vec![
            (
                "git-commit".into(),
                "Create git commits".into(),
                "user".into(),
            ),
            (
                "code-review".into(),
                "Review code quality".into(),
                "user".into(),
            ),
        ];

        let items = provider.provide(PopupKind::Command, "git");

        // Should find the git-commit skill
        assert!(
            items.iter().any(|item| {
                item.kind == PopupItemKind::Skill && item.title == "skill:git-commit"
            }),
            "git-commit skill should appear in results"
        );

        // Verify token format
        let git_skill = items
            .iter()
            .find(|item| item.kind == PopupItemKind::Skill && item.title == "skill:git-commit")
            .expect("git-commit skill should exist");

        assert_eq!(git_skill.token, "skill:git-commit ");
        assert!(git_skill.subtitle.contains("Create git commits"));
        assert!(git_skill.subtitle.contains("(user)"));
    }

    #[test]
    fn test_skills_fuzzy_match() {
        let mut provider = StaticPopupProvider::new();
        provider.skills = vec![("code-review".into(), "Review code".into(), "user".into())];

        let items = provider.provide(PopupKind::Command, "crvw");

        // Fuzzy match should find code-review
        assert!(
            items.iter().any(|item| {
                item.kind == PopupItemKind::Skill && item.title == "skill:code-review"
            }),
            "Should fuzzy match code-review with 'crvw'"
        );
    }
}

/// Get list of known slash commands
///
/// Returns the canonical list of client-handled slash commands.
/// These match the RESERVED_COMMANDS list in slash_registry.rs.
pub fn get_known_commands() -> &'static [&'static str] {
    &["help", "mode", "clear", "exit", "quit", "search", "context"]
}

/// Extract the command name from input (without the leading /)
///
/// Returns None if input doesn't start with /
///
/// # Examples
///
/// ```rust,ignore
/// assert_eq!(extract_command_name("/help foo"), Some("help"));
/// assert_eq!(extract_command_name("/mode"), Some("mode"));
/// assert_eq!(extract_command_name("not a command"), None);
/// ```
pub fn extract_command_name(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let without_slash = &trimmed[1..];
    // Get first word (command name)
    without_slash.split_whitespace().next()
}

/// Check if input is an exact match for a known slash command
///
/// Returns true if the command name (first word after /) matches a known command.
/// This includes commands with trailing spaces or arguments.
///
/// # Examples
///
/// ```rust,ignore
/// assert!(is_exact_slash_command("/help"));
/// assert!(is_exact_slash_command("/help "));
/// assert!(is_exact_slash_command("/help foo"));
/// assert!(!is_exact_slash_command("/hel"));  // partial
/// assert!(!is_exact_slash_command("/foobar"));  // unknown
/// ```
pub fn is_exact_slash_command(input: &str) -> bool {
    if let Some(cmd_name) = extract_command_name(input) {
        get_known_commands().contains(&cmd_name)
    } else {
        false
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn test_is_exact_command_help() {
        // `/help` should return true (exact match)
        assert!(is_exact_slash_command("/help"));
    }

    #[test]
    fn test_is_exact_command_with_space() {
        // `/help ` (with trailing space) should return true
        assert!(is_exact_slash_command("/help "));
    }

    #[test]
    fn test_is_exact_command_with_args() {
        // `/help foo` should return true (command exists, has args)
        assert!(is_exact_slash_command("/help foo"));
    }

    #[test]
    fn test_is_partial_command() {
        // `/hel` should return false (partial match, not exact)
        assert!(!is_exact_slash_command("/hel"));
    }

    #[test]
    fn test_is_unknown_command() {
        // `/foobar` should return false (not a known command)
        assert!(!is_exact_slash_command("/foobar"));
    }

    #[test]
    fn test_extract_command_name() {
        // `/help foo bar` should extract "help"
        assert_eq!(extract_command_name("/help foo bar"), Some("help"));
        assert_eq!(extract_command_name("/mode"), Some("mode"));
        assert_eq!(extract_command_name("not a command"), None);
    }
}
