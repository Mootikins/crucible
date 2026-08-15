use crate::tui::oil::theme;
use crucible_oil::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

// Modes are declared in Lua (`cru.modes.review = { … }`), so the TUI cannot
// know their names at compile time. It holds the id the daemon gave it and
// derives presentation from that; the alternative — an enum with a `Custom`
// arm — makes `Custom("plan") != Plan` a live hazard at every construction
// site that skips the parse function.

/// The mode a session shows before the daemon has said otherwise.
pub const DEFAULT_MODE: &str = "normal";

/// The modes to offer before `session.list_modes` answers.
///
/// Not a claim about what exists — the daemon's list replaces this wholesale
/// the moment it arrives. It exists so `/mode` and Shift+Tab work during the
/// first frames, and so a TUI driven by a mock agent (every unit test) still
/// cycles.
pub const DEFAULT_MODES: [&str; 3] = ["normal", "plan", "auto"];

/// The statusline badge for a mode: ` NORMAL `, ` PLAN `, ` REVIEW `.
///
/// Derived rather than matched so a mode the TUI has never heard of still gets
/// its own badge. The built-ins reproduce their previous labels byte-for-byte,
/// which is what keeps the statusline snapshots from moving.
pub fn mode_label(mode: &str) -> String {
    format!(" {} ", mode.to_uppercase())
}

/// The badge's colours.
///
/// Only the built-in ids are themed. A Lua-declared mode borrows the normal
/// colour rather than going unstyled — until `cru.modes.review = { color = … }`
/// is wired through, a neutral badge beats no badge.
pub fn mode_style(mode: &str) -> crucible_oil::style::Style {
    use crucible_oil::style::{AdaptiveColor, Style};

    let t = theme::active();
    let bg = |c: AdaptiveColor| if t.is_dark { c.dark } else { c.light };
    let color = match mode {
        "plan" => t.colors.mode_plan,
        "auto" => t.colors.mode_auto,
        _ => t.colors.mode_normal,
    };
    Style::new().bg(bg(color)).fg(Color::Black).bold()
}

/// The next mode in `available`, wrapping.
///
/// `available` is what `session.list_modes` returned. An empty list, or a
/// current mode the daemon no longer offers, cycles nowhere: advancing into a
/// mode `set_mode` would reject is worse than leaving the mode alone.
pub fn next_mode(current: &str, available: &[String]) -> Option<std::sync::Arc<str>> {
    let idx = available
        .iter()
        .position(|m| m.eq_ignore_ascii_case(current))?;
    available
        .get((idx + 1) % available.len())
        .map(|m| m.as_str().into())
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AutocompleteKind {
    #[default]
    None,
    File,
    Note,
    Command,
    SlashCommand,
    ReplCommand,
    Model,
    CommandArg {
        command: String,
        arg_index: usize,
    },
    SetOption {
        option: Option<String>,
    },
    /// Full-screen picker opened by `:pick [source]`.
    Pick {
        source: PickSource,
    },
}

/// Source category for the `:pick` command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PickSource {
    #[default]
    All,
    Notes,
    Commands,
    Files,
}

/// Message queue state — message counter and Ctrl-C tracking
#[derive(Default)]
pub(crate) struct MessageQueueState {
    /// Monotonic counter for assigning message IDs
    pub message_counter: usize,
    /// Timestamp of the last Ctrl-C press (for double-tap quit)
    pub last_ctrl_c: Option<std::time::Instant>,
}
