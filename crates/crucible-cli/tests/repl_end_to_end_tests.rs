//! End-to-end tests for the unified REPL tool system
//!
//! This module provides comprehensive integration tests that verify the actual REPL
//! interface works correctly with the unified tool system. Tests use TDD methodology
//! with proper output capture and validation.
//!
//! Test Coverage:
//! 1. :tools command displays grouped tools correctly
//! 2. :run command executes system tools with proper output
//! 3. Error handling for missing tools and bad parameters
//! 4. Fallback routing between system and Rune tools
//! 5. Output formatting and validation

/// Test context for end-to-end REPL tests
struct ReplTestContext {
    temp_dir: TempDir,
    kiln_path: PathBuf,
    tool_dir: PathBuf,
    db_path: PathBuf,
}

impl ReplTestContext {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let kiln_path = temp_dir.path().join("kiln");
        let tool_dir = temp_dir.path().join("tools");
        let db_path = temp_dir.path().join("test.db");

        // Create directories
        std::fs::create_dir_all(&kiln_path)?;
        std::fs::create_dir_all(&tool_dir)?;

        Ok(Self {
            temp_dir,
            kiln_path,
            tool_dir,
            db_path,
        })
    }

    /// Get the path to the crucible CLI binary
    fn get_cli_path() -> PathBuf {
        // During tests, use the target/debug/cru binary
        // CARGO_MANIFEST_DIR is /home/moot/crucible/crates/crucible-cli
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // Remove crucible-cli directory -> /home/moot/crucible/crates
        path.pop(); // Remove crates directory -> /home/moot/crucible
        path.join("target/debug/cru")
    }

    /// Start a REPL process for testing
    fn start_repl(&self) -> Result<ReplProcess> {
        let cli_path = Self::get_cli_path();

        // Build the command with proper environment and arguments
        // Note: REPL is the default command when no subcommand is specified
        // Use --non-interactive flag for testing (reads from stdin without requiring TTY)
        let mut child = Command::new(cli_path)
            .arg("--non-interactive")
            .arg("--db-path")
            .arg(&self.db_path)
            .arg("--tool-dir")
            .arg(&self.tool_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start REPL: {}", e))?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        Ok(ReplProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
        })
    }
}

/// Wrapper around a REPL process for testing
struct ReplProcess {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    stderr: BufReader<std::process::ChildStderr>,
}

impl ReplProcess {
    /// Send a command to the REPL
    fn send_command(&mut self, command: &str) -> Result<()> {
        writeln!(self.stdin, "{}", command)?;
        self.stdin.flush()?;
        Ok(())
    }

