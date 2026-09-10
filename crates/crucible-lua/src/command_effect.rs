//! What a command does to state a user could lose.
//!
//! A plugin command is a callable a *person* can reach — the web reaches one
//! over `POST /api/plugins/command`, the TUI over `plugin.run_command`. Until
//! now every command looked the same from outside: `kanban_board` reads a
//! folder and `kanban_move` rewrites a file, and nothing on the wire told them
//! apart. A caller that cannot tell them apart cannot offer one as a button it
//! presses for you and hold the other behind a question.
//!
//! ## The line this draws
//!
//! [`CommandEffect::Read`] means **the command changes nothing a user could
//! lose**. It may compute, it may cache, and it may publish derived state —
//! `kanban`'s republish re-derives the board from the files that already exist
//! and is a read by this definition. It may not write a note, a file, or a
//! configuration value.
//!
//! Everything else is [`CommandEffect::Write`].
//!
//! ## It is a disclosure, not a gate
//!
//! The plugin declares this about itself and nothing verifies it. That is the
//! same position Obsidian's May 2026 capability disclosures occupy, and
//! `docs/Meta/Analysis/Plugin API Plan.md` step 4 is explicit that a label
//! without a gate teaches a user to trust a promise nothing keeps. So a
//! consumer must present it as *declared*, and a permission layer must treat a
//! `Read` claim as a hint about what to ask, never as permission to skip
//! asking.
//!
//! ## Why an absent declaration is a write
//!
//! Every command that shipped before this existed declares nothing, and the two
//! possible defaults fail differently. Defaulting to `Read` advertises an
//! undeclared write as safe to invoke speculatively, and the cost is a file
//! changed that nobody asked to change. Defaulting to `Write` asks about an
//! undeclared read, and the cost is one question. So: absent means
//! [`CommandEffect::Write`], and the type carries no `Default` impl — the
//! choice is made once, where the absence is observed, with this reason beside
//! it.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use std::fmt;

/// What a command does to state a user could lose. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum CommandEffect {
    /// Changes nothing a user could lose. May compute, cache and publish.
    Read,
    /// Changes something a user could lose: a note, a file, a setting.
    Write,
}

impl CommandEffect {
    /// The wire spelling, and the text a plugin declares.
    ///
    /// No wildcard arm: a new variant fails to compile here rather than
    /// reaching a client under a name it has never been told about.
    pub fn as_str(self) -> &'static str {
        match self {
            CommandEffect::Read => "read",
            CommandEffect::Write => "write",
        }
    }

    /// Read a declared effect. `None` for text that names no variant.
    ///
    /// Derived from [`Self::ALL`] rather than from a second match, so a
    /// variant that is spelled by `as_str` is parseable by construction.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|effect| effect.as_str() == text)
    }

    /// Every variant. `every_variant_is_listed` proves this is complete.
    pub const ALL: [CommandEffect; 2] = [CommandEffect::Read, CommandEffect::Write];

    /// The declared spellings, for an error message that offers the options.
    pub fn declarable() -> String {
        Self::ALL
            .iter()
            .map(|effect| effect.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for CommandEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn every_variant_is_listed() {
        let iterated: Vec<CommandEffect> = CommandEffect::iter().collect();
        assert_eq!(
            iterated.as_slice(),
            CommandEffect::ALL.as_slice(),
            "CommandEffect::ALL must hold every variant, in declaration order"
        );
    }

    #[test]
    fn every_variant_round_trips_through_its_wire_name() {
        for effect in CommandEffect::iter() {
            assert_eq!(
                CommandEffect::parse(effect.as_str()),
                Some(effect),
                "{effect} must parse back from the name it is written as"
            );
        }
    }

    #[test]
    fn unknown_text_names_no_variant() {
        assert_eq!(CommandEffect::parse("reed"), None);
        assert_eq!(CommandEffect::parse(""), None);
        assert_eq!(CommandEffect::parse("READ"), None);
    }
}
