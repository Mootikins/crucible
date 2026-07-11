//! Spinner-leak regressions: no spinner glyph may survive graduation into
//! scrollback or remain on screen after content graduates.

use super::*;

/// Graduated thinking must NOT contain spinner characters.
#[test]
fn graduated_thinking_has_no_spinner() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Do something".into()));
    vt.render_frame(&mut app);

    think(&mut app, "Planning the command.");
    app.on_message(ChatAppMsg::TextDelta("Here is the plan.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Everything graduated — check scrollback for spinners
    vt.assert_no_spinners_in_scrollback();

    let full = crucible_oil::ansi::strip_ansi(&vt.full_history());
    assert!(
        full.contains("Thought"),
        "Graduated thinking should show collapsed summary.\nFull:\n{}",
        full
    );
}

/// Reproduce: spinner leaks to scrollback between graduated tools.
/// Exact sequence from user's real session.
#[test]
fn vt100_spinner_does_not_leak_to_scrollback() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(124, 59);

    // User message
    app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
    vt.render_frame(&mut app);

    // Thinking arrives
    think(
        &mut app,
        "I'll explore the repository to give you an overview of what it contains.",
    );

    // Render a few frames with spinner showing (simulates time passing)
    for _ in 0..5 {
        vt.render_frame(&mut app);
    }

    // Tool 1: Get Kiln Info (no permission needed)
    tool(&mut app, "get_kiln_info", "c1");

    // Render with completed tool + turn spinner showing
    for _ in 0..3 {
        vt.render_frame(&mut app);
    }

    // Tool 2: Bash (would normally need permission, but we skip the modal)
    tool(&mut app, "bash", "c2");
    vt.render_frame(&mut app);

    // Tool 3: Bash find
    tool(&mut app, "bash", "c3");
    vt.render_frame(&mut app);

    // Second thought block
    think(&mut app, "Let me check more details.");

    // More tools
    tool(&mut app, "read_file", "c4");
    tool(&mut app, "read_file", "c5");
    vt.render_frame(&mut app);

    // Third thought + final text
    think(&mut app, "Now I have enough context.");
    app.on_message(ChatAppMsg::TextDelta(
        "Crucible is a knowledge-grounded agent runtime.".into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Check the combined output for spinner characters in scrollback
    let stdout = vt.inner().stdout_content();
    let stdout_plain = crucible_oil::ansi::strip_ansi(stdout);

    let spinner_chars = [
        '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒',
    ];
    let spinner_lines: Vec<(usize, &str)> = stdout_plain
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            let trimmed = l.trim();
            trimmed.len() <= 2 && trimmed.chars().any(|c| spinner_chars.contains(&c))
        })
        .collect();

    assert!(
        spinner_lines.is_empty(),
        "Spinner characters found in graduated scrollback:\n{:?}\n\nFull stdout:\n{}",
        spinner_lines,
        stdout_plain
    );
}

