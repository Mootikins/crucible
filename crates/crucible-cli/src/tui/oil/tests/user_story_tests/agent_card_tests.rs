use super::support::StoryRuntime;
use crate::tui::oil::chat_runner::SessionEventStream;

#[test]
fn a_card_backed_chat_draws_the_daemons_resolved_model() {
    let mut runtime = StoryRuntime::new(100, 24);
    runtime.app().set_model("researcher");
    let mut stream = SessionEventStream::new();
    for msg in stream.translate(
        "session_initialized",
        &serde_json::json!({
            "model": "research-model", "mode": "plan", "agent_name": null,
            "kilns": [], "workspace_path": "/work/project",
        }),
    ) {
        runtime.send(msg);
    }
    assert_eq!(runtime.app().current_model(), "research-model");
    let frame = runtime.expect_frame(|frame| frame.contains("research-model"), 1);
    assert!(frame.contains("research-model"), "{frame}");
    assert!(
        !frame.contains("researcher"),
        "the initial card label must not mask the resolved model: {frame}"
    );
}
