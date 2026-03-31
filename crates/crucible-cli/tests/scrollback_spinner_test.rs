//! Scrollback spinner leak test.
//!
//! Replays the `reproduce.cast` recording through a vt100 parser with scrollback
//! enabled. This uses the REAL terminal escape sequences from an actual session
//! to detect spinners that leak from the viewport into terminal scrollback.

use std::path::Path;

/// Parse an asciinema v3 .cast file and replay through vt100.
fn replay_cast_through_vt100(
    path: &Path,
    rows: u16,
    cols: u16,
    scrollback: usize,
) -> vt100::Parser {
    let content = std::fs::read_to_string(path).expect("Failed to read cast file");
    let mut lines = content.lines();

    // Parse header
    let header: serde_json::Value =
        serde_json::from_str(lines.next().expect("empty cast file")).expect("bad header");
    let cast_cols = header["term"]["cols"].as_u64().unwrap_or(cols as u64) as u16;
    let cast_rows = header["term"]["rows"].as_u64().unwrap_or(rows as u64) as u16;

    let mut parser = vt100::Parser::new(cast_rows, cast_cols, scrollback);

    // Replay each frame
    for line in lines {
        let entry: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
        if let Some(data) = entry.get(2).and_then(|v| v.as_str()) {
            parser.process(data.as_bytes());
        }
    }

    parser
}

/// Check if a line is a standalone spinner (1-2 char line with spinner glyph).
fn is_standalone_spinner(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 4 {
        return false;
    }
    let spinner_chars = [
        '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒',
    ];
    trimmed.chars().any(|c| spinner_chars.contains(&c))
}

#[test]
#[ignore = "requires reproduce.cast file in repo root"]
fn replay_cast_no_spinner_in_scrollback() {
    let cast_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("reproduce.cast");
    if !cast_path.exists() {
        eprintln!("Skipping: reproduce.cast not found");
        return;
    }

    // Replay with scrollback enabled — this is the key difference from
    // the headless tests. With scrollback, vt100 tracks content that
    // scrolls off the top of the screen.
    let parser = replay_cast_through_vt100(&cast_path, 59, 124, 1000);

    let screen = parser.screen();
    let scrollback_count = screen.scrollback();
    eprintln!("Scrollback lines: {}", scrollback_count);

    // Read the visible screen
    let screen_contents = screen.contents();
    eprintln!("Screen rows: {}", screen_contents.lines().count());

    // Check screen for standalone spinners
    let screen_spinners: Vec<(usize, String)> = screen_contents
        .lines()
        .enumerate()
        .filter(|(_, l)| is_standalone_spinner(l))
        .map(|(i, l)| (i, l.to_string()))
        .collect();

    // Read scrollback content by shifting viewport into scrollback.
    // Parser::set_scrollback() is public and shifts the visible area.
    let mut parser = parser;
    parser.set_scrollback(usize::MAX);
    let scrollback_contents = parser.screen().contents();
    parser.set_scrollback(0);

    eprintln!("\n=== Final screen contents ===");
    for (i, line) in screen_contents.lines().enumerate() {
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            let marker = if is_standalone_spinner(line) {
                " ← SPINNER"
            } else {
                ""
            };
            eprintln!("  [{:2}] {}{}", i, trimmed, marker);
        }
    }

    // Check visible screen for standalone spinners
    assert!(
        screen_spinners.is_empty(),
        "Spinners found in final screen:\n{:?}",
        screen_spinners
    );

    // Check scrollback content for standalone spinners
    if scrollback_count > 0 {
        eprintln!("\n=== Scrollback contents ({} lines offset) ===", scrollback_count);
        let scrollback_spinners: Vec<(usize, String)> = scrollback_contents
            .lines()
            .enumerate()
            .filter(|(_, l)| is_standalone_spinner(l))
            .map(|(i, l)| (i, l.to_string()))
            .collect();

        if !scrollback_spinners.is_empty() {
            for (i, line) in &scrollback_spinners {
                eprintln!("  [{:2}] {} ← SPINNER IN SCROLLBACK", i, line);
            }
        }

        assert!(
            scrollback_spinners.is_empty(),
            "Spinners leaked into scrollback:\n{:?}",
            scrollback_spinners
        );
    }
}

/// Same test but at the user's exact terminal size (124x59 in Zellij).
#[test]
#[ignore = "requires reproduce.cast file in repo root"]
fn replay_cast_check_scrollback_exists() {
    let cast_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("reproduce.cast");
    if !cast_path.exists() {
        return;
    }

    let parser = replay_cast_through_vt100(&cast_path, 59, 124, 1000);
    let mut parser = parser;
    let scrollback = parser.screen().scrollback();

    eprintln!("Scrollback lines after full replay: {}", scrollback);

    // Read actual scrollback content
    parser.set_scrollback(usize::MAX);
    let scrollback_contents = parser.screen().contents();
    let actual_depth = parser.screen().scrollback();
    parser.set_scrollback(0);

    eprintln!("Actual scrollback depth: {}", actual_depth);

    if actual_depth > 0 {
        let spinner_lines: Vec<(usize, String)> = scrollback_contents
            .lines()
            .enumerate()
            .filter(|(_, l)| is_standalone_spinner(l))
            .map(|(i, l)| (i, l.to_string()))
            .collect();

        if !spinner_lines.is_empty() {
            eprintln!("Found {} spinner lines in scrollback:", spinner_lines.len());
            for (i, line) in &spinner_lines {
                eprintln!("  [{:2}] {}", i, line);
            }
        } else {
            eprintln!("No standalone spinners found in scrollback.");
        }
    }
}
