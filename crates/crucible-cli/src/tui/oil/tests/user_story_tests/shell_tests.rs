//! US-601: Shell modal execution.
//!
//! The shell modal spawns a real child process, so this story drives the
//! `ShellModal` component directly (spawn → poll to completion → assert
//! exit code / output / insert). Header/status/scroll formatting is
//! additionally unit-tested in `components/shell_modal.rs`; shell-history
//! storage (US-602) is tested inline in `chat_app/tests.rs`.
//!
//! Bug found (see `insert_key_should_emit_output_but_closes_first`): the
//! `i` handler returns `Close` before any `Tick` can consume the pending
//! insert, so the app finalizes the modal and stdout is never inserted.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_oil::render::render_to_plain_text;

use crate::tui::oil::components::{ShellModal, ShellModalMsg, ShellModalOutput};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// Spawn a shell command and pump ticks until it exits (or times out).
fn run_to_completion(cmd: &str) -> ShellModal {
    let cwd = std::env::current_dir().expect("cwd");
    let mut modal = ShellModal::spawn(cmd.to_string(), cwd).expect("spawn shell command");
    for _ in 0..400 {
        modal.update(ShellModalMsg::Tick, 20);
        if !modal.is_running() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        !modal.is_running(),
        "command did not complete in time: {cmd}"
    );
    modal
}

#[test]
fn successful_command_shows_output_and_exit_zero() {
    let modal = run_to_completion("echo hello-from-shell");
    let rendered = render_to_plain_text(&modal.view(80, 24), 80);
    assert!(
        rendered.contains("hello-from-shell"),
        "stdout should render in the modal:\n{rendered}"
    );
    assert!(
        rendered.contains("exit 0"),
        "a successful command should show exit 0:\n{rendered}"
    );
}

#[test]
fn failing_command_shows_nonzero_exit_code() {
    let modal = run_to_completion("exit 3");
    let rendered = render_to_plain_text(&modal.view(80, 24), 80);
    assert!(
        rendered.contains("exit 3"),
        "a failing command should surface its exit code:\n{rendered}"
    );
}

#[test]
fn quit_key_closes_completed_modal() {
    let mut modal = run_to_completion("echo done");
    let out = modal.update(ShellModalMsg::Key(key('q')), 20);
    assert!(
        matches!(out, ShellModalOutput::Close(_)),
        "`q` should close a finished modal"
    );
}

#[test]
fn insert_output_content_includes_command_and_stdout() {
    // Drives the insert-content generation directly at the component level
    // (`i` sets pending_insert + returns Close; the following Tick emits the
    // InsertOutput). Confirms the payload the chat input would receive.
    let mut modal = run_to_completion("printf 'alpha\\nbeta\\n'");
    let _ = modal.update(ShellModalMsg::Key(key('i')), 20);
    let out = modal.update(ShellModalMsg::Tick, 20);
    match out {
        ShellModalOutput::InsertOutput { content, .. } => {
            assert!(
                content.contains("alpha") && content.contains("beta"),
                "insert payload should carry the command's stdout, got:\n{content}"
            );
        }
        other => panic!("expected InsertOutput after pending insert, got {other:?}"),
    }
}

#[test]
#[ignore = "bug: 'i' returns Close before a Tick consumes pending_insert, so the \
            app finalizes the modal and stdout is never inserted into chat input (US-601)"]
fn insert_key_should_emit_output_but_closes_first() {
    let mut modal = run_to_completion("printf 'alpha\\nbeta\\n'");
    // In the real app loop this single output is all the dispatcher sees.
    let out = modal.update(ShellModalMsg::Key(key('i')), 20);
    assert!(
        matches!(out, ShellModalOutput::InsertOutput { .. }),
        "'i' should insert stdout in one step, but returned {out:?}"
    );
}
