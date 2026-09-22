//! US-910 (the branch diff, drawn by the TUI).
//!
//! The paging and the text requests are unit-tested in
//! `components/diff_modal.rs`, and the command and the reducer in
//! `chat_app/tests.rs`. This is the render half: proof that a diffset and the
//! texts of its files reach a frame.

use crossterm::event::KeyCode;
use crucible_core::diff::{DiffFileEntry, DiffFileText, Diffset, DiffsetSource, FileStatus};
use crucible_core::session::PhysicalRoot;

use crate::tui::oil::chat_app::ChatAppMsg;

use super::support::StoryRuntime;

fn entry(path: &str, status: FileStatus) -> DiffFileEntry {
    DiffFileEntry {
        root: PhysicalRoot::from_top_level("/repo"),
        path: path.into(),
        status,
        added: 1,
        removed: 1,
        binary: false,
        too_large: false,
    }
}

fn diffset() -> Diffset {
    let source = DiffsetSource::Branch {
        root: PhysicalRoot::from_top_level("/repo"),
        base: "main".into(),
        head: None,
    };
    Diffset {
        id: source.id(),
        source,
        files: vec![
            entry(
                "src/new_name.rs",
                FileStatus::Renamed {
                    from: "src/old_name.rs".into(),
                },
            ),
            entry("src/gone.rs", FileStatus::Deleted),
        ],
        unreadable_roots: Vec::new(),
    }
}

fn text(index: usize, base: &str, current: Option<&str>) -> ChatAppMsg {
    ChatAppMsg::DiffFileLoaded {
        id: diffset().id,
        index,
        text: DiffFileText {
            base_text: Some(base.into()),
            current_text: current.map(str::to_string),
        },
    }
}

/// The base, the file position, the rename and the changed lines reach the
/// frame. The next file shows the deletion. Escape gives the prompt back.
///
/// Each frame is fresh: the inline test path keeps the rows of the frame
/// before, and a real terminal redraws the full screen when a modal opens.
#[test]
fn a_branch_diff_reaches_the_frame() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::DiffLoaded(Box::new(diffset())));

    let screen = story.fresh_screen();
    assert!(
        screen.contains("main...working tree"),
        "the base:\n{screen}"
    );
    assert!(screen.contains("file 1/2"), "the position:\n{screen}");
    assert!(
        screen.contains("loading"),
        "the text is on its way:\n{screen}"
    );
    assert!(
        !screen.contains("Type a message"),
        "no prompt composed with it:\n{screen}"
    );

    story.send(text(
        0,
        "fn kept() {}\nfn old() {}\n",
        Some("fn kept() {}\nfn new() {}\n"),
    ));
    let screen = story.fresh_screen();
    assert!(
        screen.contains("src/old_name.rs → src/new_name.rs"),
        "the rename:\n{screen}"
    );
    assert!(
        screen.contains("-fn old() {}"),
        "the removed line:\n{screen}"
    );
    assert!(screen.contains("+fn new() {}"), "the added line:\n{screen}");
    assert!(screen.contains("PgUp/PgDn"), "the key hints:\n{screen}");

    story.key(KeyCode::Char('n'));
    story.send(text(1, "fn doomed() {}\n", None));
    let screen = story.fresh_screen();
    assert!(screen.contains("file 2/2"), "the position:\n{screen}");
    assert!(
        screen.contains("delete src/gone.rs"),
        "the deletion:\n{screen}"
    );
    assert!(
        screen.contains("-fn doomed() {}"),
        "the removed line:\n{screen}"
    );

    story.key(KeyCode::Esc);
    let screen = story.fresh_screen();
    assert!(!screen.contains("file 2/2"), "the view closed:\n{screen}");
    assert!(screen.contains("ASK"), "the prompt is back:\n{screen}");
}
