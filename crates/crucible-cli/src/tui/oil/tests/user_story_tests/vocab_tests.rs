//! Exemplars for the intent-vocabulary layer (`vocab.rs`) and the
//! `StoryRuntime::settle`/`expect_frame` eventual-state helpers.
//!
//! These are not a story group of their own — they demonstrate the verbs the
//! hero-flow legs and future mock-tier stories should speak, and double as a
//! smoke test that the shared driver keeps working.

use super::support::StoryRuntime;
use super::vocab::{
    approve_permission, deny_permission, expect_assistant_contains, hydrate_from_recording,
    open_permission, run_shell, send_user_message, stream_assistant_reply,
};

#[test]
fn user_message_then_assistant_reply_reads_as_intent() {
    let mut story = StoryRuntime::new(80, 24);

    let _ = send_user_message(&mut story, "Summarize the seed note");
    stream_assistant_reply(&mut story, "The seed note covers wikilinks.");

    // Eventual-state assertion: settle spinner frames, then require the reply.
    let frame = expect_assistant_contains(&mut story, "wikilinks", 32);
    assert!(
        frame.contains("wikilinks"),
        "reply should be visible:\n{frame}"
    );
}

#[test]
fn settle_reaches_a_stable_frame() {
    let mut story = StoryRuntime::new(80, 24);
    let _ = send_user_message(&mut story, "hello");
    stream_assistant_reply(&mut story, "hi there");

    let a = story.settle(32);
    let b = story.settle(32);
    assert_eq!(a, b, "settle() should land on a stable frame");
}

#[test]
fn run_shell_opens_the_shell_modal() {
    let mut story = StoryRuntime::new(80, 24);
    let _ = run_shell(&mut story, "echo hello-from-vocab");
    assert!(
        story.app().has_shell_modal(),
        "`!command` should open the shell modal"
    );
}

#[test]
fn approve_and_deny_report_the_decision() {
    let mut story = StoryRuntime::new(80, 24);

    let _ = open_permission(&mut story, "req-1", &["ls", "-la"]);
    assert!(story.app().has_interaction_modal(), "modal should open");
    assert_eq!(approve_permission(&mut story), Some(true), "`y` approves");

    let _ = open_permission(&mut story, "req-2", &["rm", "-rf", "/"]);
    assert_eq!(deny_permission(&mut story), Some(false), "`n` denies");
}

#[test]
fn hydrate_from_recording_fills_the_viewport() {
    let mut story = StoryRuntime::new(100, 30);
    // Re-attaching a console to an existing session replays its history.
    hydrate_from_recording(&mut story, "permission_flow.jsonl");
    let frame = expect_assistant_contains(&mut story, "Cargo.toml", 32);
    assert!(
        frame.contains("Cargo.toml"),
        "history should hydrate:\n{frame}"
    );
}
