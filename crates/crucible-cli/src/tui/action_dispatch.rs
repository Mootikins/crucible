//! Action dispatch for TUI runner
//!
//! This module translates `TuiAction` from the ChatView into effects
//! that the runner can execute. This keeps the runner thin and focused
//! on coordination rather than decision-making.

use crate::tui::event_result::TuiAction;
use crate::tui::state::PopupItem;

// =============================================================================
// Popup Effect (what happens when a popup item is confirmed)
// =============================================================================

/// Effect to apply when a popup item is confirmed
///
/// Scripts can override the default behavior via `PopupHook`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupEffect {
    /// Insert token into input (Commands, Skills, Agents)
    InsertToken { token: String },
    /// Add file to pending context
    AddFileContext { path: String },
    /// Add note to pending context
    AddNoteContext { path: String },
    /// Execute a REPL command immediately
    ExecuteReplCommand { name: String },
    /// Resume a session by ID
    ResumeSession { session_id: String },
}

/// Convert a PopupItem to its default effect
///
/// This is the default behavior that can be overridden by scripts.
pub fn popup_item_to_effect(item: &PopupItem) -> PopupEffect {
    match item {
        PopupItem::Command { name, .. } => PopupEffect::InsertToken {
            token: format!("/{} ", name),
        },
        PopupItem::Agent { id, .. } => PopupEffect::InsertToken {
            token: format!("@{}", id), // Just insert token for now
        },
        PopupItem::File { path, .. } => PopupEffect::AddFileContext { path: path.clone() },
        PopupItem::Note { path, .. } => PopupEffect::AddNoteContext { path: path.clone() },
        PopupItem::Skill { name, .. } => PopupEffect::InsertToken {
            token: format!("/{} ", name),
        },
        PopupItem::ReplCommand { name, .. } => {
            PopupEffect::ExecuteReplCommand { name: name.clone() }
        }
        PopupItem::Session { id, .. } => PopupEffect::ResumeSession {
            session_id: id.clone(),
        },
    }
}

// =============================================================================
// Scriptable Hook Point (for Rune/Steel/Lua)
// =============================================================================

/// Hook called when popup item is selected, before default dispatch
///
/// Scripts can return a custom PopupEffect or None for default behavior.
pub trait PopupHook: Send + Sync {
    /// Called when a popup item is selected
    ///
    /// Return `Some(effect)` to override default behavior, `None` to use default.
    fn on_popup_select(&self, item: &PopupItem) -> Option<PopupEffect>;
}

/// Registry for popup hooks (populated from Rune/Steel/Lua scripts)
#[derive(Default)]
pub struct PopupHooks {
    hooks: Vec<Box<dyn PopupHook>>,
}

impl PopupHooks {
    /// Create empty hook registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a popup hook
    pub fn register(&mut self, hook: Box<dyn PopupHook>) {
        self.hooks.push(hook);
    }

    /// Dispatch a popup selection through hooks, falling back to default
    pub fn dispatch(&self, item: &PopupItem) -> PopupEffect {
        // Try hooks first, fall back to default
        for hook in &self.hooks {
            if let Some(effect) = hook.on_popup_select(item) {
                return effect;
            }
        }
        popup_item_to_effect(item)
    }
}

// =============================================================================
// Context Resolver (for scriptable content injection at send time)
// =============================================================================

/// Hook for resolving context attachment content at send time
///
/// Scripts can override how file/note content is loaded.
pub trait ContextResolver: Send + Sync {
    /// Resolve a file attachment to its content
    fn resolve_file(&self, path: &str) -> Option<String>;
    /// Resolve a note attachment to its content
    fn resolve_note(&self, path: &str) -> Option<String>;
}

/// Default resolver that reads files from filesystem
pub struct DefaultContextResolver;

