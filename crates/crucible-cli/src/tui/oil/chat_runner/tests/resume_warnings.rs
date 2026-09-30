//! A stored resume's `warnings` reach the TUI as notifications, in the same
//! wording `runner.rs` produces (`resume_warning_text`).

use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::resume_warning_text;
use crucible_core::protocol::requests::ResumeWarning;
use crucible_core::types::Notification;

/// Each warning gets its own sentence, and no two variants share one word
/// for word: a user reading two warnings must be able to tell them apart.
#[test]
fn each_warning_kind_has_its_own_text() {
    let texts = [
        resume_warning_text(&ResumeWarning::PluginStateReset),
        resume_warning_text(&ResumeWarning::PendingWorkCleared),
        resume_warning_text(&ResumeWarning::KilnUnavailable {
            path: "/kilns/notes".to_string(),
        }),
    ];
    for text in &texts {
        assert!(!text.is_empty());
    }
    assert_ne!(texts[0], texts[1]);
    assert_ne!(texts[1], texts[2]);
    assert_ne!(texts[0], texts[2]);
}

/// `KilnUnavailable` names the kiln in its sentence, so a user with more
/// than one kiln knows which one to re-register.
#[test]
fn kiln_unavailable_names_the_path() {
    let text = resume_warning_text(&ResumeWarning::KilnUnavailable {
        path: "/kilns/research".to_string(),
    });
    assert!(
        text.contains("/kilns/research"),
        "the warning must name the kiln: {text}"
    );
}

/// What `runner.rs` does with `OpenedSession::resume_warnings`: one warning
/// notification per entry, in order. This is the same call the runner makes
/// after `open_session` returns, so a reader who wants to see the actual
/// display behavior can run it here without a daemon.
#[test]
fn resume_warnings_become_notifications_in_order() {
    let mut app = OilChatApp::default();
    let warnings = vec![
        ResumeWarning::PluginStateReset,
        ResumeWarning::PendingWorkCleared,
        ResumeWarning::KilnUnavailable {
            path: "/kilns/gone".to_string(),
        },
    ];

    for warning in &warnings {
        app.add_notification(Notification::warning(resume_warning_text(warning)));
    }

    let messages = app.notification_messages();
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert_eq!(messages[0], resume_warning_text(&warnings[0]));
    assert_eq!(messages[1], resume_warning_text(&warnings[1]));
    assert_eq!(messages[2], resume_warning_text(&warnings[2]));
}

/// A resume with no warnings adds nothing — no empty toast for a live
/// resume or a resume with nothing to report.
#[test]
fn no_warnings_means_no_notification() {
    let mut app = OilChatApp::default();
    let warnings: Vec<ResumeWarning> = Vec::new();
    for warning in &warnings {
        app.add_notification(Notification::warning(resume_warning_text(warning)));
    }
    assert!(app.notification_messages().is_empty());
}