/// Check that the final vt100 screen + accumulated stdout never contain
/// standalone spinner lines in graduated content. The stdout_buffer
/// accumulates rendered graduation content (what goes to scrollback).
/// If a spinner appears there, graduation is broken.
///
/// This test also captures EVERY intermediate screen state to detect
/// viewport spinner "ghosts" that could leak to scrollback in real terminals.
#[test]
fn vt100_no_spinner_in_any_graduated_content() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(124, 59);

    let spinner_chars = [
        '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒',
    ];
    let is_standalone_spinner = |l: &str| -> bool {
        let trimmed = l.trim();
        !trimmed.is_empty()
            && trimmed.len() <= 2
            && trimmed.chars().any(|c| spinner_chars.contains(&c))
    };

    // Track all screen states to find where spinner appears
    let mut screen_history: Vec<(String, String)> = vec![]; // (phase, screen)

    app.on_message(ChatAppMsg::UserMessage("go".into()));
    vt.render_frame(&mut app);
    screen_history.push(("after_user_msg".into(), vt.screen_contents()));

    think(&mut app, "Planning the approach carefully.");
    for i in 0..5 {
        vt.render_frame(&mut app);
        screen_history.push((format!("thinking_frame_{}", i), vt.screen_contents()));
    }

    // Tool 1 arrives and completes
    tool(&mut app, "get_kiln_info", "c1");
    vt.render_frame(&mut app);
    screen_history.push(("after_tool1".into(), vt.screen_contents()));

    // Several frames with completed tool + possible turn spinner
    for i in 0..5 {
        vt.render_frame(&mut app);
        screen_history.push((format!("post_tool1_frame_{}", i), vt.screen_contents()));
    }

    // Tool 2
    tool(&mut app, "bash", "c2");
    vt.render_frame(&mut app);
    screen_history.push(("after_tool2".into(), vt.screen_contents()));

    // Complete
    app.on_message(ChatAppMsg::TextDelta("Done.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);
    screen_history.push(("after_complete".into(), vt.screen_contents()));

    // Now check: the stdout buffer (accumulated graduation content)
    // should have NO standalone spinner lines
    let stdout = vt.inner().stdout_content();
    let stdout_plain = crucible_oil::ansi::strip_ansi(stdout);

    let spinner_in_stdout: Vec<(usize, &str)> = stdout_plain
        .lines()
        .enumerate()
        .filter(|(_, l)| is_standalone_spinner(l))
        .collect();

    if !spinner_in_stdout.is_empty() {
        // Print the screen history to help debug
        for (phase, screen) in &screen_history {
            let has_spinner = screen.lines().any(&is_standalone_spinner);
            if has_spinner {
                eprintln!("=== {} (has spinner) ===\n{}\n", phase, screen);
            }
        }
    }

    assert!(
        spinner_in_stdout.is_empty(),
        "Spinner in graduated stdout content:\n{:?}\n\nFull stdout:\n{}",
        spinner_in_stdout,
        stdout_plain
    );

    // Also check the final screen for stale spinners
    let final_screen = vt.screen_contents();
    let spinner_in_screen: Vec<(usize, &str)> = final_screen
        .lines()
        .enumerate()
        .filter(|(_, l)| is_standalone_spinner(l))
        .collect();

    assert!(
        spinner_in_screen.is_empty(),
        "Spinner in final vt100 screen:\n{:?}\n\nScreen:\n{}",
        spinner_in_screen,
        final_screen
    );
}

/// SMALL TERMINAL: Forces scrolling during graduation write.
/// When graduation content is tall enough to push old viewport content
/// past the terminal height, the old viewport (with spinner) should NOT
/// survive in scrollback.
#[test]
fn vt100_small_terminal_spinner_no_leak_on_scroll() {
    let mut app = OilChatApp::init();
    // Very small terminal — 10 rows. Viewport + graduation will exceed this.
    let mut vt = Vt100TestRuntime::new(80, 10);

    let spinner_chars = [
        '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒',
    ];
    let is_standalone_spinner = |l: &str| -> bool {
        let trimmed = l.trim();
        !trimmed.is_empty()
            && trimmed.len() <= 2
            && trimmed.chars().any(|c| spinner_chars.contains(&c))
    };

    app.on_message(ChatAppMsg::UserMessage("go".into()));
    vt.render_frame(&mut app);

    think(&mut app, "Planning.");
    vt.render_frame(&mut app); // spinner shows in viewport

    // Tool completes — thinking + tool graduate on next frame
    tool(&mut app, "bash", "c1");
    vt.render_frame(&mut app); // graduation write — may scroll in 10-row terminal

    // More renders to stabilize
    for _ in 0..3 {
        vt.render_frame(&mut app);
    }

    // Second tool
    tool(&mut app, "read_file", "c2");
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Check graduated content
    let stdout = vt.inner().stdout_content();
    let stdout_plain = crucible_oil::ansi::strip_ansi(stdout);

    let spinner_lines: Vec<(usize, &str)> = stdout_plain
        .lines()
        .enumerate()
        .filter(|(_, l)| is_standalone_spinner(l))
        .collect();

    assert!(
        spinner_lines.is_empty(),
        "Spinner leaked in 10-row terminal:\n{:?}\n\nStdout:\n{}",
        spinner_lines,
        stdout_plain
    );

    // Also check vt100 screen
    let screen = vt.screen_contents();
    let screen_spinners: Vec<(usize, &str)> = screen
        .lines()
        .enumerate()
        .filter(|(_, l)| is_standalone_spinner(l))
        .collect();

    assert!(
        screen_spinners.is_empty(),
        "Spinner in vt100 screen (10-row):\n{:?}\n\nScreen:\n{}",
        screen_spinners,
        screen
    );
}

/// Check vt100 SCREEN (not stdout buffer) for spinners after graduation.
/// This uses the actual vt100 screen state which models real terminal behavior.
#[test]
fn vt100_screen_no_spinner_after_graduation() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(124, 59);

    app.on_message(ChatAppMsg::UserMessage("go".into()));
    vt.render_frame(&mut app);

    think(&mut app, "Planning the approach.");

    // Render several frames so spinner appears in viewport
    for _ in 0..5 {
        vt.render_frame(&mut app);
    }

    // Tool arrives and completes — thinking can graduate
    tool(&mut app, "bash", "c1");
    vt.render_frame(&mut app);

    // Render a few more frames with completed tool + turn spinner
    for _ in 0..3 {
        vt.render_frame(&mut app);
    }

    // Second tool
    tool(&mut app, "read_file", "c2");
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Check the vt100 SCREEN (not stdout buffer) for spinners
    let screen = vt.screen_contents();
    let spinner_chars = [
        '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒',
    ];

    // Look for standalone spinner lines in the screen content
    let spinner_lines: Vec<(usize, &str)> = screen
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            let trimmed = l.trim();
            !trimmed.is_empty()
                && trimmed.len() <= 2
                && trimmed.chars().any(|c| spinner_chars.contains(&c))
        })
        .collect();

    assert!(
        spinner_lines.is_empty(),
        "Spinner found in vt100 screen after graduation:\n{:?}\n\nFull screen:\n{}",
        spinner_lines,
        screen
    );
}

