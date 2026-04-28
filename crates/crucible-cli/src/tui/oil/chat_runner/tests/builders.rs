use crate::tui::oil::chat_runner::OilChatRunner;
use crucible_oil::terminal::Terminal;

#[test]
fn with_show_diffs_false_propagates_to_runner_field() {
    let runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24)).with_show_diffs(false);
    assert!(
        !runner.show_diffs,
        "with_show_diffs(false) must set runner.show_diffs to false"
    );
}

#[test]
fn with_show_diffs_defaults_to_true() {
    let runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    assert!(
        runner.show_diffs,
        "OilChatRunner default for show_diffs must be true"
    );
}
