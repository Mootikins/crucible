//! Shortcuts that map short option names to full config paths.
//!
//! This module provides a registry of shortcuts that allow users to reference
//! configuration options by short, memorable names instead of full dotted paths.
//!
//! # Example
//!
//! ```rust,ignore
//! use crucible_cli::tui::oil::config::shortcuts::ShortcutRegistry;
//!
//! let registry = ShortcutRegistry::new();
//!
//! // Look up a shortcut
//! if let Some(shortcut) = registry.get("theme") {
//!     println!("Maps to: {:?}", shortcut.target);
//! }
//!
//! // Check if something is a shortcut
//! assert!(registry.is_shortcut("model"));
//!
//! // Get the target path (None for Dynamic/Virtual)
//! assert_eq!(
//!     registry.target_path("verbose"),
//!     Some("cli.verbose")
//! );
//! ```

/// Where a shortcut maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutTarget {
    /// Direct mapping to a config path (e.g., "cli.verbose").
    Path(&'static str),
    /// Requires runtime resolution (actual resolution done elsewhere).
    Dynamic,
    /// TUI-only option, not in base config.
    Virtual,
}

/// What completions to show for an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionSource {
    /// Use available models from TUI state.
    Models,
    /// Use available themes.
    Themes,
    /// Use `THINKING_PRESETS.names()`.
    ThinkingPresets,
    /// Fixed list of values.
    Static(&'static [&'static str]),
    /// No completions (bool toggle, free-form input).
    #[default]
    None,
}

/// A shortcut mapping from a short name to a config option.
#[derive(Debug, Clone, Copy)]
pub struct ConfigShortcut {
    /// Short name users type (e.g., "model", "theme").
    pub short: &'static str,
    /// Where this shortcut maps to.
    pub target: ShortcutTarget,
    /// What completions to show for this option.
    pub completions: CompletionSource,
    /// Help text describing the option.
    pub description: &'static str,
}

/// Static array of all registered shortcuts.
pub static SHORTCUTS: &[ConfigShortcut] = &[
    ConfigShortcut {
        short: "model",
        target: ShortcutTarget::Dynamic,
        completions: CompletionSource::Models,
        description: "Current LLM model",
    },
    ConfigShortcut {
        short: "thinking",
        target: ShortcutTarget::Virtual,
        completions: CompletionSource::None,
        description: "Show thinking/reasoning blocks",
    },
    ConfigShortcut {
        short: "thinkingbudget",
        target: ShortcutTarget::Path("llm.thinking_budget"),
        completions: CompletionSource::ThinkingPresets,
        description: "Thinking token budget preset",
    },
    ConfigShortcut {
        short: "theme",
        target: ShortcutTarget::Path("cli.highlighting.theme"),
        completions: CompletionSource::Themes,
        description: "Syntax highlighting theme",
    },
    ConfigShortcut {
        short: "verbose",
        target: ShortcutTarget::Path("cli.verbose"),
        completions: CompletionSource::None,
        description: "Verbose output",
    },
    ConfigShortcut {
        short: "precognition",
        target: ShortcutTarget::Virtual,
        completions: CompletionSource::None,
        description: "Auto-inject knowledge base context (auto-RAG)",
    },
    ConfigShortcut {
        short: "precognition.results",
        target: ShortcutTarget::Virtual,
        completions: CompletionSource::None,
        description: "Number of context results to inject (1-20)",
    },
    // Permission settings (session-scoped, TUI-only)
    ConfigShortcut {
        short: "perm.show_diff",
        target: ShortcutTarget::Virtual,
        completions: CompletionSource::None,
        description: "Show diff by default in permission prompts",
    },
    ConfigShortcut {
        short: "perm.autoconfirm_session",
        target: ShortcutTarget::Virtual,
        completions: CompletionSource::None,
        description: "Auto-allow all permission prompts for session",
    },
];