/// Same test but with tick-per-event (renders between every event).
#[test]
fn vt100_spinner_no_leak_tick_per_event() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(124, 59);

    app.on_message(ChatAppMsg::UserMessage("go".into()));
    vt.render_frame(&mut app);

    think(&mut app, "Planning.");
    vt.render_frame(&mut app);

    // Tool arrives — creates ToolGroup, thinking becomes graduatable
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"cmd": "ls"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    vt.render_frame(&mut app); // thinking + tool in viewport, spinner may show

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });
    vt.render_frame(&mut app); // tool complete, turn spinner shows

    // Another tool
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "README.md"}"#.into(),
        call_id: Some("c2".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c2".into()),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Done.".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let stdout = vt.inner().stdout_content();
    let stdout_plain = crucible_oil::ansi::strip_ansi(stdout);

    let spinner_chars = [
        '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒',
    ];
    let spinner_lines: Vec<(usize, &str)> = stdout_plain
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            let trimmed = l.trim();
            trimmed.len() <= 2 && trimmed.chars().any(|c| spinner_chars.contains(&c))
        })
        .collect();

    assert!(
        spinner_lines.is_empty(),
        "Spinner leaked to scrollback:\n{:?}\n\nFull stdout:\n{}",
        spinner_lines,
        stdout_plain
    );
}

// ─── Bug 3: Spinner leaks to scrollback via vt100 inspection ──────
//
// These tests use vt100::set_scrollback() to read actual scrollback
// content — the previous tests only checked the stdout_buffer string.
// These catch the class of bugs where spinner chars end up in real
// terminal scrollback during graduation.

/// Permission modal graduation: thinking graduates, viewport gets turn
/// spinner, then tool arrives. Scrollback must not contain spinner chars.
#[test]
fn vt100_scrollback_no_spinner_after_permission_graduation() {
    use crucible_core::interaction::{InteractionRequest, PermRequest};

    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // User message
    app.on_message(ChatAppMsg::UserMessage("Do something risky".into()));
    vt.render_frame(&mut app);

    // Thinking arrives
    think(&mut app, "I need to run a dangerous command.");

    // Render with spinner in viewport
    for _ in 0..3 {
        vt.render_frame(&mut app);
    }

    // Permission request arrives — thinking becomes graduatable
    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "perm-1".into(),
        request: InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/tmp/test"])),
    });

    // This render graduates the thinking, viewport now has turn spinner + modal
    vt.render_frame(&mut app);

    // Tool call arrives (simulating user approval)
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"cmd": "rm -rf /tmp/test"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Inspect scrollback via vt100 — the actual terminal scrollback
    vt.assert_no_spinners_in_scrollback();
}

