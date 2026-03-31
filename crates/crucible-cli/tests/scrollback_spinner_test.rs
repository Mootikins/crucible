//! Scrollback spinner leak test.
//!
//! Replays the `reproduce.cast` recording through TWO vt100 parsers:
//! 1. Normal size (user's terminal) — models real scrollback behavior
//! 2. Tall (1000 rows) — captures full history since nothing scrolls off
//!
//! The tall parser's contents() shows everything that would be in
//! scrollback + screen. Comparing against the normal parser's screen
//! extracts the scrollback portion for spinner detection.

use std::path::Path;

const SPINNER_CHARS: &[char] = &[
    '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒',
];

/// Check if a line is a standalone spinner (short line with spinner glyph).
fn is_standalone_spinner(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.len() <= 4 && trimmed.chars().any(|c| SPINNER_CHARS.contains(&c))
}

/// Replay a .cast file through both a normal and tall vt100 parser.
/// Returns (normal_parser, tall_parser).
fn replay_cast(path: &Path) -> (vt100::Parser, vt100::Parser) {
    let content = std::fs::read_to_string(path).expect("Failed to read cast file");
    let mut lines = content.lines();

    let header: serde_json::Value =
        serde_json::from_str(lines.next().expect("empty cast file")).expect("bad header");
    let cols = header["term"]["cols"].as_u64().unwrap_or(124) as u16;
    let rows = header["term"]["rows"].as_u64().unwrap_or(59) as u16;

    let mut normal = vt100::Parser::new(rows, cols, 1000);
    let mut tall = vt100::Parser::new(1000, cols, 0);

    for line in lines {
        let entry: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
        if let Some(data) = entry.get(2).and_then(|v| v.as_str()) {
            let bytes = data.as_bytes();
            normal.process(bytes);
            tall.process(bytes);
        }
    }

    (normal, tall)
}

/// Extract scrollback content from the tall parser by subtracting
/// the visible screen (from the normal parser).
fn extract_scrollback(normal: &vt100::Parser, tall: &vt100::Parser) -> String {
    let tall_contents = tall.screen().contents();
    let screen_contents = normal.screen().contents();

    let tall_lines: Vec<&str> = tall_contents.lines().collect();
    let screen_lines: Vec<&str> = screen_contents.lines().collect();

    let scrollback_count = tall_lines.len().saturating_sub(screen_lines.len());
    if scrollback_count > 0 {
        tall_lines[..scrollback_count].join("\n")
    } else {
        String::new()
    }
}

fn cast_path() -> Option<std::path::PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("reproduce.cast");
    if path.exists() { Some(path) } else { None }
}

#[test]
#[ignore = "requires reproduce.cast file in repo root"]
fn replay_cast_no_spinner_in_scrollback() {
    let path = match cast_path() {
        Some(p) => p,
        None => {
            eprintln!("Skipping: reproduce.cast not found");
            return;
        }
    };

    let (normal, tall) = replay_cast(&path);

    // Extract scrollback via tall parser
    let scrollback = extract_scrollback(&normal, &tall);
    let scrollback_line_count = scrollback.lines().count();
    eprintln!("Scrollback: {} lines", scrollback_line_count);

    // Check scrollback for standalone spinners
    let scrollback_spinners: Vec<(usize, String)> = scrollback
        .lines()
        .enumerate()
        .filter(|(_, l)| is_standalone_spinner(l))
        .map(|(i, l)| (i, l.to_string()))
        .collect();

    if !scrollback_spinners.is_empty() {
        eprintln!("\n=== SPINNERS IN SCROLLBACK ===");
        for (i, line) in &scrollback_spinners {
            eprintln!("  [{:3}] {} ← SPINNER", i, line);
        }

        // Print surrounding context
        let scrollback_lines: Vec<&str> = scrollback.lines().collect();
        for (i, _) in &scrollback_spinners {
            let start = i.saturating_sub(2);
            let end = (*i + 3).min(scrollback_lines.len());
            eprintln!("\n  Context around line {}:", i);
            for j in start..end {
                let marker = if j == *i { " <<<" } else { "" };
                eprintln!("    [{:3}] {}{}", j, scrollback_lines[j], marker);
            }
        }
    }

    // Also check the full tall parser output for spinners
    let tall_contents = tall.screen().contents();
    let tall_spinners: Vec<(usize, String)> = tall_contents
        .lines()
        .enumerate()
        .filter(|(_, l)| is_standalone_spinner(l))
        .map(|(i, l)| (i, l.to_string()))
        .collect();

    if !tall_spinners.is_empty() {
        eprintln!("\n=== SPINNERS IN FULL HISTORY (tall parser) ===");
        let tall_lines: Vec<&str> = tall_contents.lines().collect();
        for (i, line) in &tall_spinners {
            let start = i.saturating_sub(2);
            let end = (*i + 3).min(tall_lines.len());
            eprintln!("\n  Spinner '{}' at line {}:", line, i);
            for j in start..end {
                let marker = if j == *i { " <<<" } else { "" };
                eprintln!("    [{:3}] {}{}", j, tall_lines[j], marker);
            }
        }
    }

    assert!(
        scrollback_spinners.is_empty(),
        "Spinners leaked into scrollback:\n{:?}",
        scrollback_spinners
    );
}

#[test]
#[ignore = "requires reproduce.cast file in repo root"]
fn replay_cast_tall_parser_no_spinner_anywhere() {
    let path = match cast_path() {
        Some(p) => p,
        None => return,
    };

    let (_normal, tall) = replay_cast(&path);
    let contents = tall.screen().contents();

    let spinners: Vec<(usize, String)> = contents
        .lines()
        .enumerate()
        .filter(|(_, l)| is_standalone_spinner(l))
        .map(|(i, l)| (i, l.to_string()))
        .collect();

    eprintln!(
        "Tall parser: {} total lines, {} standalone spinners",
        contents.lines().count(),
        spinners.len()
    );

    for (i, line) in &spinners {
        eprintln!("  [{:3}] {}", i, line);
    }

    assert!(
        spinners.is_empty(),
        "Spinners found in tall parser output (full history):\n{:?}",
        spinners
    );
}
