//! US-302 (subagent display) + US-303 (MCP server status).
//!
//! Subagent / delegation lifecycle events render as status rows with a
//! prompt preview and a summary or error. `:mcp` lists configured servers
//! with live connection status. Both are driven through the real render
//! path; `:mcp` is exercised end-to-end via the REPL popup (`:mcp` + Enter).

use super::support::StoryRuntime;
use crate::tui::oil::chat_app::{ChatAppMsg, McpServerDisplay};

// ─── US-302: subagents & delegations ───────────────────────────────────

#[test]
fn subagent_spawn_shows_prompt_preview() {
    let mut story = StoryRuntime::new(100, 24);
    story.send(ChatAppMsg::UserMessage("delegate this".into()));
    story.send(ChatAppMsg::SubagentSpawned {
        id: "s1".into(),
        prompt: "Analyze the auth module".into(),
    });

    let screen = story.screen();
    assert!(
        screen.contains("Analyze the auth module"),
        "a spawned subagent should show its prompt preview:\n{screen}"
    );
}

#[test]
fn subagent_completion_shows_summary() {
    let mut story = StoryRuntime::new(100, 24);
    story.send(ChatAppMsg::UserMessage("delegate this".into()));
    story.send(ChatAppMsg::SubagentSpawned {
        id: "s1".into(),
        prompt: "Analyze the auth module".into(),
    });
    story.send(ChatAppMsg::SubagentCompleted {
        id: "s1".into(),
        summary: "Found three issues".into(),
    });

    let screen = story.screen();
    assert!(
        screen.contains("Found three issues"),
        "a completed subagent should show its summary:\n{screen}"
    );
}

#[test]
fn subagent_failure_shows_error() {
    let mut story = StoryRuntime::new(100, 24);
    story.send(ChatAppMsg::UserMessage("delegate this".into()));
    story.send(ChatAppMsg::SubagentSpawned {
        id: "s1".into(),
        prompt: "Analyze the auth module".into(),
    });
    story.send(ChatAppMsg::SubagentFailed {
        id: "s1".into(),
        error: "subagent timed out".into(),
    });

    let screen = story.screen();
    assert!(
        screen.contains("timed out"),
        "a failed subagent should surface its error distinctly:\n{screen}"
    );
}

#[test]
fn concurrent_subagents_render_as_separate_rows() {
    let mut story = StoryRuntime::new(100, 24);
    story.send(ChatAppMsg::UserMessage("delegate two things".into()));
    story.send(ChatAppMsg::SubagentSpawned {
        id: "s1".into(),
        prompt: "First parallel task".into(),
    });
    story.send(ChatAppMsg::SubagentSpawned {
        id: "s2".into(),
        prompt: "Second parallel task".into(),
    });

    let screen = story.screen();
    assert!(
        screen.contains("First parallel task") && screen.contains("Second parallel task"),
        "concurrent subagents should each render:\n{screen}"
    );
}

#[test]
fn delegation_shows_target_agent() {
    let mut story = StoryRuntime::new(100, 24);
    story.send(ChatAppMsg::UserMessage("delegate to claude".into()));
    story.send(ChatAppMsg::DelegationSpawned {
        id: "d1".into(),
        prompt: "Refactor the parser".into(),
        target_agent: Some("claude".into()),
    });

    let screen = story.screen();
    assert!(
        screen.contains("claude") && screen.contains("Refactor the parser"),
        "a delegation should name its target agent and prompt:\n{screen}"
    );
}

// ─── US-303: `:mcp` server status ──────────────────────────────────────

#[test]
fn mcp_command_lists_servers_with_connection_status() {
    let mut story = StoryRuntime::new(80, 24);
    story.app().set_mcp_servers(vec![
        McpServerDisplay {
            name: "github".into(),
            prefix: "gh".into(),
            tool_count: 4,
            connected: true,
        },
        McpServerDisplay {
            name: "filesystem".into(),
            prefix: "fs".into(),
            tool_count: 2,
            connected: false,
        },
    ]);

    story.text(":mcp");
    story.enter();

    let screen = story.screen();
    assert!(
        screen.contains("github") && screen.contains("filesystem"),
        ":mcp should list every configured server:\n{screen}"
    );
    assert!(
        screen.contains('●'),
        "a connected server should render a filled status dot:\n{screen}"
    );
    assert!(
        screen.contains('○'),
        "a disconnected server should render a hollow status dot:\n{screen}"
    );
}

#[test]
fn mcp_command_with_no_servers_reports_empty() {
    let mut story = StoryRuntime::new(80, 24);
    story.text(":mcp");
    story.enter();

    let screen = story.screen();
    assert!(
        screen.contains("No MCP servers configured"),
        ":mcp with no servers should say so:\n{screen}"
    );
}