/// Rapid sequential graduations: 5 quick graduation cycles.
/// Mimics the 25ms burst from the production log.
#[test]
fn vt100_rapid_sequential_graduations_clean_scrollback() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Do many things".into()));
    vt.render_frame(&mut app);

    // 5 rapid tool cycles — each graduates the previous content
    for i in 0..5 {
        let id = format!("c{}", i);
        think(&mut app, &format!("Step {}.", i));
        tool(&mut app, "bash", &id);
        vt.render_frame(&mut app); // graduation happens here
    }

    app.on_message(ChatAppMsg::TextDelta("All done.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Inspect scrollback for spinner leak
    vt.assert_no_spinners_in_scrollback();
}

/// Verify graduation writes are inside synchronized update blocks.
/// Captures raw terminal bytes and checks escape sequence structure.
///
/// The clear() sequence (MoveUp + ClearFromCursorDown) and graduation
/// content must be inside the same synchronized update block as the
/// viewport render. This prevents the terminal from showing intermediate
/// states where old spinner content is visible.
#[test]
fn vt100_graduation_bytes_inside_sync_update() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("go".into()));
    vt.render_frame(&mut app);

    // This frame will trigger graduation (user message graduates
    // because thinking + tool follow it)
    think(&mut app, "Planning.");
    tool(&mut app, "bash", "c1");
    vt.render_frame(&mut app);

    // Get raw bytes from this graduation frame
    let bytes = vt.last_frame_bytes();
    let byte_str = String::from_utf8_lossy(bytes);

    let begin_sync = "\x1b[?2026h";
    let end_sync = "\x1b[?2026l";

    // Find the FIRST begin_sync and LAST end_sync
    let first_begin = byte_str.find(begin_sync);
    let last_end = byte_str.rfind(end_sync);

    // There must be sync markers in a graduation frame
    assert!(
        first_begin.is_some() && last_end.is_some(),
        "Graduation frame must contain synchronized update markers.\nBytes:\n{}",
        byte_str.replace('\x1b', "ESC")
    );

    let begin_pos = first_begin.unwrap();
    let end_pos = last_end.unwrap();

    // Find ClearFromCursorDown: \x1b[0J or \x1b[J
    // This is the clear() operation that erases the viewport
    let clear_pattern_positions: Vec<usize> = byte_str
        .match_indices("\x1b[")
        .filter_map(|(pos, _)| {
            let rest = &byte_str[pos + 2..];
            if rest.starts_with("0J") || rest.starts_with("J") {
                Some(pos)
            } else {
                None
            }
        })
        .collect();

    // The clear() during graduation should be AFTER begin_sync
    for clear_pos in &clear_pattern_positions {
        assert!(
            *clear_pos > begin_pos && *clear_pos < end_pos,
            "Clear(FromCursorDown) at byte {} must be inside sync block [{}, {}].\n\
             Graduation writes must be inside synchronized update to prevent\n\
             terminal from showing intermediate states with spinner content.\n\
             Bytes:\n{}",
            clear_pos,
            begin_pos,
            end_pos,
            byte_str.replace('\x1b', "ESC")
        );
    }
}

// ─── Bug 4: Permission modal triggers spinner-in-scrollback ───────
//
// The daemon sends OpenInteraction(Permission) BEFORE ToolCall.
// This causes the trailing AssistantResponse (thinking) to graduate
// via permission_pending, and the viewport gets a turn spinner.
// The turn spinner then leaks into scrollback during the next
// graduation cycle.

