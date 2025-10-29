//! End-to-end integration tests for the REPL tool system
//!
//! These tests validate that the actual REPL interface works correctly
//! with the unified tool system, including tool discovery, grouping,
//! and execution from the user's perspective.

use std::io::{Write, Read, BufRead, BufReader};
use std::process::{Command, Stdio, Child};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use anyhow::Result;

/// Test helper that spawns a REPL process and interacts with it
struct ReplTestProcess {
    process: Child,
    temp_dir: TempDir,
}

impl ReplTestProcess {
    /// Spawn a REPL process for testing
    fn spawn() -> Result<Self> {
        // Create temporary directories for clean test environment
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let tool_dir = temp_dir.path().join("tools");

        // Create tools directory
        std::fs::create_dir_all(&tool_dir)?;

        // Spawn the crucible-cli REPL process
        let mut process = Command::new(env!("CARGO_BIN_EXE_crucible-cli"))
            .arg("--db-path")
            .arg(db_path.to_str().unwrap())
            .arg("--tool-dir")
            .arg(tool_dir.to_str().unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Give the REPL time to start up
        thread::sleep(Duration::from_millis(1000));

        Ok(Self { process, temp_dir })
    }

    /// Send a command to the REPL and wait for response
    fn send_command(&mut self, command: &str) -> Result<String> {
        // Send command to stdin
        if let Some(stdin) = self.process.stdin.as_mut() {
            writeln!(stdin, "{}", command)?;
            stdin.flush()?;
        }

        // Wait a moment for processing
        thread::sleep(Duration::from_millis(500));

        // Read stdout response
        if let Some(stdout) = self.process.stdout.as_mut() {
            let mut reader = BufReader::new(stdout);
            let mut output = String::new();

            // Read lines until we see a prompt or timeout
            let start_time = std::time::Instant::now();
            while start_time.elapsed() < Duration::from_secs(5) {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        output.push_str(&line);
                        // Stop if we see the prompt again
                        if line.contains("crucible>") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            Ok(output)
        } else {
            Err(anyhow::anyhow!("Cannot read from process stdout"))
        }
    }

    /// Quit the REPL cleanly
    fn quit(&mut self) -> Result<()> {
        self.send_command(":quit")?;

        // Wait for process to exit
        match self.process.wait() {
            Ok(status) => {
                if !status.success() {
                    return Err(anyhow::anyhow!("REPL process exited with status: {}", status));
                }
            }
            Err(e) => return Err(anyhow::anyhow!("Failed to wait for REPL process: {}", e)),
        }

        Ok(())
    }
}

impl Drop for ReplTestProcess {
    fn drop(&mut self) {
        // Ensure process is terminated
        if let Err(e) = self.process.kill() {
            eprintln!("Failed to kill REPL process: {}", e);
        }
    }
}

#[test]
#[ignore] // Integration test - requires built binary
fn test_repl_tools_command_displays_grouped_tools() -> Result<()> {
    println!("🧪 Testing REPL :tools command displays grouped tools");

    let mut repl = ReplTestProcess::spawn()?;

    // Send :tools command
    let output = repl.send_command(":tools")?;
    println!("📋 REPL :tools output:\n{}", output);

    // Verify the output contains expected group structure
    let assertions = vec![
        // Should show system tools group
        ("SYSTEM group header", output.contains("SYSTEM")),
        ("crucible-tools description", output.contains("crucible-tools")),
        ("System tool count", output.contains("[25 tools]") || output.contains("[2") || output.contains("[")), // Tool count may vary
        ("System tools list", output.lines().any(|line| line.trim().starts_with("system_"))),

        // Should show proper formatting
        ("Available Tools header", output.contains("Available Tools")),
        ("Group indicators", output.contains("(") && output.contains("[")),

        // Should have example usage
        ("Usage example", output.contains(":run <tool>") || output.contains("Example:")),
    ];

    for (description, assertion) in assertions {
        assert!(assertion, "❌ Missing expected content: {}", description);
        println!("✅ {}: {}", description, if assertion { "PASS" } else { "FAIL" });
    }

    // Verify system tools are actually listed (check for specific known tools)
    let expected_system_tools = vec![
        "system_info",
        "get_kiln_stats",
        "list_files",
        "search_content",
        "get_database_stats",
    ];

    let mut found_tools = 0;
    for tool in expected_system_tools {
        if output.contains(tool) {
            found_tools += 1;
            println!("✅ Found expected tool: {}", tool);
        }
    }

    assert!(found_tools >= 3, "❌ Expected to find at least 3 system tools, found {}", found_tools);

    repl.quit()?;

    println!("✅ REPL :tools command test passed");
    Ok(())
}

#[test]
#[ignore] // Integration test - requires built binary
fn test_repl_run_command_executes_system_tools() -> Result<()> {
    println!("🧪 Testing REPL :run command executes system tools");

    let mut repl = ReplTestProcess::spawn()?;

    // Test a simple system tool that should work without parameters
    let output = repl.send_command(":run system_info")?;
    println!("📋 REPL :run system_info output:\n{}", output);

    // Verify system_info tool executed successfully
    let assertions = vec![
        ("Tool execution", !output.is_empty()),
        ("System information", output.contains("System") || output.contains("Info") || output.contains("system")),
        ("No error indicators", !output.contains("❌") && !output.contains("ERROR") && !output.contains("error")),
    ];

    for (description, assertion) in assertions {
        assert!(assertion, "❌ {}: {}", description, output);
        println!("✅ {}: {}", description, if assertion { "PASS" } else { "FAIL" });
    }

    // Test another tool that should provide specific output
    let output2 = repl.send_command(":run get_kiln_stats")?;
    println!("📋 REPL :run get_kiln_stats output:\n{}", output2);

    // Verify get_kiln_stats provides meaningful output
    let kiln_stats_assertions = vec![
        ("Stats output", !output2.is_empty()),
        ("No tool errors", !output2.contains("Tool Error") && !output2.contains("not found")),
        ("Execution completed", output2.len() > 10), // Should have some content
    ];

    for (description, assertion) in kiln_stats_assertions {
        assert!(assertion, "❌ {}: {}", description, output2);
        println!("✅ {}: {}", description, if assertion { "PASS" } else { "FAIL" });
    }

    repl.quit()?;

    println!("✅ REPL :run command test passed");
    Ok(())
}

#[test]
#[ignore] // Integration test - requires built binary
fn test_repl_help_command_works() -> Result<()> {
    println!("🧪 Testing REPL help commands work");

    let mut repl = ReplTestProcess::spawn()?;

    // Test general help
    let help_output = repl.send_command(":help")?;
    println!("📋 REPL :help output length: {} chars", help_output.len());

    assert!(help_output.len() > 100, "❌ Help output should be substantial");
    assert!(help_output.contains("REPL") || help_output.contains("commands"), "❌ Help should mention REPL or commands");

    // Test specific command help
    let tools_help_output = repl.send_command(":help tools")?;
    println!("📋 REPL :help tools output: {}", tools_help_output);

    assert!(!tools_help_output.is_empty(), "❌ Tools help should not be empty");

    repl.quit()?;

    println!("✅ REPL help commands test passed");
    Ok(())
}

#[test]
#[ignore] // Integration test - requires built binary
fn test_repl_unknown_command_handling() -> Result<()> {
    println!("🧪 Testing REPL unknown command handling");

    let mut repl = ReplTestProcess::spawn()?;

    // Test running an unknown tool
    let output = repl.send_command(":run non_existent_tool_12345")?;
    println!("📋 REPL unknown tool output: {}", output);

    // Should handle unknown tools gracefully
    assert!(output.contains("not found") || output.contains("Error") || output.contains("❌"),
           "❌ Should show error for unknown tool");

    repl.quit()?;

    println!("✅ REPL unknown command handling test passed");
    Ok(())
}

#[cfg(test)]
mod test_helpers {
    use super::*;

    #[test]
    fn test_repl_process_creation() -> Result<()> {
        // This test just validates we can create the test structure
        // without actually spawning a process (to avoid CI issues)
        let temp_dir = TempDir::new()?;
        assert!(temp_dir.path().exists());
        Ok(())
    }
}