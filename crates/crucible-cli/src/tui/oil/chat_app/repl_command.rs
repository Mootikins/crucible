//! The one table of built-in REPL (`:`) commands.
//!
//! Four lists used to describe the same commands by hand: the typo suggester,
//! the `:` autocomplete popup, the `:pick commands` source and `:help
//! commands`. They drifted. Every reader now derives its list from
//! [`ReplCommand::ALL`], and a test walks `strum::EnumIter` to prove that
//! `ALL` names every variant the compiler knows about.
//!
//! Dispatch stays in `command_handling.rs`: a command with arguments needs its
//! own parse, and a table cannot express that. A new command is one variant
//! here plus one match arm there; the first is a compile error when the
//! exhaustive matches below lack it.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

/// A built-in REPL command the chat app handles itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub(super) enum ReplCommand {
    Quit,
    Help,
    Clear,
    Undo,
    Model,
    Set,
    Export,
    Messages,
    Palette,
    Pick,
    Mcp,
    Plugins,
    Reload,
    Config,
    Lua,
}

impl ReplCommand {
    /// Every command, in the order the popup and `:help` list them.
    pub(super) const ALL: &'static [ReplCommand] = &[
        Self::Quit,
        Self::Help,
        Self::Clear,
        Self::Undo,
        Self::Model,
        Self::Set,
        Self::Export,
        Self::Messages,
        Self::Palette,
        Self::Pick,
        Self::Mcp,
        Self::Plugins,
        Self::Reload,
        Self::Config,
        Self::Lua,
    ];

    /// The word after `:` that runs this command.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Help => "help",
            Self::Clear => "clear",
            Self::Undo => "undo",
            Self::Model => "model",
            Self::Set => "set",
            Self::Export => "export",
            Self::Messages => "messages",
            Self::Palette => "palette",
            Self::Pick => "pick",
            Self::Mcp => "mcp",
            Self::Plugins => "plugins",
            Self::Reload => "reload",
            Self::Config => "config",
            Self::Lua => "lua",
        }
    }

    /// Other words that run the same command.
    pub(super) fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Quit => &["q"],
            Self::Help => &["h"],
            Self::Messages => &["msgs", "notifications"],
            Self::Palette => &["commands"],
            Self::Clear
            | Self::Undo
            | Self::Model
            | Self::Set
            | Self::Export
            | Self::Pick
            | Self::Mcp
            | Self::Plugins
            | Self::Reload
            | Self::Config
            | Self::Lua => &[],
        }
    }

    /// The command with its arguments, as `:help commands` shows it.
    pub(super) fn usage(self) -> &'static str {
        match self {
            Self::Quit => ":quit, :q",
            Self::Help => ":help [topic]",
            Self::Clear => ":clear",
            Self::Undo => ":undo [N]",
            Self::Model => ":model <name>",
            Self::Set => ":set <opt>",
            Self::Export => ":export <path>",
            Self::Messages => ":messages",
            Self::Palette => ":palette",
            Self::Pick => ":pick [source]",
            Self::Mcp => ":mcp",
            Self::Plugins => ":plugins",
            Self::Reload => ":reload <name>",
            Self::Config => ":config",
            Self::Lua => ":lua <expr>",
        }
    }

    /// One line that says what the command does.
    pub(super) fn description(self) -> &'static str {
        match self {
            Self::Quit => "Exit chat",
            Self::Help => "Show help",
            Self::Clear => "Clear conversation history",
            Self::Undo => "Undo the last N agent turns (default 1)",
            Self::Model => "Switch model (or list available)",
            Self::Set => "View/modify runtime options (e.g. :set thinkingbudget=high)",
            Self::Export => "Export session to markdown",
            Self::Messages => "Toggle notification drawer",
            Self::Palette => "Open command palette (F1)",
            Self::Pick => "Fuzzy picker (notes, files, commands)",
            Self::Mcp => "Show MCP server status",
            Self::Plugins => "Show loaded plugins",
            Self::Reload => "Reload plugin(s)",
            Self::Config => "Show current configuration",
            Self::Lua => "Evaluate Lua (daemon-side; := shorthand)",
        }
    }

    /// The popup `kind` tag, which picks the row's colour.
    pub(super) fn kind(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Quit
            | Self::Help
            | Self::Clear
            | Self::Undo
            | Self::Model
            | Self::Set
            | Self::Export
            | Self::Messages
            | Self::Palette
            | Self::Pick
            | Self::Plugins
            | Self::Reload
            | Self::Config
            | Self::Lua => "core",
        }
    }

    /// The popup row: `(":name", description, kind)`.
    pub(super) fn popup_entry(self) -> (&'static str, &'static str, &'static str) {
        (self.label(), self.description(), self.kind())
    }

    /// The command with its `:` prefix, as the popup shows it.
    fn label(self) -> &'static str {
        match self {
            Self::Quit => ":quit",
            Self::Help => ":help",
            Self::Clear => ":clear",
            Self::Undo => ":undo",
            Self::Model => ":model",
            Self::Set => ":set",
            Self::Export => ":export",
            Self::Messages => ":messages",
            Self::Palette => ":palette",
            Self::Pick => ":pick",
            Self::Mcp => ":mcp",
            Self::Plugins => ":plugins",
            Self::Reload => ":reload",
            Self::Config => ":config",
            Self::Lua => ":lua",
        }
    }

    /// Every popup row, in table order.
    pub(super) fn popup_entries() -> Vec<(&'static str, &'static str, &'static str)> {
        Self::ALL.iter().map(|c| c.popup_entry()).collect()
    }

    /// Every name and alias, for typo suggestions.
    pub(super) fn known_words() -> Vec<&'static str> {
        Self::ALL
            .iter()
            .flat_map(|c| std::iter::once(c.name()).chain(c.aliases().iter().copied()))
            .collect()
    }

    /// The body of `:help commands`, one aligned line per command.
    pub(super) fn help_lines() -> String {
        let width = Self::ALL.iter().map(|c| c.usage().len()).max().unwrap_or(0);
        Self::ALL
            .iter()
            .map(|c| format!("{:<width$} — {}", c.usage(), c.description()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::ReplCommand;
    use strum::IntoEnumIterator;

    /// `ALL` is the list every reader walks, so a variant absent from it is a
    /// command no popup, help text or suggester knows.
    #[test]
    fn all_names_every_variant_once() {
        let from_compiler: Vec<ReplCommand> = ReplCommand::iter().collect();
        let mut from_table = ReplCommand::ALL.to_vec();
        from_table.sort_by_key(|c| c.name());
        let mut expected = from_compiler.clone();
        expected.sort_by_key(|c| c.name());
        assert_eq!(from_table, expected);
        assert_eq!(ReplCommand::ALL.len(), from_compiler.len());
    }

    #[test]
    fn labels_are_the_names_with_a_colon() {
        for cmd in ReplCommand::iter() {
            assert_eq!(cmd.label(), format!(":{}", cmd.name()));
            assert!(cmd.usage().starts_with(cmd.label()), "{:?}", cmd);
        }
    }

    #[test]
    fn help_text_lists_every_command() {
        let help = ReplCommand::help_lines();
        for cmd in ReplCommand::iter() {
            assert!(help.contains(cmd.usage()), "{:?} missing from help", cmd);
            assert!(
                help.contains(cmd.description()),
                "{:?} missing from help",
                cmd
            );
        }
    }

    #[test]
    fn known_words_hold_every_name_and_alias() {
        let words = ReplCommand::known_words();
        for cmd in ReplCommand::iter() {
            assert!(words.contains(&cmd.name()));
            for alias in cmd.aliases() {
                assert!(words.contains(alias));
            }
        }
        let mut dedup = words.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(dedup.len(), words.len(), "duplicate word in the table");
    }
}
