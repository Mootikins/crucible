//! US-801: Review history without losing my place.
//!
//! The main chat viewport graduates completed content to the *terminal's*
//! scrollback (real stdout), so the app holds no scroll-offset state for
//! it — reviewing graduated history and jump-to-live are genuinely a
//! real-terminal (T4/PTY) concern and can't be observed headlessly.
//!
//! The scroll state the app *does* own is the shell modal's scroll region
//! (US-601's `j/k/g/G/PgUp/PgDn`), which also carries the auto-follow
//! semantics US-801 describes (manual scroll disables follow). We drive it
//! through the public `Key` path here; the auto-follow-disables-on-scroll
//! behaviour is unit-tested against private state in
//! `components/shell_modal.rs`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::oil::components::{ShellModal, ShellModalMsg};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn run(cmd: &str) -> ShellModal {
    let cwd = std::env::current_dir().expect("cwd");
    let mut modal = ShellModal::spawn(cmd.to_string(), cwd).expect("spawn");
    for _ in 0..400 {
        modal.update(ShellModalMsg::Tick, 10);
        if !modal.is_running() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!modal.is_running(), "command did not complete: {cmd}");
    modal
}

#[test]
fn top_and_bottom_show_different_windows() {
    let mut modal = run("seq 1 60");
    let visible = 10;

    modal.update(ShellModalMsg::Key(key(KeyCode::Char('g'))), visible);
    let top: Vec<String> = modal.visible_lines(visible).to_vec();
    assert_eq!(
        top.first().map(String::as_str),
        Some("1"),
        "`g` should reveal the first line"
    );

    modal.update(ShellModalMsg::Key(key(KeyCode::Char('G'))), visible);
    let bottom: Vec<String> = modal.visible_lines(visible).to_vec();
    assert_eq!(
        bottom.last().map(String::as_str),
        Some("60"),
        "`G` should reveal the last line"
    );

    assert_ne!(top, bottom, "scrolling should change the visible window");
}

#[test]
fn page_up_from_bottom_reveals_earlier_lines() {
    let mut modal = run("seq 1 60");
    let visible = 10;

    modal.update(ShellModalMsg::Key(key(KeyCode::Char('G'))), visible);
    let bottom_first = modal.visible_lines(visible).first().cloned();

    modal.update(ShellModalMsg::Key(key(KeyCode::PageUp)), visible);
    let after_pgup_first = modal.visible_lines(visible).first().cloned();

    assert_ne!(
        bottom_first, after_pgup_first,
        "PageUp from the bottom should scroll earlier lines into view"
    );
}
