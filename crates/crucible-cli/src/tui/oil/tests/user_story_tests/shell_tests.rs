//! US-601: Shell modal execution.
//!
//! The shell modal spawns a real child process, so this story drives the
//! `ShellModal` component directly (spawn → poll to completion → assert
//! exit code / output / insert). Header/status/scroll formatting is
//! additionally unit-tested in `components/shell_modal.rs`; shell-history
//! storage (US-602) is tested inline in `chat_app/tests.rs`.
//!
//! Two bugs lived here and are now pinned: `i` returned `Close` before any
//! `Tick` could consume its pending insert (so stdout never reached the
//! composer), and the app discarded the closed modal's `ShellHistoryItem`
//! (so the command never reached the transcript).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_oil::render::render_to_plain_text;

use super::support::StoryRuntime;
use crate::tui::oil::components::{ShellHistoryItem, ShellModal, ShellModalMsg, ShellModalOutput};

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

/// `i` must carry the output in the same step it closes.
///
/// This was `#[ignore]`d as a known bug: `i` stashed a `pending_insert` flag
/// and returned `Close`, expecting a later `Tick` to emit the output — but the
/// app drops the modal the moment it sees `Close`, so no further `Tick` ever
/// reached it. One output per key is all the dispatcher ever sees, so the
/// output has to say both things at once.
#[test]
fn insert_key_inserts_output_in_one_step() {
    let mut modal = run_to_completion("printf 'alpha\\nbeta\\n'");
    let out = modal.update(ShellModalMsg::Key(key('i')), 20);
    match out {
        ShellModalOutput::Close {
            insert: Some(inserted),
            ..
        } => {
            assert!(
                inserted.content.contains("alpha") && inserted.content.contains("beta"),
                "insert payload should carry the command's stdout, got:\n{}",
                inserted.content
            );
            assert!(!inserted.truncated, "`i` inserts the whole output");
        }
        other => panic!("'i' should close AND insert in one step, got {other:?}"),
    }
}

/// …and `q` closes without inserting anything.
#[test]
fn quit_key_closes_completed_modal_without_inserting() {
    let mut modal = run_to_completion("echo done");
    match modal.update(ShellModalMsg::Key(key('q')), 20) {
        ShellModalOutput::Close { insert, .. } => {
            assert!(insert.is_none(), "`q` discards the output, got {insert:?}");
        }
        other => panic!("`q` should close a finished modal, got {other:?}"),
    }
}

/// T2 — the command has to be on screen, not merely in a container.
///
/// The state half is `chat_app::tests::a_closed_shell_command_is_recorded_in_the_transcript`.
/// This one goes through the real render path, because a node in the list
/// that never paints is the same bug one layer along — and every piece
/// downstream of `add_shell_execution` had been unreachable long enough that
/// none of it was proven either.
#[test]
fn a_closed_shell_command_appears_in_the_frame() {
    let mut story = StoryRuntime::new(80, 24);
    story
        .app()
        .handle_shell_modal_output(ShellModalOutput::Close {
            history_item: ShellHistoryItem {
                command: "cargo build --release".to_string(),
                exit_code: 101,
                output_tail: vec!["error: could not compile".to_string()],
                output_path: None,
            },
            insert: None,
        });

    let screen = story.fresh_screen();
    assert!(
        screen.contains("cargo build --release"),
        "the command should be on screen:\n{screen}"
    );
    assert!(
        screen.contains("exit 101"),
        "so should its exit code:\n{screen}"
    );
}