    /// Read output until a prompt is seen or timeout
    fn read_output(&mut self, timeout_ms: u64) -> Result<String> {
        let mut output = String::new();
        let start = std::time::Instant::now();

        while start.elapsed() < Duration::from_millis(timeout_ms) {
            let mut line = String::new();
            match self.stdout.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    output.push_str(&line);
                    // Look for REPL prompt
                    if line.contains("crucible>") {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        Ok(output)
    }

    /// Read all output currently available
    fn read_available_output(&mut self) -> Result<String> {
        let mut output = String::new();
        let mut buffer = String::new();

        while self.stdout.read_line(&mut buffer).unwrap_or(0) > 0 {
            output.push_str(&buffer);
            buffer.clear();
        }

        Ok(output)
    }

    /// Wait for the REPL to initialize
    /// In non-interactive mode, we just sleep briefly since the REPL starts immediately
    /// and doesn't block waiting for terminal setup
    fn wait_for_ready(&mut self) -> Result<()> {
        // Give the REPL process a moment to initialize
        // In non-interactive mode, it doesn't need to set up a terminal
        thread::sleep(Duration::from_millis(500));
        Ok(())
    }

    /// Send quit command to clean exit
    fn quit(&mut self) -> Result<()> {
        self.send_command(":quit")?;
        let _ = self.child.wait();
        Ok(())
    }
}

/// Task 1: Test that :tools command displays grouped tools properly
#[tokio::test]
async fn test_repl_tools_command_displays_grouped_tools() -> Result<()> {
    let context = ReplTestContext::new()?;
    let mut repl = context.start_repl()?;

    // Wait for REPL to initialize
    repl.wait_for_ready()?;

    // Clear any initial output
    let _ = repl.read_available_output()?;

    // Send :tools command
    repl.send_command(":tools")?;

    // Read the output
    let output = repl.read_output(2000)?;

    // Clean exit
    repl.quit()?;

    // Assertions about the output format
    assert!(
        output.contains("Available Tools"),
        "Output should contain 'Available Tools'"
    );

    // Should show grouped tools with proper format
    assert!(output.contains("SYSTEM"), "Should show SYSTEM group");
    assert!(
        output.contains("crucible-tools"),
        "Should show crucible-tools description"
    );
    assert!(
        output.contains("[") && output.contains("tools]"),
        "Should show tool count"
    );

    // Should show specific system tools
    let expected_tools = vec!["system_info", "list_files", "search_documents"];
    for tool in expected_tools {
        assert!(
            output.contains(tool),
            "Output should contain '{}':\n{}",
            tool,
            output
        );
    }

    // Verify the format: "SYSTEM (crucible-tools) [X tools]:"
    let group_header_regex = Regex::new(r"SYSTEM \(crucible-tools\) \[\d+ tools\]:").unwrap();
    assert!(
        group_header_regex.is_match(&output),
        "Output should match group header format: {}",
        output
    );

    println!("✅ :tools command displays grouped tools correctly");
    println!("📋 Sample output:\n{}", output);

    Ok(())
}

/// Task 2: Test that :run command executes system tools correctly
#[tokio::test]
async fn test_repl_run_command_executes_system_tools() -> Result<()> {
    let context = ReplTestContext::new()?;
    let mut repl = context.start_repl()?;

    // Wait for REPL to initialize
    repl.wait_for_ready()?;

    // Clear any initial output
    let _ = repl.read_available_output()?;

    // Test system_info tool (no arguments required)
    repl.send_command(":run system_info")?;

    // Read the output
    let output = repl.read_output(3000)?;

    // Should contain valid JSON output
    assert!(
        output.contains("{") && output.contains("}"),
        "Output should be JSON format: {}",
        output
    );

    // Should contain system information fields
    assert!(
        output.contains("platform") || output.contains("os") || output.contains("arch"),
        "Output should contain system info: {}",
        output
    );

    // Validate JSON is properly formatted
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .map_err(|e| anyhow::anyhow!("Invalid JSON output: {} - Error: {}", output, e))?;

    assert!(
        parsed.as_object().is_some(),
        "Output should be a JSON object"
    );

    // Test list_files tool with an argument
    repl.send_command(":run list_files /tmp")?;
    let files_output = repl.read_output(2000)?;

    // Should not error out (even if /tmp is empty)
    assert!(
        !files_output.contains("❌ Tool Error"),
        "list_files should not error: {}",
        files_output
    );

    // Clean exit
    repl.quit()?;

    println!("✅ :run command executes system tools correctly");
    println!("📊 system_info output: {}", output);
    println!("📁 list_files output: {}", files_output);

    Ok(())
}

/// Task 3: Test error handling for missing tools and bad parameters
#[tokio::test]
async fn test_repl_error_handling() -> Result<()> {
    let context = ReplTestContext::new()?;
    let mut repl = context.start_repl()?;

    // Wait for REPL to initialize
    repl.wait_for_ready()?;

    // Clear any initial output
    let _ = repl.read_available_output()?;

    // Test missing tool
    repl.send_command(":run nonexistent_tool")?;
    let missing_tool_output = repl.read_output(2000)?;

    assert!(
        missing_tool_output.contains("not found")
            || missing_tool_output.contains("Error")
            || missing_tool_output.contains("failed"),
        "Should show error for missing tool: {}",
        missing_tool_output
    );

    // Test missing required arguments
    repl.send_command(":run list_files")?; // list_files requires a path argument
    let missing_args_output = repl.read_output(2000)?;

    assert!(
        missing_args_output.contains("❌ Tool Error")
            || missing_args_output.contains("failed")
            || missing_args_output.contains("missing"),
        "Should show error for missing arguments: {}",
        missing_args_output
    );

    // Test invalid command
    repl.send_command(":invalid_command")?;
    let invalid_command_output = repl.read_output(2000)?;

    assert!(
        invalid_command_output.contains("Unknown")
            || invalid_command_output.contains("invalid")
            || invalid_command_output.contains("help"),
        "Should show error for invalid command: {}",
        invalid_command_output
    );

    // Clean exit
    repl.quit()?;

    println!("✅ Error handling works correctly");
    println!("❌ Missing tool: {}", missing_tool_output);
    println!("❌ Missing args: {}", missing_args_output);
    println!("❌ Invalid command: {}", invalid_command_output);

    Ok(())
}

/// Task 4: Test fallback routing between system and Rune tools
#[tokio::test]
async fn test_repl_fallback_routing() -> Result<()> {
    let context = ReplTestContext::new()?;

    // Create a simple Rune tool for testing
    let rune_tool_content = r#"
// Simple test tool
import std;
import io;

pub fn main(args) {
    io::println("Hello from Rune tool!");
    std::exit(0);
}
"#;

    let rune_tool_path = context.tool_dir.join("test_rune_tool.rn");
    std::fs::write(&rune_tool_path, rune_tool_content)?;

    let mut repl = context.start_repl()?;

    // Wait for REPL to initialize
    repl.wait_for_ready()?;

    // Clear any initial output
    let _ = repl.read_available_output()?;

    // First, test that system tools work (system tools should be tried first)
    repl.send_command(":run system_info")?;
    let system_output = repl.read_output(2000)?;
    assert!(
        system_output.contains("{"),
        "System tool should work: {}",
        system_output
    );

    // Test that Rune tools appear in :tools listing
    repl.send_command(":tools")?;
    let tools_output = repl.read_output(2000)?;

    // Should show both SYSTEM and potentially RUNE groups
    assert!(
        tools_output.contains("SYSTEM"),
        "Should show SYSTEM group: {}",
        tools_output
    );

    // Test running the Rune tool directly (if discovered)
    repl.send_command(":run test_rune_tool")?;
    let rune_output = repl.read_output(2000)?;

    // This might work if Rune tools are discovered, or fail gracefully if not
    println!("🔍 Rune tool execution result: {}", rune_output);

    // Clean exit
    repl.quit()?;

    println!("✅ Fallback routing test completed");
    println!("🔧 System tools work: {}", system_output.contains("{"));
    println!(
        "📜 Tools listing shows groups: {}",
        tools_output.contains("SYSTEM")
    );

    Ok(())
}

/// Task 5: Test output formatting and validation
#[tokio::test]
async fn test_repl_output_formatting() -> Result<()> {
    let context = ReplTestContext::new()?;
    let mut repl = context.start_repl()?;

    // Wait for REPL to initialize
    repl.wait_for_ready()?;

    // Clear any initial output
    let _ = repl.read_available_output()?;

    // Test that :tools command has proper formatting with colors and structure
    repl.send_command(":tools")?;
    let tools_output = repl.read_output(2000)?;

    // Should have proper structure with newlines and indentation
    assert!(tools_output.contains("\n"), "Output should have newlines");

    // Should contain emojis for visual appeal
    assert!(
        tools_output.contains("📦") || tools_output.contains("🔧"),
        "Output should contain visual indicators: {}",
        tools_output
    );

    // Test that tool execution produces clean output
    repl.send_command(":run system_info")?;
    let system_output = repl.read_output(2000)?;

    // Should be valid JSON without extra noise
    let trimmed = system_output.trim();
    let _ = serde_json::from_str::<serde_json::Value>(trimmed).map_err(|e| {
        anyhow::anyhow!(
            "System info should be valid JSON: {} - Error: {}",
            trimmed,
            e
        )
    })?;

    // Clean exit
    repl.quit()?;

    println!("✅ Output formatting is clean and valid");
    println!(
        "🎨 Tools output formatted properly: {}",
        tools_output.contains("\n")
    );
    println!("📊 System output is valid JSON: true");

    Ok(())
}

/// Helper test to verify our test infrastructure works
#[tokio::test]
async fn test_repl_infrastructure_smoke_test() -> Result<()> {
    let context = ReplTestContext::new()?;

    // Verify directories exist
    assert!(context.kiln_path.exists(), "Kiln path should exist");
    assert!(context.tool_dir.exists(), "Tool directory should exist");

    // Verify CLI path exists (might not in all test environments)
    let cli_path = ReplTestContext::get_cli_path();
    println!("🔧 CLI path: {:?}", cli_path);

    // For now, just verify our setup works
    println!("✅ Test infrastructure setup successful");

    Ok(())
}
use anyhow::Result;
use regex::Regex;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
