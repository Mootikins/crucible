//! Regression tests for rendering issues
//!
//! These tests reproduce specific rendering bugs to prevent regressions.

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::planning::FramePlanner;
use insta::assert_snapshot;

fn render_app(app: &OilChatApp) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let mut planner = FramePlanner::new(80, 24);
    let snapshot = planner.plan(&tree);
    strip_ansi(&snapshot.screen())
}

/// Issue: Table content duplicated after graduation
///
/// When streaming content with tables completes and graduates to scrollback,
/// the table content appears twice - once as the table, once as plain text.
///
/// Expected: Table should appear exactly once in graduated output.
#[test]
fn table_not_duplicated_after_graduation() {
    let mut app = OilChatApp::default();

    // Simulate streaming a message with a table
    let markdown_with_table = r#"Here's a summary:

| Feature | Status |
|---------|--------|
| Tables  | Working |
| Lists   | Working |

That's the overview."#;

    // Start conversation with user message
    app.on_message(ChatAppMsg::UserMessage("Show me a summary".to_string()));

    // Send as streaming deltas
    app.on_message(ChatAppMsg::TextDelta(markdown_with_table.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);

    // Count occurrences of "Tables" - should appear exactly once in the table
    let tables_count = rendered.matches("Tables").count();
    assert_eq!(
        tables_count, 1,
        "Table content should not be duplicated. Found {} occurrences of 'Tables'",
        tables_count
    );

    // Snapshot the full output
    assert_snapshot!("table_graduation_no_duplication", rendered);
}

/// Issue: Table cells lose spacing when wrapped
///
/// Multi-line content in table cells (like bullet points) gets split
/// incorrectly, with spacing lost between wrapped lines.
#[test]
fn table_cell_wrapping_preserves_spacing() {
    let mut app = OilChatApp::default();

    let markdown_with_wrapped_cells = r#"
| Section | Description |
|---------|-------------|
| Core ideas | • Markdown sessions – every chat is a file • Your notes = memory – embed every block |
"#;

    app.on_message(ChatAppMsg::UserMessage(
        "Explain the core ideas".to_string(),
    ));
    app.on_message(ChatAppMsg::TextDelta(
        markdown_with_wrapped_cells.to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);

    // Check that bullet points aren't orphaned on separate lines
    let lines: Vec<&str> = rendered.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains("•") {
            // If a line has a bullet, it should have more than just the bullet
            let bullet_line = line.trim();
            assert!(
                bullet_line.len() > 3,
                "Line {} has orphaned bullet: '{}'",
                i,
                line
            );
        }
    }

    assert_snapshot!("table_cell_wrapping", rendered);
}

/// Issue: Content duplication during streaming-to-graduated transition
///
/// When content transitions from streaming (viewport) to graduated (scrollback),
/// content may appear in both places or be duplicated.
#[test]
fn no_duplication_during_graduation_transition() {
    let mut app = OilChatApp::default();

    // Add a distinctive message
    let unique_content = "This is a unique test message with identifier XYZ123";

    app.on_message(ChatAppMsg::UserMessage("Test question".to_string()));
    app.on_message(ChatAppMsg::TextDelta(unique_content.to_string()));

    // Capture state before completion
    let before_graduation = render_app(&app);
    let before_count = before_graduation.matches("XYZ123").count();

    // Complete streaming (triggers graduation)
    app.on_message(ChatAppMsg::StreamComplete);

    // Capture state after graduation
    let after_graduation = render_app(&app);
    let after_count = after_graduation.matches("XYZ123").count();

    // Content should appear exactly once before and after
    assert_eq!(before_count, 1, "Content duplicated before graduation");
    assert_eq!(after_count, 1, "Content duplicated after graduation");

    assert_snapshot!("graduation_transition_atomic", after_graduation);
}

/// Issue: Spacing lost between graduated elements
///
/// When multiple elements (paragraphs, lists, tables) graduate together,
/// the spacing between them is sometimes lost.
#[test]
fn spacing_preserved_between_graduated_elements() {
    let mut app = OilChatApp::default();

    let markdown_with_spacing = r#"First paragraph here.

Second paragraph here.

- List item 1
- List item 2

Final paragraph."#;

    app.on_message(ChatAppMsg::UserMessage("Explain with examples".to_string()));
    app.on_message(ChatAppMsg::TextDelta(markdown_with_spacing.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);
    let lines: Vec<&str> = rendered.lines().collect();

    // Find "Second paragraph" and check there's spacing before it
    let second_para_idx = lines.iter().position(|l| l.contains("Second paragraph"));
    if let Some(idx) = second_para_idx {
        assert!(idx > 0, "Second paragraph should not be first line");
        // There should be a blank line or the first paragraph before it
        let has_spacing = idx > 1 && lines[idx - 1].trim().is_empty();
        assert!(
            has_spacing || lines.iter().take(idx).any(|l| l.contains("First")),
            "Should have proper spacing before 'Second paragraph'"
        );
    }

    assert_snapshot!("graduated_spacing_preserved", rendered);
}

/// Issue: Complex markdown with tables renders correctly
///
/// Test the actual output from the user's example to see what's happening.
#[test]
fn complex_markdown_with_table() {
    let mut app = OilChatApp::default();

    // Simplified version of the user's actual output
    let complex_markdown = r#"Crucible – a local-first AI assistant

| Section | What it covers |
|---------|----------------|
| What it is | A Rust-powered AI agent that lives on your machine. All conversations are stored as plain-text Markdown files. |
| Core ideas | • Markdown sessions – every chat is a file<br>• Your notes = memory – embed every block |

Why it matters

1. Control – All data lives on your machine
2. Flexibility – Plug in any LLM"#;

    app.on_message(ChatAppMsg::UserMessage("What is Crucible?".to_string()));
    app.on_message(ChatAppMsg::TextDelta(complex_markdown.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);

    // Check for duplication - "Crucible" should appear a reasonable number of times
    let crucible_count = rendered.matches("Crucible").count();
    assert!(
        crucible_count <= 2,
        "Content appears to be duplicated. 'Crucible' appears {} times",
        crucible_count
    );

    assert_snapshot!("complex_markdown_table", rendered);
}

#[test]
fn heading_after_paragraph_has_spacing() {
    let mut app = OilChatApp::default();

    let md_with_heading = r#"Here's what I can do:

## File Operations

- read_file: Read files
- write_file: Write files"#;

    app.on_message(ChatAppMsg::UserMessage(
        "Tell me about your tools".to_string(),
    ));
    app.on_message(ChatAppMsg::TextDelta(md_with_heading.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);
    let lines: Vec<&str> = rendered.lines().collect();

    let para_idx = lines.iter().position(|l| l.contains("can do")).unwrap();
    let heading_idx = lines
        .iter()
        .position(|l| l.contains("File Operations"))
        .unwrap();

    assert!(
        heading_idx > para_idx + 1,
        "Should have blank line between paragraph and heading.\nPara at line {}: {:?}\nHeading at line {}: {:?}\n\nAll lines:\n{}",
        para_idx, lines.get(para_idx),
        heading_idx, lines.get(heading_idx),
        lines.iter().enumerate().map(|(i, l)| format!("{:02}: {:?}", i, l)).collect::<Vec<_>>().join("\n")
    );

    assert_snapshot!("heading_after_paragraph", rendered);
}

/// Issue: Double blank lines between paragraphs/headings in graduated output
///
/// The user reported seeing double blank lines between blocks in graduated
/// scrollback output. This test checks that no two consecutive blank lines
/// appear in the rendered output of a multi-block assistant message.
#[test]
fn no_double_blank_lines_in_graduated_output() {
    let mut app = OilChatApp::default();

    let md = r#"Crucible is a local-first AI knowledge management system built in Rust. Here's a summary of what it does:

## Core Concept

Crucible is an AI assistant where every conversation becomes a searchable note you own.

## Key Features

1. Sessions as Markdown — conversations saved to your kiln
2. Knowledge Graph — wikilinks with semantic search
3. Lua Plugins — write extensions in Lua or Fennel

That's the overview."#;

    app.on_message(ChatAppMsg::UserMessage("What is Crucible?".to_string()));
    app.on_message(ChatAppMsg::TextDelta(md.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);
    let lines: Vec<&str> = rendered.lines().collect();

    let fmt_lines = || {
        lines
            .iter()
            .enumerate()
            .map(|(j, l)| format!("{:02}: {:?}", j, l))
            .collect::<Vec<_>>()
            .join("\n")
    };

    // Find where the UI chrome starts (first line of all box-drawing characters).
    // Everything before that is content + padding, which we need to check.
    let ui_chrome_start = lines
        .iter()
        .position(|l| {
            // UI chrome separator is all box-drawing characters (▄ or ▀)
            l.chars().all(|c| c == '\u{2584}' || c == '\u{2580}')
        })
        .unwrap_or(lines.len());

    // Find the last non-blank line before the UI chrome
    let last_content = lines[..ui_chrome_start]
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .unwrap_or(0);

    for i in 0..last_content {
        let both_blank = lines[i].trim().is_empty() && lines[i + 1].trim().is_empty();
        assert!(
            !both_blank,
            "Double blank line at lines {} and {}.\n\n{}",
            i,
            i + 1,
            fmt_lines()
        );
    }

    let list_end = lines
        .iter()
        .rposition(|l| l.contains("Lua Plugins"))
        .expect("should have list item");
    let final_para = lines
        .iter()
        .position(|l| l.contains("That's the overview"))
        .expect("should have final paragraph");
    assert!(
        final_para > list_end + 1,
        "Missing blank line between list and final paragraph (list at {}, para at {}).\n\n{}",
        list_end,
        final_para,
        fmt_lines()
    );

    assert_snapshot!("no_double_blank_lines", rendered);
}

/// Property: No container sequence should ever produce double blank lines.
///
/// Exercises multiple representative container patterns and asserts that
/// no triple-newline (\n\n\n, which visually is two blank lines) appears
/// in the rendered content area.
#[test]
fn no_double_blank_lines_in_any_container_sequence() {
    // Each scenario is a sequence of ChatAppMsg events that produces
    // a different combination of container kinds.
    let scenarios: Vec<(&str, Vec<ChatAppMsg>)> = vec![
        (
            "user_assistant",
            vec![
                ChatAppMsg::UserMessage("Hello".to_string()),
                ChatAppMsg::TextDelta("Hi there.".to_string()),
                ChatAppMsg::StreamComplete,
            ],
        ),
        (
            "user_tool_tool_assistant",
            vec![
                ChatAppMsg::UserMessage("Do stuff".to_string()),
                ChatAppMsg::ToolCall {
                    name: "bash".to_string(),
                    args: r#"{"command":"echo a"}"#.to_string(),
                    call_id: None,
                    description: None,
                    source: None,
                    lua_primary_arg: None,
                },
                ChatAppMsg::ToolResultDelta {
                    name: "bash".to_string(),
                    delta: "a".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolResultComplete {
                    name: "bash".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolCall {
                    name: "bash".to_string(),
                    args: r#"{"command":"echo b"}"#.to_string(),
                    call_id: None,
                    description: None,
                    source: None,
                    lua_primary_arg: None,
                },
                ChatAppMsg::ToolResultDelta {
                    name: "bash".to_string(),
                    delta: "b".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolResultComplete {
                    name: "bash".to_string(),
                    call_id: None,
                },
                ChatAppMsg::TextDelta("Done.".to_string()),
                ChatAppMsg::StreamComplete,
            ],
        ),
        (
            "user_thinking_text_tool_text",
            vec![
                ChatAppMsg::UserMessage("Analyze".to_string()),
                ChatAppMsg::ThinkingDelta("Hmm...".to_string()),
                ChatAppMsg::TextDelta("Let me check.".to_string()),
                ChatAppMsg::ToolCall {
                    name: "read_file".to_string(),
                    args: r#"{"path":"f.rs"}"#.to_string(),
                    call_id: None,
                    description: None,
                    source: None,
                    lua_primary_arg: None,
                },
                ChatAppMsg::ToolResultDelta {
                    name: "read_file".to_string(),
                    delta: "fn main() {}".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolResultComplete {
                    name: "read_file".to_string(),
                    call_id: None,
                },
                ChatAppMsg::TextDelta("Looks good.".to_string()),
                ChatAppMsg::StreamComplete,
            ],
        ),
        (
            "tool_tool_tool",
            vec![
                ChatAppMsg::UserMessage("Read everything".to_string()),
                ChatAppMsg::ToolCall {
                    name: "bash".to_string(),
                    args: r#"{"command":"echo 1"}"#.to_string(),
                    call_id: None,
                    description: None,
                    source: None,
                    lua_primary_arg: None,
                },
                ChatAppMsg::ToolResultDelta {
                    name: "bash".to_string(),
                    delta: "1".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolResultComplete {
                    name: "bash".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolCall {
                    name: "bash".to_string(),
                    args: r#"{"command":"echo 2"}"#.to_string(),
                    call_id: None,
                    description: None,
                    source: None,
                    lua_primary_arg: None,
                },
                ChatAppMsg::ToolResultDelta {
                    name: "bash".to_string(),
                    delta: "2".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolResultComplete {
                    name: "bash".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolCall {
                    name: "bash".to_string(),
                    args: r#"{"command":"echo 3"}"#.to_string(),
                    call_id: None,
                    description: None,
                    source: None,
                    lua_primary_arg: None,
                },
                ChatAppMsg::ToolResultDelta {
                    name: "bash".to_string(),
                    delta: "3".to_string(),
                    call_id: None,
                },
                ChatAppMsg::ToolResultComplete {
                    name: "bash".to_string(),
                    call_id: None,
                },
                ChatAppMsg::StreamComplete,
            ],
        ),
    ];

    for (label, events) in &scenarios {
        let mut app = OilChatApp::default();
        for event in events {
            app.on_message(event.clone());
        }

        let rendered = render_app(&app);
        let lines: Vec<&str> = rendered.lines().collect();

        // Find where UI chrome starts (box-drawing separator)
        let ui_chrome_start = lines
            .iter()
            .position(|l| l.chars().all(|c| c == '\u{2584}' || c == '\u{2580}'))
            .unwrap_or(lines.len());

        // Find last content line before chrome
        let last_content = lines[..ui_chrome_start]
            .iter()
            .rposition(|l| !l.trim().is_empty())
            .unwrap_or(0);

        for i in 0..last_content {
            let both_blank = lines[i].trim().is_empty() && lines[i + 1].trim().is_empty();
            assert!(
                !both_blank,
                "[{label}] Double blank line at lines {i} and {}.\n\n{}",
                i + 1,
                lines
                    .iter()
                    .enumerate()
                    .map(|(j, l)| format!("{j:02}: {l:?}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}

// Tests blank_line_between_graduated_prompt_and_streaming_response and
// spinner_visible_after_user_message_graduates removed: they relied on
// mark_graduated / graduated_keys which no longer exist.
// Graduation is now automatic via drain_completed.