impl ContextResolver for DefaultContextResolver {
    fn resolve_file(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn resolve_note(&self, _path: &str) -> Option<String> {
        // Notes are in kiln, resolve via storage client
        // This is a placeholder - actual impl would use storage
        None
    }
}

/// Effects that the runner should execute
///
/// These are the side effects that require runner coordination,
/// such as starting async operations or exiting the application.
#[derive(Debug, Clone, PartialEq)]
pub enum RunnerEffect {
    /// Exit the main loop
    Exit,

    /// Cancel current operation (streaming or clear input)
    Cancel,

    /// Start sending a message (initiates streaming)
    SendMessage(String),

    /// Execute a slash command
    ExecuteCommand(String),

    /// Cycle through session modes
    CycleMode,

    /// Scroll the conversation view
    Scroll(ScrollEffect),

    /// Apply popup selection to input
    ApplyPopupSelection(PopupItem),

    /// Handle dialog result
    DialogResult(DialogEffect),

    /// Just render (no side effect needed)
    Render,

    /// No effect needed
    None,
}

/// Scroll effects for the conversation view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollEffect {
    /// Scroll up by lines
    Up(usize),
    /// Scroll down by lines
    Down(usize),
    /// Scroll to top
    ToTop,
    /// Scroll to bottom
    ToBottom,
    /// Scroll to specific position
    ToPosition(usize),
}

/// Dialog result effects
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogEffect {
    /// Dialog was confirmed
    Confirmed,
    /// Dialog was cancelled
    Cancelled,
    /// Item was selected (index)
    Selected(usize),
    /// Dialog was dismissed (info dialogs)
    Dismissed,
}