/// Registry for looking up shortcuts by name.
///
/// Currently wraps the static `SHORTCUTS` array, but designed to allow
/// future extension (e.g., user-defined shortcuts).
#[derive(Debug, Clone, Copy, Default)]
pub struct ShortcutRegistry {
    // Using a unit struct for now; could hold custom shortcuts later
    _private: (),
}

impl ShortcutRegistry {
    /// Create a new shortcut registry.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Find a shortcut by its short name.
    #[must_use]
    pub fn get(&self, short: &str) -> Option<&'static ConfigShortcut> {
        SHORTCUTS.iter().find(|s| s.short == short)
    }

    /// Check if the given key is a registered shortcut.
    #[must_use]
    pub fn is_shortcut(&self, key: &str) -> bool {
        SHORTCUTS.iter().any(|s| s.short == key)
    }

    /// Check if the shortcut is virtual (TUI-only).
    #[must_use]
    pub fn is_virtual(&self, key: &str) -> bool {
        self.get(key)
            .is_some_and(|s| matches!(s.target, ShortcutTarget::Virtual))
    }

    /// Get the target path for a shortcut, if it maps to a direct path.
    ///
    /// Returns `None` for `Dynamic` and `Virtual` shortcuts.
    #[must_use]
    pub fn target_path(&self, key: &str) -> Option<&'static str> {
        self.get(key).and_then(|s| match s.target {
            ShortcutTarget::Path(path) => Some(path),
            ShortcutTarget::Dynamic | ShortcutTarget::Virtual => None,
        })
    }

    /// Get the completion source for a shortcut.
    ///
    /// Returns `CompletionSource::None` if the shortcut doesn't exist.
    #[must_use]
    pub fn completions_for(&self, key: &str) -> CompletionSource {
        self.get(key)
            .map_or(CompletionSource::None, |s| s.completions)
    }

    /// Get the description for a shortcut.
    #[must_use]
    pub fn description(&self, key: &str) -> Option<&'static str> {
        self.get(key).map(|s| s.description)
    }

    /// Iterate over all registered shortcuts.
    pub fn all(&self) -> impl Iterator<Item = &'static ConfigShortcut> {
        SHORTCUTS.iter()
    }

    /// Find the short name for a given config path.
    ///
    /// Only finds shortcuts with `Path` targets.
    #[must_use]
    pub fn reverse_lookup(&self, path: &str) -> Option<&'static str> {
        SHORTCUTS.iter().find_map(|s| match s.target {
            ShortcutTarget::Path(p) if p == path => Some(s.short),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_shortcut_by_name() {
        let registry = ShortcutRegistry::new();

        let model = registry.get("model");
        assert!(model.is_some());
        let model = model.unwrap();
        assert_eq!(model.short, "model");
        assert_eq!(model.target, ShortcutTarget::Dynamic);
        assert_eq!(model.completions, CompletionSource::Models);

        let theme = registry.get("theme");
        assert!(theme.is_some());
        let theme = theme.unwrap();
        assert_eq!(theme.target, ShortcutTarget::Path("cli.highlighting.theme"));

        // Non-existent shortcut
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn is_virtual_detection() {
        let registry = ShortcutRegistry::new();

        // "thinking" is Virtual
        assert!(registry.is_virtual("thinking"));

        // "model" is Dynamic, not Virtual
        assert!(!registry.is_virtual("model"));

        // "theme" is Path, not Virtual
        assert!(!registry.is_virtual("theme"));

        // Non-existent is not Virtual
        assert!(!registry.is_virtual("nonexistent"));
    }

    #[test]
    fn target_path_for_path_vs_dynamic_vs_virtual() {
        let registry = ShortcutRegistry::new();

        // Path target returns the path
        assert_eq!(registry.target_path("verbose"), Some("cli.verbose"));
        assert_eq!(
            registry.target_path("theme"),
            Some("cli.highlighting.theme")
        );
        assert_eq!(
            registry.target_path("thinkingbudget"),
            Some("llm.thinking_budget")
        );

        // Dynamic returns None
        assert_eq!(registry.target_path("model"), None);

        // Virtual returns None
        assert_eq!(registry.target_path("thinking"), None);

        // Non-existent returns None
        assert_eq!(registry.target_path("nonexistent"), None);
    }

    #[test]
    fn reverse_lookup_finds_shortcuts() {
        let registry = ShortcutRegistry::new();

        // Find short name from path
        assert_eq!(registry.reverse_lookup("cli.verbose"), Some("verbose"));
        assert_eq!(
            registry.reverse_lookup("cli.highlighting.theme"),
            Some("theme")
        );
        assert_eq!(
            registry.reverse_lookup("llm.thinking_budget"),
            Some("thinkingbudget")
        );

        // Non-existent path returns None
        assert_eq!(registry.reverse_lookup("nonexistent.path"), None);

        // Dynamic/Virtual targets don't have paths to reverse lookup
        // (they're not in the reverse lookup results)
    }

    #[test]
    fn all_returns_all_shortcuts() {
        let registry = ShortcutRegistry::new();

        let all: Vec<_> = registry.all().collect();

        // Should have all defined shortcuts
        assert_eq!(all.len(), SHORTCUTS.len());
        assert_eq!(all.len(), 9);

        // Verify we have expected shortcuts
        let shorts: Vec<_> = all.iter().map(|s| s.short).collect();
        assert!(shorts.contains(&"model"));
        assert!(shorts.contains(&"thinking"));
        assert!(shorts.contains(&"thinkingbudget"));
        assert!(shorts.contains(&"theme"));
        assert!(shorts.contains(&"verbose"));
        assert!(shorts.contains(&"precognition"));
        assert!(shorts.contains(&"precognition.results"));
        assert!(shorts.contains(&"perm.show_diff"));
        assert!(shorts.contains(&"perm.autoconfirm_session"));
    }

    #[test]
    fn is_shortcut_check() {
        let registry = ShortcutRegistry::new();

        assert!(registry.is_shortcut("model"));
        assert!(registry.is_shortcut("theme"));
        assert!(registry.is_shortcut("verbose"));
        assert!(!registry.is_shortcut("nonexistent"));
        assert!(!registry.is_shortcut("cli.verbose")); // Full path, not shortcut
    }

    #[test]
    fn completions_for_returns_correct_source() {
        let registry = ShortcutRegistry::new();

        assert_eq!(registry.completions_for("model"), CompletionSource::Models);
        assert_eq!(registry.completions_for("theme"), CompletionSource::Themes);
        assert_eq!(
            registry.completions_for("thinkingbudget"),
            CompletionSource::ThinkingPresets
        );
        assert_eq!(registry.completions_for("verbose"), CompletionSource::None);
        assert_eq!(registry.completions_for("thinking"), CompletionSource::None);
        // Non-existent defaults to None
        assert_eq!(
            registry.completions_for("nonexistent"),
            CompletionSource::None
        );
    }

    #[test]
    fn description_returns_help_text() {
        let registry = ShortcutRegistry::new();

        assert_eq!(registry.description("model"), Some("Current LLM model"));
        assert_eq!(
            registry.description("theme"),
            Some("Syntax highlighting theme")
        );
        assert_eq!(registry.description("nonexistent"), None);
    }

    #[test]
    fn shortcut_registry_is_copy() {
        let registry = ShortcutRegistry::new();
        let _copy = registry;
        // If this compiles, registry implements Copy
        let _ = registry;
    }

    #[test]
    fn shortcut_target_is_copy() {
        let target = ShortcutTarget::Dynamic;
        let _copy = target;
        // If this compiles, ShortcutTarget implements Copy
        let _ = target;
    }

    #[test]
    fn completion_source_is_copy() {
        let source = CompletionSource::Models;
        let _copy = source;
        // If this compiles, CompletionSource implements Copy
        let _ = source;
    }
}