/// The exact event sequence that triggers the spinner leak:
/// 1. User message
/// 2. Thinking (spinner in viewport)
/// 3. OpenInteraction(Permission) → thinking graduates, turn spinner appears
/// 4. Render frames with turn spinner visible
/// 5. ToolCall arrives → graduation happens, spinner may leak
/// 6. Repeat with more permission tools
///
/// Uses the user's terminal size (124x59) to match production behavior.
#[test]
fn reproduce_permission_modal_spinner_leak() {
    use crucible_core::interaction::{InteractionRequest, PermRequest};
    use crucible_oil::node::{BRAILLE_SPINNER_FRAMES, SPINNER_FRAMES};

    let all_spinner_chars: Vec<char> = SPINNER_FRAMES
        .iter()
        .chain(BRAILLE_SPINNER_FRAMES.iter())
        .copied()
        .collect();

    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(124, 59);

    // Helper: check scrollback for spinners after each phase.
    // Scrollback = tall parser contents minus normal parser screen.
    // The viewport may legitimately contain a turn spinner (it's chrome,
    // not content), so we only assert on what has scrolled off.
    let check = |vt: &mut Vt100TestRuntime, phase: &str| {
        let scrollback = vt.scrollback_contents();
        let history_plain = crucible_oil::ansi::strip_ansi(&scrollback);
        let spinners: Vec<(usize, String)> = history_plain
            .lines()
            .enumerate()
            .filter(|(_, l)| {
                let trimmed = l.trim();
                !trimmed.is_empty()
                    && trimmed.len() <= 4
                    && trimmed.chars().any(|c| all_spinner_chars.contains(&c))
            })
            .map(|(i, l)| (i, l.to_string()))
            .collect();

        if !spinners.is_empty() {
            let lines: Vec<&str> = history_plain.lines().collect();
            eprintln!("=== SPINNER LEAK at phase '{}' ===", phase);
            for (i, line) in &spinners {
                let start = i.saturating_sub(2);
                let end = (*i + 3).min(lines.len());
                eprintln!("  Spinner '{}' at line {}:", line.trim(), i);
                for (j, line) in (start..end).zip(&lines[start..end]) {
                    let marker = if j == *i { " <<<" } else { "" };
                    eprintln!("    [{:3}] {}{}", j, line, marker);
                }
            }
        }

        assert!(
            spinners.is_empty(),
            "Phase '{}': spinners in history:\n{:?}",
            phase,
            spinners
        );
    };

    // ── Turn 1: User message ──
    app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
    vt.render_frame(&mut app);

    // ── Thinking arrives (spinner shows in viewport) ──
    think(&mut app, "I'll explore the repository structure.");
    vt.render_frame(&mut app);
    vt.render_frame(&mut app); // spinner ticks

    // ── Tool 1: get_kiln_info (no permission needed) ──
    tool(&mut app, "get_kiln_info", "c1");
    vt.render_frame(&mut app);

    // Idle frames — turn spinner shows (tool complete, waiting for next)
    vt.render_frame(&mut app);
    vt.render_frame(&mut app);

    // ── Tool 2: bash (PERMISSION REQUIRED) ──
    // Daemon sends OpenInteraction BEFORE ToolCall
    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "perm-1".into(),
        request: InteractionRequest::Permission(PermRequest::bash(["ls", "-la"])),
    });
    vt.render_frame(&mut app); // thinking graduates, modal + turn spinner

    // User approves (simulated — just send the tool call)
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"cmd": "ls -la"}"#.into(),
        call_id: Some("c2".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c2".into()),
    });
    vt.render_frame(&mut app);

    check(&mut vt, "after_first_permission_tool");

    // Idle frames — turn spinner between tools
    vt.render_frame(&mut app);
    vt.render_frame(&mut app);

    // ── Tool 3: bash find (PERMISSION REQUIRED) ──
    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "perm-2".into(),
        request: InteractionRequest::Permission(PermRequest::bash([
            "find",
            ".",
            "-maxdepth",
            "2",
            "-name",
            "README*",
        ])),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"cmd": "find . -maxdepth 2 -name README*"}"#.into(),
        call_id: Some("c3".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c3".into()),
    });
    vt.render_frame(&mut app);

    check(&mut vt, "after_second_permission_tool");

    // ── Second thinking block ──
    think(&mut app, "Let me check the crate structure.");
    vt.render_frame(&mut app);
    vt.render_frame(&mut app);

    // ── Tool 4: bash ls crates (PERMISSION REQUIRED) ──
    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "perm-3".into(),
        request: InteractionRequest::Permission(PermRequest::bash(["ls", "-la", "crates/"])),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"cmd": "ls -la crates/"}"#.into(),
        call_id: Some("c4".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c4".into()),
    });
    vt.render_frame(&mut app);

    check(&mut vt, "after_third_permission_tool");

    // ── Final response ──
    think(&mut app, "Now I have enough context.");
    app.on_message(ChatAppMsg::TextDelta(
        "Crucible is a knowledge-grounded agent runtime.".into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    check(&mut vt, "final");
}