/// Dispatch a TuiAction to a RunnerEffect
///
/// This is the central translation point that keeps the runner
/// from needing to understand the details of TuiAction.
pub fn dispatch(action: TuiAction) -> RunnerEffect {
    use crate::tui::event_result::ScrollDirection;

    match action {
        TuiAction::Exit => RunnerEffect::Exit,
        TuiAction::Cancel => RunnerEffect::Cancel,
        TuiAction::SendMessage(msg) => RunnerEffect::SendMessage(msg),
        TuiAction::ExecuteCommand(cmd) => RunnerEffect::ExecuteCommand(cmd),
        TuiAction::CycleMode => RunnerEffect::CycleMode,

        TuiAction::ScrollLines(lines) => {
            let effect = if lines > 0 {
                ScrollEffect::Down(lines as usize)
            } else {
                ScrollEffect::Up((-lines) as usize)
            };
            RunnerEffect::Scroll(effect)
        }

        TuiAction::ScrollTo(pos) => {
            let effect = if pos == 0 {
                ScrollEffect::ToTop
            } else if pos == usize::MAX {
                ScrollEffect::ToBottom
            } else {
                ScrollEffect::ToPosition(pos)
            };
            RunnerEffect::Scroll(effect)
        }

        TuiAction::ScrollPage(dir) => {
            let effect = match dir {
                ScrollDirection::Up => ScrollEffect::Up(10),
                ScrollDirection::Down => ScrollEffect::Down(10),
            };
            RunnerEffect::Scroll(effect)
        }

        TuiAction::ConfirmPopup(_idx) => {
            // This is handled by the popup system, convert to Render for now
            RunnerEffect::Render
        }

        TuiAction::DismissPopup => RunnerEffect::Render,

        TuiAction::PopupConfirm(item) => RunnerEffect::ApplyPopupSelection(item),
        TuiAction::PopupClose => RunnerEffect::Render,

        TuiAction::CloseDialog(result) => {
            let effect = match result {
                crate::tui::event_result::DialogResult::Confirm => DialogEffect::Confirmed,
                crate::tui::event_result::DialogResult::Cancel => DialogEffect::Cancelled,
                crate::tui::event_result::DialogResult::Select(idx) => DialogEffect::Selected(idx),
            };
            RunnerEffect::DialogResult(effect)
        }

        TuiAction::DialogConfirm => RunnerEffect::DialogResult(DialogEffect::Confirmed),
        TuiAction::DialogCancel => RunnerEffect::DialogResult(DialogEffect::Cancelled),
        TuiAction::DialogSelect(idx) => RunnerEffect::DialogResult(DialogEffect::Selected(idx)),
        TuiAction::DialogDismiss => RunnerEffect::DialogResult(DialogEffect::Dismissed),

        TuiAction::RequestFocus(_) => RunnerEffect::Render,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::event_result::ScrollDirection;
    use crate::tui::state::PopupItem;

    // ==========================================================================
    // PopupEffect conversion tests
    // ==========================================================================

    #[test]
    fn test_popup_effect_command() {
        let item = PopupItem::cmd("help").desc("Show help");
        let effect = popup_item_to_effect(&item);
        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "/help ".into()
            }
        );
    }

    #[test]
    fn test_popup_effect_agent() {
        let item = PopupItem::agent("dev").desc("Developer agent");
        let effect = popup_item_to_effect(&item);
        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "@dev".into()
            }
        );
    }

    #[test]
    fn test_popup_effect_file() {
        let item = PopupItem::file("/path/to/file.rs");
        let effect = popup_item_to_effect(&item);
        assert_eq!(
            effect,
            PopupEffect::AddFileContext {
                path: "/path/to/file.rs".into()
            }
        );
    }

    #[test]
    fn test_popup_effect_note() {
        let item = PopupItem::note("Project/README");
        let effect = popup_item_to_effect(&item);
        assert_eq!(
            effect,
            PopupEffect::AddNoteContext {
                path: "Project/README".into()
            }
        );
    }

    #[test]
    fn test_popup_effect_skill() {
        let item = PopupItem::skill("commit").desc("Create git commit");
        let effect = popup_item_to_effect(&item);
        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "/commit ".into()
            }
        );
    }

    #[test]
    fn test_popup_effect_repl_command() {
        let item = PopupItem::repl("quit").desc("Exit the application");
        let effect = popup_item_to_effect(&item);
        assert_eq!(
            effect,
            PopupEffect::ExecuteReplCommand {
                name: "quit".into()
            }
        );
    }

    #[test]
    fn test_popup_effect_session() {
        let item = PopupItem::session("sess_123")
            .desc("Previous conversation")
            .with_message_count(5);
        let effect = popup_item_to_effect(&item);
        assert_eq!(
            effect,
            PopupEffect::ResumeSession {
                session_id: "sess_123".into()
            }
        );
    }

    // ==========================================================================
    // PopupHooks tests
    // ==========================================================================

    struct MockHook {
        override_effect: Option<PopupEffect>,
    }

    impl PopupHook for MockHook {
        fn on_popup_select(&self, _item: &PopupItem) -> Option<PopupEffect> {
            self.override_effect.clone()
        }
    }

    #[test]
    fn test_popup_hooks_default_when_empty() {
        let hooks = PopupHooks::new();
        let item = PopupItem::cmd("test");
        let effect = hooks.dispatch(&item);
        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "/test ".into()
            }
        );
    }

    #[test]
    fn test_popup_hooks_uses_first_matching() {
        let mut hooks = PopupHooks::new();
        hooks.register(Box::new(MockHook {
            override_effect: Some(PopupEffect::InsertToken {
                token: "custom".into(),
            }),
        }));

        let item = PopupItem::file("/test.rs");
        let effect = hooks.dispatch(&item);
        // Hook overrides the default file behavior
        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "custom".into()
            }
        );
    }

    #[test]
    fn test_popup_hooks_falls_through_none() {
        let mut hooks = PopupHooks::new();
        hooks.register(Box::new(MockHook {
            override_effect: None,
        }));

        let item = PopupItem::file("/test.rs");
        let effect = hooks.dispatch(&item);
        // Falls through to default
        assert_eq!(
            effect,
            PopupEffect::AddFileContext {
                path: "/test.rs".into()
            }
        );
    }

    // ==========================================================================
    // Basic action dispatch
    // ==========================================================================

    #[test]
    fn test_dispatch_exit() {
        let effect = dispatch(TuiAction::Exit);
        assert_eq!(effect, RunnerEffect::Exit);
    }

    #[test]
    fn test_dispatch_cancel() {
        let effect = dispatch(TuiAction::Cancel);
        assert_eq!(effect, RunnerEffect::Cancel);
    }

    #[test]
    fn test_dispatch_send_message() {
        let effect = dispatch(TuiAction::SendMessage("hello".into()));
        assert_eq!(effect, RunnerEffect::SendMessage("hello".into()));
    }

    #[test]
    fn test_dispatch_execute_command() {
        let effect = dispatch(TuiAction::ExecuteCommand("/help".into()));
        assert_eq!(effect, RunnerEffect::ExecuteCommand("/help".into()));
    }

    #[test]
    fn test_dispatch_cycle_mode() {
        let effect = dispatch(TuiAction::CycleMode);
        assert_eq!(effect, RunnerEffect::CycleMode);
    }

    // ==========================================================================
    // Scroll action dispatch
    // ==========================================================================

    #[test]
    fn test_dispatch_scroll_up() {
        let effect = dispatch(TuiAction::ScrollLines(-3));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Up(3)));
    }

    #[test]
    fn test_dispatch_scroll_down() {
        let effect = dispatch(TuiAction::ScrollLines(5));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Down(5)));
    }

    #[test]
    fn test_dispatch_scroll_page_up() {
        let effect = dispatch(TuiAction::ScrollPage(ScrollDirection::Up));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Up(10)));
    }

    #[test]
    fn test_dispatch_scroll_page_down() {
        let effect = dispatch(TuiAction::ScrollPage(ScrollDirection::Down));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Down(10)));
    }

    #[test]
    fn test_dispatch_scroll_to_top() {
        let effect = dispatch(TuiAction::ScrollTo(0));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::ToTop));
    }

    #[test]
    fn test_dispatch_scroll_to_bottom() {
        let effect = dispatch(TuiAction::ScrollTo(usize::MAX));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::ToBottom));
    }

    // ==========================================================================
    // Popup action dispatch
    // ==========================================================================

    #[test]
    fn test_dispatch_popup_confirm() {
        let item = PopupItem::cmd("help").desc("Show help").with_score(100);

        let effect = dispatch(TuiAction::PopupConfirm(item.clone()));
        assert_eq!(effect, RunnerEffect::ApplyPopupSelection(item));
    }

    #[test]
    fn test_dispatch_popup_close() {
        let effect = dispatch(TuiAction::PopupClose);
        assert_eq!(effect, RunnerEffect::Render);
    }

    // ==========================================================================
    // Dialog action dispatch
    // ==========================================================================

    #[test]
    fn test_dispatch_dialog_confirm() {
        let effect = dispatch(TuiAction::DialogConfirm);
        assert_eq!(effect, RunnerEffect::DialogResult(DialogEffect::Confirmed));
    }

    #[test]
    fn test_dispatch_dialog_cancel() {
        let effect = dispatch(TuiAction::DialogCancel);
        assert_eq!(effect, RunnerEffect::DialogResult(DialogEffect::Cancelled));
    }

    #[test]
    fn test_dispatch_dialog_select() {
        let effect = dispatch(TuiAction::DialogSelect(2));
        assert_eq!(
            effect,
            RunnerEffect::DialogResult(DialogEffect::Selected(2))
        );
    }

    #[test]
    fn test_dispatch_dialog_dismiss() {
        let effect = dispatch(TuiAction::DialogDismiss);
        assert_eq!(effect, RunnerEffect::DialogResult(DialogEffect::Dismissed));
    }
}
