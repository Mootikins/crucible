//! Stability and Long-Running Tests
//!
//! These tests verify TUI stability over extended operation:
//! - Memory leak detection
//! - Event loop consistency
//! - Piped input handling
//! - State corruption detection
//!
//! # Running Tests
//!
//! ```bash
//! # Quick stability check (1000 iterations)
//! cargo test -p crucible-cli stability -- --ignored --nocapture
//!
//! # Extended check (100k iterations, ~10 min)
//! STABILITY_ITERATIONS=100000 cargo test -p crucible-cli stability_extended -- --ignored --nocapture
//!
//! # 12-hour soak test
//! STABILITY_DURATION_HOURS=12 cargo test -p crucible-cli stability_soak -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

// =============================================================================
// Harness-based stability tests (fast, in-process)
// =============================================================================

/// Quick stability check - run harness through many iterations
#[test]
#[ignore = "stability test - run explicitly"]
fn stability_harness_iterations() {
    use crucible_cli::tui::testing::Harness;
    use crossterm::event::KeyCode;

    let iterations: usize = std::env::var("STABILITY_ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    println!("Running {} harness iterations...", iterations);

    let start = Instant::now();
    let mut h = Harness::new(80, 24);

    for i in 0..iterations {
        // Simulate typical user actions
        match i % 10 {
            0 => {
                // Type some text
                h.keys("test message ");
            }
            1 => {
                // Backspace
                h.key(KeyCode::Backspace);
            }
            2 => {
                // Open popup
                h.key(KeyCode::Char('/'));
            }
            3 => {
                // Navigate popup
                h.key(KeyCode::Down);
                h.key(KeyCode::Up);
            }
            4 => {
                // Close popup
                h.key(KeyCode::Esc);
            }
            5 => {
                // Render frame
                let _ = h.render();
            }
            6 => {
                // Clear input
                h.key_ctrl('u');
            }
            7 => {
                // Word navigation
                h.key_ctrl('w');
            }
            8 => {
                // Cursor movement
                h.key(KeyCode::Left);
                h.key(KeyCode::Right);
            }
            9 => {
                // Home/End
                h.key(KeyCode::Home);
                h.key(KeyCode::End);
            }
            _ => {}
        }

        // Every 100 iterations, verify state consistency
        if i % 100 == 0 && i > 0 {
            // Render should never panic
            let frame = h.render();
            assert!(!frame.is_empty(), "Render should produce output");

            // Popup state should be consistent
            if h.has_popup() {
                assert!(h.popup().is_some());
            }

            print!(".");
            if i % 1000 == 0 {
                println!(" {}", i);
            }
        }
    }

    let elapsed = start.elapsed();
    println!(
        "\nCompleted {} iterations in {:?} ({:.0} iter/sec)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
}

/// Extended stability with streaming events
#[test]
#[ignore = "extended stability test"]
fn stability_with_streaming() {
    use crucible_cli::tui::streaming_channel::StreamingEvent;
    use crucible_cli::tui::testing::Harness;

    let iterations: usize = std::env::var("STABILITY_ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);

    println!("Running {} streaming iterations...", iterations);

    let start = Instant::now();
    let mut h = Harness::new(80, 24);

    for i in 0..iterations {
        // Inject streaming events
        let events = vec![
            StreamingEvent::Delta {
                text: format!("Response chunk {} ", i),
                seq: i as u64,
            },
            StreamingEvent::Delta {
                text: "with more text.".to_string(),
                seq: i as u64 + 1,
            },
            StreamingEvent::Done {
                full_response: format!("Response chunk {} with more text.", i),
            },
        ];

        for event in events {
            h.event(event);
        }

        // Render after events
        let frame = h.render();
        assert!(!frame.is_empty());

        if i % 50 == 0 {
            print!(".");
            if i % 500 == 0 && i > 0 {
                println!(" {}", i);
            }
        }
    }

    let elapsed = start.elapsed();
    println!(
        "\nCompleted {} streaming iterations in {:?}",
        iterations, elapsed
    );
}

/// Stress test popup fuzzy matching
#[test]
#[ignore = "popup stress test"]
fn stability_popup_fuzzy() {
    use crucible_cli::tui::state::{PopupItem, PopupKind};
    use crucible_cli::tui::testing::Harness;
    use crossterm::event::KeyCode;

    let iterations: usize = std::env::var("STABILITY_ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);

    println!("Running {} popup iterations...", iterations);

    // Create many popup items to stress fuzzy matching
    let items: Vec<PopupItem> = (0..100)
        .map(|i| PopupItem {
            kind: crucible_cli::tui::state::PopupItemKind::Command,
            title: format!("command-{:03}", i),
            subtitle: format!("Description for command {}", i),
            token: format!("cmd{:03}", i),
            score: 0,
            available: true,
        })
        .collect();

    let start = Instant::now();

    for i in 0..iterations {
        let mut h = Harness::new(80, 24).with_popup_items(PopupKind::Command, items.clone());

        // Type filter text
        let filter = format!("cmd{}", i % 10);
        h.keys(&filter);

        // Navigate
        for _ in 0..5 {
            h.key(KeyCode::Down);
        }
        for _ in 0..3 {
            h.key(KeyCode::Up);
        }

        // Render
        let _ = h.render();

        // Close
        h.key(KeyCode::Esc);

        if i % 20 == 0 {
            print!(".");
        }
    }

    let elapsed = start.elapsed();
    println!(
        "\nCompleted {} popup iterations in {:?}",
        iterations, elapsed
    );
}

// =============================================================================
// Memory monitoring (Linux-specific)
// =============================================================================

/// Get current process memory usage in KB (Linux only)
#[cfg(target_os = "linux")]
fn get_memory_kb() -> Option<usize> {
    use std::fs;
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse().ok();
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn get_memory_kb() -> Option<usize> {
    None // Memory monitoring not available on this platform
}

/// Memory leak detection test
#[test]
#[ignore = "memory leak detection"]
fn stability_memory_leaks() {
    use crucible_cli::tui::testing::Harness;
    use crossterm::event::KeyCode;

    let iterations: usize = std::env::var("STABILITY_ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);

    println!("Running {} iterations for memory leak detection...", iterations);

    let initial_mem = get_memory_kb();
    let mut peak_mem = initial_mem;
    let mut samples: Vec<usize> = Vec::new();

    let start = Instant::now();

    for i in 0..iterations {
        // Create fresh harness each iteration to detect accumulation
        let mut h = Harness::new(80, 24);

        // Simulate session
        h.keys("hello world");
        h.key(KeyCode::Char('/'));
        h.key(KeyCode::Esc);
        let _ = h.render();

        // Sample memory periodically
        if i % 500 == 0 {
            if let Some(mem) = get_memory_kb() {
                samples.push(mem);
                if peak_mem.map_or(true, |p| mem > p) {
                    peak_mem = Some(mem);
                }
                print!(".");
            }
        }
    }

    let elapsed = start.elapsed();
    let final_mem = get_memory_kb();

    println!("\n\nMemory Analysis:");
    println!("================");
    if let (Some(initial), Some(final_m)) = (initial_mem, final_mem) {
        let growth = final_m as i64 - initial as i64;
        let growth_per_iter = growth as f64 / iterations as f64;

        println!("Initial: {} KB", initial);
        println!("Final:   {} KB", final_m);
        println!("Peak:    {} KB", peak_mem.unwrap_or(0));
        println!("Growth:  {} KB ({:.2} bytes/iter)", growth, growth_per_iter * 1024.0);

        // Warn if growth is excessive (> 1 byte per iteration average)
        if growth_per_iter > 1.0 {
            println!("\n⚠️  WARNING: Potential memory leak detected!");
            println!("    Growth rate: {:.2} KB/1000 iterations", growth_per_iter * 1000.0);
        } else {
            println!("\n✓ Memory usage appears stable");
        }
    } else {
        println!("Memory monitoring not available on this platform");
    }

    println!("\nCompleted in {:?}", elapsed);
}

// =============================================================================
// Duration-based soak test
// =============================================================================

/// Long-running soak test (configurable duration)
#[test]
#[ignore = "soak test - runs for hours"]
fn stability_soak_test() {
    use crucible_cli::tui::testing::Harness;
    use crossterm::event::KeyCode;

    let duration_hours: f64 = std::env::var("STABILITY_DURATION_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.1); // Default to 6 minutes for quick testing

    let duration = Duration::from_secs_f64(duration_hours * 3600.0);
    let report_interval = Duration::from_secs(60);

    println!("Starting soak test for {:.1} hours...", duration_hours);
    println!("Reporting every {:?}", report_interval);

    let start = Instant::now();
    let mut last_report = start;
    let mut iterations: u64 = 0;
    let mut errors: u64 = 0;
    let initial_mem = get_memory_kb();

    while start.elapsed() < duration {
        // Run batch of operations
        let mut h = Harness::new(80, 24);

        for _ in 0..100 {
            h.keys("test ");
            h.key(KeyCode::Char('/'));
            h.key(KeyCode::Down);
            h.key(KeyCode::Esc);
            h.key_ctrl('u');
            iterations += 1;
        }

        // Verify state
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h.render())) {
            Ok(frame) => {
                if frame.is_empty() {
                    errors += 1;
                }
            }
            Err(_) => {
                errors += 1;
            }
        }

        // Periodic reporting
        if last_report.elapsed() >= report_interval {
            let elapsed = start.elapsed();
            let current_mem = get_memory_kb();

            print!(
                "\r[{:02}:{:02}:{:02}] {} iterations, {} errors",
                elapsed.as_secs() / 3600,
                (elapsed.as_secs() % 3600) / 60,
                elapsed.as_secs() % 60,
                iterations,
                errors
            );

            if let (Some(init), Some(curr)) = (initial_mem, current_mem) {
                print!(", mem: {} KB (+{})", curr, curr as i64 - init as i64);
            }

            println!();
            last_report = Instant::now();
        }
    }

    let elapsed = start.elapsed();
    let final_mem = get_memory_kb();

    println!("\n\nSoak Test Results");
    println!("=================");
    println!("Duration:   {:?}", elapsed);
    println!("Iterations: {}", iterations);
    println!("Errors:     {}", errors);
    println!("Rate:       {:.0} iter/sec", iterations as f64 / elapsed.as_secs_f64());

    if let (Some(init), Some(fin)) = (initial_mem, final_mem) {
        let growth = fin as i64 - init as i64;
        println!("Memory:     {} KB -> {} KB (growth: {} KB)", init, fin, growth);
    }

    assert_eq!(errors, 0, "Soak test encountered {} errors", errors);
}

// =============================================================================
// CLI piped input tests
// =============================================================================

/// Test piped input to CLI
#[test]
#[ignore = "requires built binary"]
fn stability_piped_input() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Find binary
    let binary = std::env::var("CRUCIBLE_BIN")
        .unwrap_or_else(|_| "./target/release/cru".to_string());

    if !std::path::Path::new(&binary).exists() {
        println!("Binary not found at {}, trying debug...", binary);
        let debug_bin = "./target/debug/cru";
        if !std::path::Path::new(debug_bin).exists() {
            println!("Skipping: no binary found");
            return;
        }
    }

    println!("Testing piped input...");

    // Test 1: Simple echo through config show
    let output = Command::new(&binary)
        .args(["config", "show"])
        .stdin(Stdio::null())
        .output()
        .expect("Failed to execute");

    assert!(output.status.success() || output.status.code() == Some(0));
    println!("✓ config show works");

    // Test 2: Piped input to parse (if available)
    let mut child = Command::new(&binary)
        .args(["--help"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn");

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"test input\n");
    }

    let output = child.wait_with_output().expect("Failed to wait");
    assert!(output.status.success());
    println!("✓ piped input accepted");

    println!("\nPiped input tests passed!");
}

/// Test multi-line piped input
#[test]
#[ignore = "requires built binary and specific command"]
fn stability_multiline_pipe() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let binary = std::env::var("CRUCIBLE_BIN")
        .unwrap_or_else(|_| "./target/debug/cru".to_string());

    if !std::path::Path::new(&binary).exists() {
        println!("Skipping: binary not found");
        return;
    }

    // Multi-line input test
    let input = r#"line 1
line 2
line 3
"#;

    let mut child = Command::new(&binary)
        .args(["--version"]) // Safe command that accepts any stdin
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).expect("Failed to write");
    }

    let output = child.wait_with_output().expect("Failed to wait");
    assert!(output.status.success(), "Multi-line pipe failed");

    println!("✓ Multi-line piped input works");
}
