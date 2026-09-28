//! US-103: the slash popup lists the session's command catalog, and a
//! command that the daemon runs leaves the TUI as the line the user typed.

use super::support::StoryRuntime;
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::ChatAppMsg;
use crucible_core::types::{BuiltinCommand, CommandKind, SessionCommand};

fn skill(name: &str) -> SessionCommand {
    SessionCommand {
        name: name.to_string(),
        description: "Tidy the imports".to_string(),
        input_hint: None,
        kind: CommandKind::Skill,
    }
}

fn session_with_a_skill() -> StoryRuntime {
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::CommandsLoaded(vec![
        BuiltinCommand::Help.entry(),
        skill("tidy"),
    ]));
    story
}

#[test]
fn the_popup_lists_a_catalog_command_with_its_source() {
    let mut story = session_with_a_skill();
    story.text("/ti");
    let screen = story.screen();
    assert!(
        screen.contains("/tidy"),
        "the popup lists the skill:\n{screen}"
    );
    assert!(
        screen.contains("Tidy the imports (skill)"),
        "the popup names the source:\n{screen}"
    );
}

#[test]
fn a_catalog_command_leaves_the_tui_as_the_typed_line() {
    let mut story = session_with_a_skill();
    story.text("/tidy src/lib.rs");
    let action = story.enter();
    let sent = match action {
        Action::Send(msg) => vec![msg],
        Action::Batch(actions) => actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Send(msg) => Some(msg),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    assert!(
        sent.iter().any(
            |m| matches!(m, ChatAppMsg::ExecuteSlashCommand(line) if line == "/tidy src/lib.rs")
        ),
        "got {sent:?}"
    );
}
